//! VirtioNet MMIO device state machine and queue processing.

use alloc::{sync::Arc, vec, vec::Vec};
use core::sync::atomic::{AtomicBool, Ordering};

use ax_errno::{AxResult, ax_err};
use ax_kspin::SpinNoIrq as Mutex;
use axdevice_base::{AccessWidth, BaseDeviceOps, EmuDeviceType, GuestPhysAddr, GuestPhysAddrRange};
use axvm_types::EmulatedDeviceType;

use super::{
    backend::{LoopbackBackend, NetPortBackend},
    queue::{GuestDma, VirtQueue},
    regs::*,
};

struct DeviceInner {
    base: usize,
    length: usize,
    mac: [u8; 6],
    irq_id: usize,
    port_id: Option<super::PortId>,
    status: u32,
    device_features_sel: u32,
    driver_features_sel: u32,
    driver_features: u64,
    queue_sel: u32,
    queues: [VirtQueue; QUEUE_COUNT],
    interrupt_status: u32,
    pending_notify: bool,
    backend: Arc<dyn NetPortBackend>,
    /// Scratch for TX/RX frame assembly.
    frame_buf: Vec<u8>,
}

/// Emulated Virtio MMIO net device.
pub struct VirtioNetDevice {
    inner: Mutex<DeviceInner>,
    /// Set by a remote TX path; drained on the owning VM's vCPU so guest DMA
    /// writes run on the same pCPU that will resume the guest (cache-coherent).
    pending_rx_kick: AtomicBool,
}

impl VirtioNetDevice {
    /// Creates a device covering `[base, base+length)` with the given MAC.
    pub fn new(
        base: usize,
        length: usize,
        mac: [u8; 6],
        irq_id: usize,
        backend: Arc<dyn NetPortBackend>,
    ) -> Self {
        Self {
            inner: Mutex::new(DeviceInner {
                base,
                length,
                mac,
                irq_id,
                port_id: None,
                status: 0,
                device_features_sel: 0,
                driver_features_sel: 0,
                driver_features: 0,
                queue_sel: 0,
                queues: [VirtQueue::default(); QUEUE_COUNT],
                interrupt_status: 0,
                pending_notify: false,
                backend,
                frame_buf: vec![0u8; 2048],
            }),
            pending_rx_kick: AtomicBool::new(false),
        }
    }

    /// Loopback convenience constructor.
    pub fn with_loopback(base: usize, length: usize, mac: [u8; 6], irq_id: usize) -> Self {
        Self::new(base, length, mac, irq_id, Arc::new(LoopbackBackend::new()))
    }

    /// Sets the optional L2 switch port id for peer kick.
    pub fn set_port_id(&self, port_id: Option<super::PortId>) {
        self.inner.lock().port_id = port_id;
    }

    /// Returns the L2 switch port id, if attached.
    pub fn port_id(&self) -> Option<super::PortId> {
        self.inner.lock().port_id
    }

    /// Guest-visible IRQ line id (GIC INTID for aarch64 SPI).
    pub fn irq_id(&self) -> usize {
        self.inner.lock().irq_id
    }

    /// Returns whether QUEUE_NOTIFY left work that needs guest DMA processing.
    pub fn take_pending_notify(&self) -> bool {
        let mut inner = self.inner.lock();
        let pending = inner.pending_notify;
        inner.pending_notify = false;
        pending
    }

    /// Requests RX queue processing on the device owner's vCPU.
    pub fn request_rx_kick(&self) {
        self.pending_rx_kick.store(true, Ordering::Release);
    }

    /// Clears and returns whether a remote peer requested RX processing.
    pub fn take_rx_kick(&self) -> bool {
        self.pending_rx_kick.swap(false, Ordering::AcqRel)
    }

    /// Returns whether the guest has not yet acknowledged a queue interrupt.
    pub fn interrupt_pending(&self) -> bool {
        self.inner.lock().interrupt_status != 0
    }

    /// Processes TX then RX using guest DMA callbacks (fw_cfg-style).
    ///
    /// Returns whether any queue work completed (used for IRQ injection).
    pub fn process_queues<R, W>(&self, mut read: R, mut write: W) -> AxResult<bool>
    where
        R: FnMut(GuestPhysAddr, &mut [u8]) -> AxResult,
        W: FnMut(GuestPhysAddr, &[u8]) -> AxResult,
    {
        let (tx, rx) = self.process_queues_split(&mut read, &mut write)?;
        Ok(tx || rx)
    }

    /// Like [`process_queues`], but reports TX and RX completion separately.
    pub fn process_queues_split<R, W>(&self, read: &mut R, write: &mut W) -> AxResult<(bool, bool)>
    where
        R: FnMut(GuestPhysAddr, &mut [u8]) -> AxResult,
        W: FnMut(GuestPhysAddr, &[u8]) -> AxResult,
    {
        struct ClosureDma<'a, R, W> {
            read: &'a mut R,
            write: &'a mut W,
        }
        impl<R, W> GuestDma for ClosureDma<'_, R, W>
        where
            R: FnMut(GuestPhysAddr, &mut [u8]) -> AxResult,
            W: FnMut(GuestPhysAddr, &[u8]) -> AxResult,
        {
            fn read(&mut self, gpa: GuestPhysAddr, buf: &mut [u8]) -> AxResult {
                (self.read)(gpa, buf)
            }
            fn write(&mut self, gpa: GuestPhysAddr, buf: &[u8]) -> AxResult {
                (self.write)(gpa, buf)
            }
        }

        let mut dma = ClosureDma { read, write };
        let mut inner = self.inner.lock();
        if inner.status & STATUS_DRIVER_OK == 0 {
            return Ok((false, false));
        }
        let tx_raised = Self::process_tx(&mut inner, &mut dma)?;
        let rx_raised = Self::process_rx(&mut inner, &mut dma)?;
        if tx_raised || rx_raised {
            inner.interrupt_status |= ISR_QUEUE;
        }
        Ok((tx_raised, rx_raised))
    }

    fn process_tx<D: GuestDma>(inner: &mut DeviceInner, dma: &mut D) -> AxResult<bool> {
        let q = inner.queues[QUEUE_TX as usize];
        if !q.ready || q.num == 0 {
            return Ok(false);
        }
        let mut raised = false;
        let avail_idx = q.avail_idx(dma)?;
        while inner.queues[QUEUE_TX as usize].last_avail_idx != avail_idx {
            let last = inner.queues[QUEUE_TX as usize].last_avail_idx;
            let ring_slot = last % (q.num as u16);
            let head = q.avail_ring(dma, ring_slot)?;
            let (_, len, _) = q.read_chain_to(dma, head, &mut inner.frame_buf)?;
            if len >= NET_HDR_LEN {
                let frame = &inner.frame_buf[NET_HDR_LEN..len];
                inner.backend.transmit(frame);
            }
            let used_idx = q.used_idx(dma)?;
            q.write_used(dma, used_idx, head, len as u32)?;
            inner.queues[QUEUE_TX as usize].last_avail_idx = last.wrapping_add(1);
            raised = true;
        }
        Ok(raised)
    }

    fn process_rx<D: GuestDma>(inner: &mut DeviceInner, dma: &mut D) -> AxResult<bool> {
        let q = inner.queues[QUEUE_RX as usize];
        if !q.ready || q.num == 0 {
            return Ok(false);
        }
        let mut raised = false;
        // Assemble virtio-net hdr + ethernet frame in frame_buf.
        let mut packet = vec![0u8; 2048];
        loop {
            let avail_idx = q.avail_idx(dma)?;
            let last = inner.queues[QUEUE_RX as usize].last_avail_idx;
            if last == avail_idx {
                // No guest RX buffer yet; leave frames queued on the backend.
                break;
            }

            let eth_len = {
                let room = packet.len().saturating_sub(NET_HDR_LEN);
                if room == 0 {
                    break;
                }
                match inner
                    .backend
                    .try_receive(&mut packet[NET_HDR_LEN..NET_HDR_LEN + room])
                {
                    Some(n) => n,
                    None => break,
                }
            };
            // Zero virtio_net_hdr_v1; num_buffers=1 (one eth frame per RX buffer).
            packet[..NET_HDR_LEN].fill(0);
            packet[10] = 1;
            packet[11] = 0;
            // Ethernet minimum without FCS is 60 bytes; short UDP/ARP frames from
            // a peer guest may be only 42–52 bytes and get dropped by Linux if
            // delivered unpadded.
            let eth_len = if eth_len < 60 {
                if NET_HDR_LEN + 60 > packet.len() {
                    break;
                }
                packet[NET_HDR_LEN + eth_len..NET_HDR_LEN + 60].fill(0);
                60
            } else {
                eth_len
            };
            let total = NET_HDR_LEN + eth_len;

            let ring_slot = last % (q.num as u16);
            let head = q.avail_ring(dma, ring_slot)?;
            let written = q.write_chain_from(dma, head, &packet[..total])?;
            let used_idx = q.used_idx(dma)?;
            q.write_used(dma, used_idx, head, written)?;
            inner.queues[QUEUE_RX as usize].last_avail_idx = last.wrapping_add(1);
            raised = true;
            if eth_len >= 14 {
                let et = u16::from_be_bytes([packet[NET_HDR_LEN + 12], packet[NET_HDR_LEN + 13]]);
                debug!(
                    "virtio-net RX: eth_len={eth_len} ethertype={et:#06x} irq={}",
                    inner.irq_id
                );
            }
        }
        Ok(raised)
    }

    fn reset(inner: &mut DeviceInner) {
        inner.status = 0;
        inner.device_features_sel = 0;
        inner.driver_features_sel = 0;
        inner.driver_features = 0;
        inner.queue_sel = 0;
        inner.queues = [VirtQueue::default(); QUEUE_COUNT];
        inner.interrupt_status = 0;
        inner.pending_notify = false;
    }

    fn selected_queue_mut(inner: &mut DeviceInner) -> AxResult<&mut VirtQueue> {
        let idx = inner.queue_sel as usize;
        if idx >= QUEUE_COUNT {
            return ax_err!(InvalidInput, "invalid virtqueue index");
        }
        Ok(&mut inner.queues[idx])
    }

    fn read_config(inner: &DeviceInner, offset: usize, width: AccessWidth) -> AxResult<usize> {
        // Config layout: mac[6] at 0, status u16 at 6 (VIRTIO_NET_F_STATUS).
        let mut cfg = [0u8; 8];
        cfg[..6].copy_from_slice(&inner.mac);
        // Link up.
        cfg[6] = 1;
        cfg[7] = 0;
        if offset >= cfg.len() {
            return Ok(0);
        }
        let end = (offset + width.size()).min(cfg.len());
        let mut val = 0usize;
        for (i, b) in cfg[offset..end].iter().enumerate() {
            val |= (*b as usize) << (8 * i);
        }
        Ok(val)
    }
}

impl BaseDeviceOps<GuestPhysAddrRange> for VirtioNetDevice {
    fn emu_type(&self) -> EmuDeviceType {
        EmulatedDeviceType::VirtioNet
    }

    fn address_range(&self) -> GuestPhysAddrRange {
        let inner = self.inner.lock();
        GuestPhysAddrRange::from_start_size(GuestPhysAddr::from_usize(inner.base), inner.length)
    }

    fn handle_read(&self, addr: GuestPhysAddr, width: AccessWidth) -> AxResult<usize> {
        let inner = self.inner.lock();
        let offset = addr.as_usize().wrapping_sub(inner.base);
        if offset >= OFF_CONFIG {
            return Self::read_config(&inner, offset - OFF_CONFIG, width);
        }
        let val = match offset {
            OFF_MAGIC => MAGIC,
            OFF_VERSION => VERSION,
            OFF_DEVICE_ID => DEVICE_ID_NET,
            OFF_VENDOR_ID => VENDOR_ID,
            OFF_DEVICE_FEATURES => {
                let shift = (inner.device_features_sel as u64) * 32;
                ((DEVICE_FEATURES >> shift) & 0xffff_ffff) as u32
            }
            OFF_DEVICE_FEATURES_SEL => inner.device_features_sel,
            OFF_DRIVER_FEATURES => {
                let shift = (inner.driver_features_sel as u64) * 32;
                ((inner.driver_features >> shift) & 0xffff_ffff) as u32
            }
            OFF_DRIVER_FEATURES_SEL => inner.driver_features_sel,
            OFF_QUEUE_SEL => inner.queue_sel,
            OFF_QUEUE_NUM_MAX => {
                if (inner.queue_sel as usize) < QUEUE_COUNT {
                    QUEUE_SIZE_MAX
                } else {
                    0
                }
            }
            OFF_QUEUE_NUM => {
                let q = &inner.queues[inner.queue_sel as usize % QUEUE_COUNT];
                q.num
            }
            OFF_QUEUE_READY => {
                let q = &inner.queues[inner.queue_sel as usize % QUEUE_COUNT];
                u32::from(q.ready)
            }
            OFF_INTERRUPT_STATUS => inner.interrupt_status,
            OFF_STATUS => inner.status,
            OFF_QUEUE_DESC_LOW => {
                (inner.queues[inner.queue_sel as usize % QUEUE_COUNT].desc & 0xffff_ffff) as u32
            }
            OFF_QUEUE_DESC_HIGH => {
                (inner.queues[inner.queue_sel as usize % QUEUE_COUNT].desc >> 32) as u32
            }
            OFF_QUEUE_AVAIL_LOW => {
                (inner.queues[inner.queue_sel as usize % QUEUE_COUNT].avail & 0xffff_ffff) as u32
            }
            OFF_QUEUE_AVAIL_HIGH => {
                (inner.queues[inner.queue_sel as usize % QUEUE_COUNT].avail >> 32) as u32
            }
            OFF_QUEUE_USED_LOW => {
                (inner.queues[inner.queue_sel as usize % QUEUE_COUNT].used & 0xffff_ffff) as u32
            }
            OFF_QUEUE_USED_HIGH => {
                (inner.queues[inner.queue_sel as usize % QUEUE_COUNT].used >> 32) as u32
            }
            OFF_CONFIG_GENERATION => 0,
            _ => 0,
        };
        Ok((val as usize) & width_mask(width))
    }

    fn handle_write(&self, addr: GuestPhysAddr, width: AccessWidth, val: usize) -> AxResult {
        let mut inner = self.inner.lock();
        let offset = addr.as_usize().wrapping_sub(inner.base);
        let v = (val as u32) & (width_mask(width) as u32);

        match offset {
            OFF_DEVICE_FEATURES_SEL => inner.device_features_sel = v,
            OFF_DRIVER_FEATURES_SEL => inner.driver_features_sel = v,
            OFF_DRIVER_FEATURES => {
                let shift = (inner.driver_features_sel as u64) * 32;
                let mask = 0xffff_ffff_u64 << shift;
                inner.driver_features =
                    (inner.driver_features & !mask) | ((u64::from(v) << shift) & mask);
            }
            OFF_QUEUE_SEL => inner.queue_sel = v,
            OFF_QUEUE_NUM => {
                let q = Self::selected_queue_mut(&mut inner)?;
                if v > 0 && v <= QUEUE_SIZE_MAX && v.is_power_of_two() {
                    q.num = v;
                }
            }
            OFF_QUEUE_READY => {
                let q = Self::selected_queue_mut(&mut inner)?;
                q.ready = v == 1;
                if !q.ready {
                    q.last_avail_idx = 0;
                }
            }
            OFF_QUEUE_NOTIFY => {
                inner.pending_notify = true;
            }
            OFF_INTERRUPT_ACK => {
                inner.interrupt_status &= !v;
            }
            OFF_STATUS => {
                if v == 0 {
                    Self::reset(&mut inner);
                } else {
                    inner.status = v;
                    if v & STATUS_FAILED != 0 {
                        warn!("virtio-net guest set FAILED status");
                    }
                }
            }
            OFF_QUEUE_DESC_LOW => {
                let q = Self::selected_queue_mut(&mut inner)?;
                q.desc = (q.desc & !0xffff_ffff) | u64::from(v);
            }
            OFF_QUEUE_DESC_HIGH => {
                let q = Self::selected_queue_mut(&mut inner)?;
                q.desc = (q.desc & 0xffff_ffff) | (u64::from(v) << 32);
            }
            OFF_QUEUE_AVAIL_LOW => {
                let q = Self::selected_queue_mut(&mut inner)?;
                q.avail = (q.avail & !0xffff_ffff) | u64::from(v);
            }
            OFF_QUEUE_AVAIL_HIGH => {
                let q = Self::selected_queue_mut(&mut inner)?;
                q.avail = (q.avail & 0xffff_ffff) | (u64::from(v) << 32);
            }
            OFF_QUEUE_USED_LOW => {
                let q = Self::selected_queue_mut(&mut inner)?;
                q.used = (q.used & !0xffff_ffff) | u64::from(v);
            }
            OFF_QUEUE_USED_HIGH => {
                let q = Self::selected_queue_mut(&mut inner)?;
                q.used = (q.used & 0xffff_ffff) | (u64::from(v) << 32);
            }
            _ => {}
        }
        Ok(())
    }
}

fn width_mask(width: AccessWidth) -> usize {
    match width.size() {
        1 => 0xff,
        2 => 0xffff,
        4 => 0xffff_ffff,
        _ => usize::MAX,
    }
}

#[cfg(test)]
mod tests {
    use axdevice_base::BaseDeviceOps;

    use super::*;

    #[test]
    fn mmio_identity_registers() {
        let net = VirtioNetDevice::with_loopback(0xa000000, 0x200, [2, 0, 0, 0, 0, 2], 48);
        let width = AccessWidth::Dword;
        assert_eq!(
            net.handle_read(GuestPhysAddr::from_usize(0xa000000), width)
                .unwrap(),
            MAGIC as usize
        );
        assert_eq!(
            net.handle_read(GuestPhysAddr::from_usize(0xa000004), width)
                .unwrap(),
            VERSION as usize
        );
        assert_eq!(
            net.handle_read(GuestPhysAddr::from_usize(0xa000008), width)
                .unwrap(),
            DEVICE_ID_NET as usize
        );
    }

    #[test]
    fn notify_sets_pending_flag() {
        let net = VirtioNetDevice::with_loopback(0xa000000, 0x200, [2, 0, 0, 0, 0, 2], 48);
        net.handle_write(GuestPhysAddr::from_usize(0xa000050), AccessWidth::Dword, 1)
            .unwrap();
        assert!(net.take_pending_notify());
        assert!(!net.take_pending_notify());
    }

    #[test]
    fn version1_net_hdr_includes_num_buffers() {
        // VIRTIO_F_VERSION_1 is always advertised; guests therefore use the
        // 12-byte virtio_net_hdr_v1 (legacy 10-byte hdr omits num_buffers).
        assert_ne!(DEVICE_FEATURES & VIRTIO_F_VERSION_1, 0);
        assert_eq!(NET_HDR_LEN, 12);
    }
}

//! Hypervisor L2 virtual switch for VirtioNet ports (Task 2).

use alloc::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
    vec::Vec,
};
use core::sync::atomic::{AtomicU32, Ordering};

use ax_kspin::SpinNoIrq as Mutex;

use super::backend::{MAX_FRAME_LEN, NetPortBackend};

/// Default max frames queued per port.
pub const DEFAULT_QUEUE_DEPTH: usize = 64;

/// icpc UDP destination port allow-listed by [`IcpcPortAcl`].
pub const ICPC_UDP_PORT: u16 = 9527;

static FAULT_DROP_EVERY: AtomicU32 = AtomicU32::new(0);

/// Enables deterministic L2 drop for CI fault-injection tests.
///
/// When `drop_every_n > 0`, roughly `1/drop_every_n` of icpc UDP frames are dropped
/// (ARP and non-icpc traffic pass). `0` disables injection.
pub fn configure_vsw_fault_inject(drop_every_n: u32) {
    FAULT_DROP_EVERY.store(drop_every_n, Ordering::Relaxed);
    if drop_every_n > 0 {
        info!("vsw fault inject: drop every {drop_every_n} forwarded frame(s)");
    }
}

fn icpc_frame_drop_hash(frame: &[u8]) -> u32 {
    let mut hash = 0x811c_9dc5u32;
    for &byte in frame {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

fn should_drop_forwarded_frame(frame: &[u8]) -> bool {
    let every = FAULT_DROP_EVERY.load(Ordering::Relaxed);
    if every == 0 {
        return false;
    }
    if frame.len() >= 14 {
        let ethertype = u16::from_be_bytes([frame[12], frame[13]]);
        // Keep ARP so guest peers can resolve MACs under injected loss.
        if ethertype == 0x0806 {
            return false;
        }
    }
    let (udp_src, udp_dst) = parse_udp_ports(frame);
    let is_icpc = matches!(
        (udp_src, udp_dst),
        (_, Some(ICPC_UDP_PORT)) | (Some(ICPC_UDP_PORT), _)
    );
    if !is_icpc {
        return false;
    }
    icpc_frame_drop_hash(frame) % every == 0
}

/// One switch port identifier (unique across the hypervisor).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PortId(pub u16);

/// Access-control decision for a forwarded frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AclAction {
    Allow,
    Drop,
}

/// Optional L2/L4 filter applied before enqueue.
pub trait FrameAcl: Send + Sync {
    fn decide(
        &self,
        src_mac: [u8; 6],
        dst_mac: [u8; 6],
        udp_src: Option<u16>,
        udp_dst: Option<u16>,
    ) -> AclAction;
}

/// Default ACL: allow ARP / non-UDP; for UDP allow only [`ICPC_UDP_PORT`].
pub struct IcpcPortAcl;

impl FrameAcl for IcpcPortAcl {
    fn decide(
        &self,
        _src_mac: [u8; 6],
        _dst_mac: [u8; 6],
        udp_src: Option<u16>,
        udp_dst: Option<u16>,
    ) -> AclAction {
        match (udp_src, udp_dst) {
            (None, None) => AclAction::Allow,
            (_, Some(ICPC_UDP_PORT)) | (Some(ICPC_UDP_PORT), _) => AclAction::Allow,
            (Some(_), Some(_)) => AclAction::Drop,
            _ => AclAction::Allow,
        }
    }
}

struct PortQueue {
    frames: VecDeque<Vec<u8>>,
    depth: usize,
}

impl PortQueue {
    fn new(depth: usize) -> Self {
        Self {
            frames: VecDeque::new(),
            depth,
        }
    }

    fn push(&mut self, frame: Vec<u8>) {
        if self.frames.len() >= self.depth {
            let _ = self.frames.pop_front();
        }
        self.frames.push_back(frame);
    }

    fn pop(&mut self) -> Option<Vec<u8>> {
        self.frames.pop_front()
    }
}

/// Minimal learning virtual switch shared by VirtioNet ports.
pub struct VirtualSwitch {
    mac_table: Mutex<BTreeMap<[u8; 6], PortId>>,
    ports: Mutex<BTreeMap<PortId, PortQueue>>,
    acl: Arc<dyn FrameAcl>,
    queue_depth: usize,
}

impl VirtualSwitch {
    /// Creates a switch with the given ACL and per-port queue depth.
    pub fn new(acl: Arc<dyn FrameAcl>, queue_depth: usize) -> Self {
        Self {
            mac_table: Mutex::new(BTreeMap::new()),
            ports: Mutex::new(BTreeMap::new()),
            acl,
            queue_depth: queue_depth.max(1),
        }
    }

    /// Registers a port (idempotent).
    pub fn add_port(&self, port: PortId) {
        self.ports
            .lock()
            .entry(port)
            .or_insert_with(|| PortQueue::new(self.queue_depth));
    }

    /// Learns `src_mac` on `ingress` and forwards `frame` according to dst MAC + ACL.
    ///
    /// Returns the destination port ids that received a copy of the frame.
    pub fn forward(&self, ingress: PortId, frame: &[u8]) -> Vec<PortId> {
        if frame.len() < 14 || frame.len() > MAX_FRAME_LEN {
            return Vec::new();
        }
        let mut dst = [0u8; 6];
        let mut src = [0u8; 6];
        dst.copy_from_slice(&frame[0..6]);
        src.copy_from_slice(&frame[6..12]);
        let (udp_src, udp_dst) = parse_udp_ports(frame);

        if self.acl.decide(src, dst, udp_src, udp_dst) == AclAction::Drop {
            return Vec::new();
        }

        if let (Some(us), Some(ud)) = (udp_src, udp_dst) {
            debug!(
                "vsw forward UDP {us}->{ud} ingress={} len={}",
                ingress.0,
                frame.len()
            );
        }

        self.mac_table.lock().insert(src, ingress);

        let targets: Vec<PortId> = {
            let table = self.mac_table.lock();
            if dst == [0xff; 6] {
                self.ports
                    .lock()
                    .keys()
                    .copied()
                    .filter(|p| *p != ingress)
                    .collect()
            } else if let Some(port) = table.get(&dst).copied() {
                if port == ingress {
                    Vec::new()
                } else {
                    alloc::vec![port]
                }
            } else {
                self.ports
                    .lock()
                    .keys()
                    .copied()
                    .filter(|p| *p != ingress)
                    .collect()
            }
        };

        let mut ports = self.ports.lock();
        for port in &targets {
            if should_drop_forwarded_frame(frame) {
                debug!("vsw fault inject: drop frame to port {}", port.0);
                continue;
            }
            if let Some(q) = ports.get_mut(port) {
                q.push(frame.to_vec());
            }
        }
        targets
    }

    /// Pops one frame for `port`, if any.
    pub fn try_receive(&self, port: PortId) -> Option<Vec<u8>> {
        self.ports.lock().get_mut(&port)?.pop()
    }

    /// Returns whether `port` has at least one queued frame.
    pub fn has_pending(&self, port: PortId) -> bool {
        self.ports
            .lock()
            .get(&port)
            .is_some_and(|q| !q.frames.is_empty())
    }
}

/// VirtioNet port attached to [`global_vsw`].
pub struct SwitchPortBackend {
    port: PortId,
    switch: Arc<VirtualSwitch>,
}

impl SwitchPortBackend {
    /// Creates a backend bound to `port` on `switch`.
    pub fn new(port: PortId, switch: Arc<VirtualSwitch>) -> Self {
        switch.add_port(port);
        Self { port, switch }
    }

    /// Returns this port id.
    pub fn port_id(&self) -> PortId {
        self.port
    }
}

impl NetPortBackend for SwitchPortBackend {
    fn transmit(&self, frame: &[u8]) {
        let _ = self.switch.forward(self.port, frame);
    }

    fn try_receive(&self, out: &mut [u8]) -> Option<usize> {
        let frame = self.switch.try_receive(self.port)?;
        if out.len() < frame.len() {
            // Drop oversized delivery rather than requeue under SpinNoIrq.
            return None;
        }
        out[..frame.len()].copy_from_slice(&frame);
        Some(frame.len())
    }

    fn has_pending_rx(&self) -> bool {
        self.switch.has_pending(self.port)
    }
}

static GLOBAL_VSW: Mutex<Option<Arc<VirtualSwitch>>> = Mutex::new(None);

/// Returns the process-wide virtual switch, creating it on first use.
pub fn global_vsw() -> Arc<VirtualSwitch> {
    let mut guard = GLOBAL_VSW.lock();
    if let Some(sw) = guard.as_ref() {
        return sw.clone();
    }
    let sw = Arc::new(VirtualSwitch::new(
        Arc::new(IcpcPortAcl),
        DEFAULT_QUEUE_DEPTH,
    ));
    *guard = Some(sw.clone());
    sw
}

fn parse_udp_ports(frame: &[u8]) -> (Option<u16>, Option<u16>) {
    if frame.len() < 14 + 20 + 8 {
        return (None, None);
    }
    let ethertype = u16::from_be_bytes([frame[12], frame[13]]);
    if ethertype != 0x0800 {
        return (None, None);
    }
    let ihl = (frame[14] & 0x0f) as usize * 4;
    if ihl < 20 || frame.len() < 14 + ihl + 8 {
        return (None, None);
    }
    if frame[14 + 9] != 17 {
        return (None, None);
    }
    let src_off = 14 + ihl;
    let dst_off = src_off + 2;
    let src = u16::from_be_bytes([frame[src_off], frame[src_off + 1]]);
    let dst = u16::from_be_bytes([frame[dst_off], frame[dst_off + 1]]);
    (Some(src), Some(dst))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learns_and_unicasts() {
        let sw = Arc::new(VirtualSwitch::new(Arc::new(IcpcPortAcl), 8));
        sw.add_port(PortId(1));
        sw.add_port(PortId(2));

        let mut frame = [0u8; 64];
        frame[0..6].copy_from_slice(&[0x02, 0, 0, 0, 0, 2]);
        frame[6..12].copy_from_slice(&[0x02, 0, 0, 0, 0, 1]);
        frame[12] = 0x08;
        frame[13] = 0x00;

        assert_eq!(sw.forward(PortId(1), &frame), alloc::vec![PortId(2)]);
        assert!(sw.try_receive(PortId(2)).is_some());

        let mut reply = frame;
        reply[0..6].copy_from_slice(&[0x02, 0, 0, 0, 0, 1]);
        reply[6..12].copy_from_slice(&[0x02, 0, 0, 0, 0, 2]);
        sw.forward(PortId(2), &reply);
        assert!(sw.try_receive(PortId(1)).is_some());

        sw.forward(PortId(1), &frame);
        assert!(sw.try_receive(PortId(2)).is_some());
        assert!(sw.try_receive(PortId(1)).is_none());
    }

    #[test]
    fn switch_port_backends_exchange_frames() {
        let sw = global_vsw();
        let a = SwitchPortBackend::new(PortId(10), sw.clone());
        let b = SwitchPortBackend::new(PortId(11), sw);

        let mut frame = [0u8; 64];
        frame[0..6].copy_from_slice(&[0x02, 0, 0, 0, 0, 11]);
        frame[6..12].copy_from_slice(&[0x02, 0, 0, 0, 0, 10]);
        frame[12] = 0x08;
        frame[13] = 0x06; // ARP — ACL allows
        a.transmit(&frame);

        let mut out = [0u8; 128];
        let n = b.try_receive(&mut out).expect("frame delivered to peer");
        assert_eq!(&out[..n], &frame[..]);
    }

    #[test]
    fn icpc_port_acl_drops_non_icpc_udp() {
        let sw = Arc::new(VirtualSwitch::new(Arc::new(IcpcPortAcl), 8));
        sw.add_port(PortId(1));
        sw.add_port(PortId(2));

        let mut udp = [0u8; 64];
        udp[0..6].copy_from_slice(&[0x02, 0, 0, 0, 0, 2]);
        udp[6..12].copy_from_slice(&[0x02, 0, 0, 0, 0, 1]);
        udp[12] = 0x08;
        udp[13] = 0x00;
        udp[14] = 0x45;
        udp[23] = 17;
        udp[34] = 0x12;
        udp[35] = 0x34;
        udp[36] = 0x30;
        udp[37] = 0x39; // dst port 12345

        assert!(sw.forward(PortId(1), &udp).is_empty());
        assert!(sw.try_receive(PortId(2)).is_none());
    }

    #[test]
    fn fault_inject_drops_some_icpc_udp_frames() {
        configure_vsw_fault_inject(2);
        let sw = Arc::new(VirtualSwitch::new(Arc::new(IcpcPortAcl), 8));
        sw.add_port(PortId(1));
        sw.add_port(PortId(2));

        let mut arp = [0u8; 64];
        arp[0..6].copy_from_slice(&[0x02, 0, 0, 0, 0, 2]);
        arp[6..12].copy_from_slice(&[0x02, 0, 0, 0, 0, 1]);
        arp[12] = 0x08;
        arp[13] = 0x06;
        sw.forward(PortId(1), &arp);
        sw.forward(PortId(1), &arp);
        assert!(sw.try_receive(PortId(2)).is_some());
        assert!(sw.try_receive(PortId(2)).is_some());

        let mut udp = [0u8; 64];
        udp[0..6].copy_from_slice(&[0x02, 0, 0, 0, 0, 2]);
        udp[6..12].copy_from_slice(&[0x02, 0, 0, 0, 0, 1]);
        udp[12] = 0x08;
        udp[13] = 0x00;
        udp[14] = 0x45;
        udp[23] = 17; // UDP
        udp[34] = 0x12;
        udp[35] = 0x34;
        udp[36] = (ICPC_UDP_PORT >> 8) as u8;
        udp[37] = (ICPC_UDP_PORT & 0xff) as u8;

        let mut delivered = 0usize;
        let mut dropped = 0usize;
        for seq in 0u8..32 {
            udp[38] = seq;
            sw.forward(PortId(1), &udp);
            if sw.try_receive(PortId(2)).is_some() {
                delivered += 1;
            } else {
                dropped += 1;
            }
        }
        assert!(delivered > 0);
        assert!(dropped > 0);
        configure_vsw_fault_inject(0);
    }
}

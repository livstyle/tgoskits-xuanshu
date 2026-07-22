//! Virtqueue descriptor walking helpers.

use ax_errno::{AxResult, ax_err};
use axvm_types::GuestPhysAddr;

use super::regs::{VIRTQ_DESC_F_NEXT, VIRTQ_DESC_F_WRITE};

/// Guest memory read/write callbacks used while processing queues.
pub trait GuestDma {
    fn read(&mut self, gpa: GuestPhysAddr, buf: &mut [u8]) -> AxResult;
    fn write(&mut self, gpa: GuestPhysAddr, buf: &[u8]) -> AxResult;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct VirtQueue {
    pub num: u32,
    pub ready: bool,
    pub desc: u64,
    pub avail: u64,
    pub used: u64,
    pub last_avail_idx: u16,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VirtqDesc {
    pub addr: u64,
    pub len: u32,
    pub flags: u16,
    pub next: u16,
}

impl VirtQueue {
    pub fn read_desc<D: GuestDma>(&self, dma: &mut D, index: u16) -> AxResult<VirtqDesc> {
        let mut raw = [0u8; 16];
        let gpa = GuestPhysAddr::from_usize((self.desc + u64::from(index) * 16) as usize);
        dma.read(gpa, &mut raw)?;
        Ok(VirtqDesc {
            addr: u64::from_le_bytes(raw[0..8].try_into().unwrap()),
            len: u32::from_le_bytes(raw[8..12].try_into().unwrap()),
            flags: u16::from_le_bytes(raw[12..14].try_into().unwrap()),
            next: u16::from_le_bytes(raw[14..16].try_into().unwrap()),
        })
    }

    pub fn avail_idx<D: GuestDma>(&self, dma: &mut D) -> AxResult<u16> {
        let mut raw = [0u8; 2];
        let gpa = GuestPhysAddr::from_usize((self.avail + 2) as usize);
        dma.read(gpa, &mut raw)?;
        Ok(u16::from_le_bytes(raw))
    }

    pub fn avail_ring<D: GuestDma>(&self, dma: &mut D, index: u16) -> AxResult<u16> {
        let mut raw = [0u8; 2];
        let gpa = GuestPhysAddr::from_usize((self.avail + 4 + u64::from(index) * 2) as usize);
        dma.read(gpa, &mut raw)?;
        Ok(u16::from_le_bytes(raw))
    }

    pub fn used_idx<D: GuestDma>(&self, dma: &mut D) -> AxResult<u16> {
        let mut raw = [0u8; 2];
        let gpa = GuestPhysAddr::from_usize((self.used + 2) as usize);
        dma.read(gpa, &mut raw)?;
        Ok(u16::from_le_bytes(raw))
    }

    pub fn write_used<D: GuestDma>(
        &self,
        dma: &mut D,
        used_idx: u16,
        desc_id: u16,
        len: u32,
    ) -> AxResult {
        let mut entry = [0u8; 8];
        entry[0..4].copy_from_slice(&u32::from(desc_id).to_le_bytes());
        entry[4..8].copy_from_slice(&len.to_le_bytes());
        let entry_gpa = GuestPhysAddr::from_usize(
            (self.used + 4 + u64::from(used_idx % self.num as u16) * 8) as usize,
        );
        dma.write(entry_gpa, &entry)?;
        let next = used_idx.wrapping_add(1);
        let idx_gpa = GuestPhysAddr::from_usize((self.used + 2) as usize);
        dma.write(idx_gpa, &next.to_le_bytes())?;
        Ok(())
    }

    /// Reads a descriptor chain into `buf`, returning `(head, bytes, write_only)`.
    pub fn read_chain_to<D: GuestDma>(
        &self,
        dma: &mut D,
        head: u16,
        buf: &mut [u8],
    ) -> AxResult<(u16, usize, bool)> {
        let mut idx = head;
        let mut offset = 0usize;
        let mut write_only = true;
        let mut hops = 0u32;
        loop {
            if hops >= self.num {
                return ax_err!(InvalidData, "virtqueue descriptor chain too long");
            }
            hops += 1;
            let desc = self.read_desc(dma, idx)?;
            if desc.flags & VIRTQ_DESC_F_WRITE == 0 {
                write_only = false;
                let len = desc.len as usize;
                if offset + len > buf.len() {
                    return ax_err!(InvalidInput, "TX frame exceeds buffer");
                }
                dma.read(
                    GuestPhysAddr::from_usize(desc.addr as usize),
                    &mut buf[offset..offset + len],
                )?;
                offset += len;
            }
            if desc.flags & VIRTQ_DESC_F_NEXT == 0 {
                break;
            }
            idx = desc.next;
        }
        Ok((head, offset, write_only))
    }

    /// Writes `data` into a write-only descriptor chain starting at `head`.
    pub fn write_chain_from<D: GuestDma>(
        &self,
        dma: &mut D,
        head: u16,
        data: &[u8],
    ) -> AxResult<u32> {
        let mut idx = head;
        let mut remaining = data;
        let mut written = 0u32;
        let mut hops = 0u32;
        loop {
            if hops >= self.num {
                return ax_err!(InvalidData, "virtqueue descriptor chain too long");
            }
            hops += 1;
            let desc = self.read_desc(dma, idx)?;
            if desc.flags & VIRTQ_DESC_F_WRITE != 0 {
                let room = desc.len as usize;
                let n = remaining.len().min(room);
                if n > 0 {
                    dma.write(
                        GuestPhysAddr::from_usize(desc.addr as usize),
                        &remaining[..n],
                    )?;
                    remaining = &remaining[n..];
                    written += n as u32;
                }
            }
            if desc.flags & VIRTQ_DESC_F_NEXT == 0 {
                break;
            }
            idx = desc.next;
        }
        if !remaining.is_empty() {
            return ax_err!(InvalidInput, "RX buffer too small for frame");
        }
        Ok(written)
    }
}

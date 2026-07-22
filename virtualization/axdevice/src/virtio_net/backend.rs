//! Frame forwarding backend for a VirtioNet port.

use alloc::{collections::VecDeque, vec::Vec};

use ax_kspin::SpinNoIrq as Mutex;

/// Maximum Ethernet frame size accepted by the loopback path (incl. headers).
pub const MAX_FRAME_LEN: usize = 1518;

/// Backend that receives TX frames and supplies RX frames for one guest port.
pub trait NetPortBackend: Send + Sync {
    /// Delivers one Ethernet frame from the guest TX queue.
    fn transmit(&self, frame: &[u8]);

    /// Pops one pending RX frame into `out`, returning the length, if any.
    fn try_receive(&self, out: &mut [u8]) -> Option<usize>;

    /// Returns whether at least one RX frame is queued.
    fn has_pending_rx(&self) -> bool {
        false
    }
}

/// Single-port loopback: every TX frame becomes an RX frame on the same port.
pub struct LoopbackBackend {
    rx: Mutex<VecDeque<Vec<u8>>>,
}

impl LoopbackBackend {
    /// Creates an empty loopback queue.
    pub const fn new() -> Self {
        Self {
            rx: Mutex::new(VecDeque::new()),
        }
    }
}

impl Default for LoopbackBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl NetPortBackend for LoopbackBackend {
    fn transmit(&self, frame: &[u8]) {
        if frame.is_empty() || frame.len() > MAX_FRAME_LEN {
            return;
        }
        let mut q = self.rx.lock();
        // Bound memory under TX flood.
        if q.len() >= 64 {
            let _ = q.pop_front();
        }
        q.push_back(frame.to_vec());
    }

    fn try_receive(&self, out: &mut [u8]) -> Option<usize> {
        let mut q = self.rx.lock();
        let frame = q.pop_front()?;
        if out.len() < frame.len() {
            // Put it back if the guest RX buffer is too small.
            q.push_front(frame);
            return None;
        }
        out[..frame.len()].copy_from_slice(&frame);
        Some(frame.len())
    }

    fn has_pending_rx(&self) -> bool {
        !self.rx.lock().is_empty()
    }
}

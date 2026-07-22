//! Registry of VirtioNet ports for cross-VM RX kick after L2 forward.

use alloc::{collections::BTreeMap, sync::Arc, vec::Vec};

use ax_kspin::SpinNoIrq as Mutex;

use super::{VirtioNetDevice, vsw::PortId};

/// One registered VirtioNet endpoint attached to the global switch.
#[derive(Clone)]
pub struct VirtioNetPortEndpoint {
    pub vm_id: usize,
    pub irq_id: usize,
    pub port: PortId,
    pub device: Arc<VirtioNetDevice>,
}

static PORTS: Mutex<BTreeMap<PortId, VirtioNetPortEndpoint>> = Mutex::new(BTreeMap::new());

/// Registers (or replaces) a VirtioNet port endpoint.
pub fn register_port(endpoint: VirtioNetPortEndpoint) {
    PORTS.lock().insert(endpoint.port, endpoint);
}

/// Removes a port endpoint.
pub fn unregister_port(port: PortId) {
    PORTS.lock().remove(&port);
}

/// Returns all endpoints except `except` (if any).
pub fn peer_endpoints(except: Option<PortId>) -> Vec<VirtioNetPortEndpoint> {
    PORTS
        .lock()
        .values()
        .filter(|ep| except != Some(ep.port))
        .cloned()
        .collect()
}

/// Returns endpoints registered for `vm_id`.
pub fn endpoints_for_vm(vm_id: usize) -> Vec<VirtioNetPortEndpoint> {
    PORTS
        .lock()
        .values()
        .filter(|ep| ep.vm_id == vm_id)
        .cloned()
        .collect()
}

/// Looks up one endpoint by port id.
pub fn lookup_port(port: PortId) -> Option<VirtioNetPortEndpoint> {
    PORTS.lock().get(&port).cloned()
}

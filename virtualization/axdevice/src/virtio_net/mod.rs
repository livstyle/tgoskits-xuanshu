//! Emulated Virtio MMIO network device (Task 2).
//!
//! Provides a Virtio 1.0 MMIO transport with RX/TX virtqueues and a pluggable
//! [`NetPortBackend`]. Ports may use per-device loopback or the shared
//! Hypervisor L2 switch ([`vsw`]).

mod backend;
mod device;
mod factory;
mod port_registry;
mod queue;
mod regs;
mod vsw;

pub use backend::{LoopbackBackend, NetPortBackend};
pub use device::VirtioNetDevice;
pub use factory::VirtioNetFactory;
pub use port_registry::{
    VirtioNetPortEndpoint, endpoints_for_vm, lookup_port, peer_endpoints, register_port,
    unregister_port,
};
pub use vsw::{
    AclAction, FrameAcl, ICPC_UDP_PORT, IcpcPortAcl, PortId, SwitchPortBackend, VirtualSwitch,
    global_vsw,
};

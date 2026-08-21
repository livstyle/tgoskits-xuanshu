//! Hypervisor L2 switch helpers for Task 2 icpc tests.
//!
//! Guest virtio-net devices are implemented in [`axvirtio_net`] and wired from
//! `os/axvisor/src/virtio_net.rs`. This module keeps the legacy switch backend
//! (`vsw`) and fault-injection hooks used by CI.

mod backend;
mod vsw;

pub use backend::{LoopbackBackend, NetPortBackend};
pub use vsw::{
    AclAction, FrameAcl, ICPC_UDP_PORT, IcpcPortAcl, PortId, SwitchPortBackend, VirtualSwitch,
    configure_vsw_fault_inject, global_vsw,
};

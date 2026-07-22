//! VirtioNet device factory.

use alloc::sync::Arc;

use ax_errno::{AxResult, ax_err};
use axdevice_base::MmioDeviceAdapter;
use axvm_types::{EmulatedDeviceConfig, EmulatedDeviceType};

use super::{
    LoopbackBackend, PortId, SwitchPortBackend, VirtioNetDevice, VirtioNetPortEndpoint, global_vsw,
    register_port,
};
use crate::{DeviceBuildContext, DeviceBundle, DeviceFactory, DeviceRegistration};

/// Builds [`VirtioNetDevice`] from VM `emu_devices` entries.
///
/// `cfg_list` layout:
/// - `[mac0..mac5]` — optional MAC (defaults to locally-administered)
/// - `[mac0..mac5, port_id]` — attach to the global L2 switch on `port_id`
/// - `[port_id]` alone is invalid; MAC defaults still need 0 or 6 bytes before port
///
/// When `port_id` is omitted, the device uses a private loopback backend.
pub struct VirtioNetFactory;

impl DeviceFactory for VirtioNetFactory {
    fn device_type(&self) -> EmulatedDeviceType {
        EmulatedDeviceType::VirtioNet
    }

    fn build(
        &self,
        config: &EmulatedDeviceConfig,
        context: &DeviceBuildContext<'_>,
    ) -> AxResult<DeviceBundle> {
        if config.length == 0 {
            return ax_err!(InvalidInput, "virtio-net length must be non-zero");
        }
        let (mac, port_id) = parse_mac_and_port(&config.cfg_list, config.id_hint())?;
        let (backend, port): (Arc<dyn super::NetPortBackend>, Option<PortId>) =
            if let Some(port) = port_id {
                let sw = global_vsw();
                (Arc::new(SwitchPortBackend::new(port, sw)), Some(port))
            } else {
                (Arc::new(LoopbackBackend::new()), None)
            };

        let device = Arc::new(VirtioNetDevice::new(
            config.base_gpa,
            config.length,
            mac,
            config.irq_id,
            backend,
        ));
        device.set_port_id(port);

        if let Some(port) = port {
            register_port(VirtioNetPortEndpoint {
                vm_id: context.vm_id(),
                irq_id: config.irq_id,
                port,
                device: device.clone(),
            });
        }

        info!(
            "virtio-net '{}' @ {:#x} len={:#x} irq={} vm={} port={:?} \
             mac={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            config.name,
            config.base_gpa,
            config.length,
            config.irq_id,
            context.vm_id(),
            port,
            mac[0],
            mac[1],
            mac[2],
            mac[3],
            mac[4],
            mac[5]
        );
        Ok(DeviceRegistration::Device(MmioDeviceAdapter::from_arc(device)).into())
    }
}

fn parse_mac_and_port(cfg: &[usize], hint: u8) -> AxResult<([u8; 6], Option<PortId>)> {
    if cfg.is_empty() {
        return Ok(([0x02, 0x00, 0x00, 0x00, 0x00, hint], None));
    }
    if cfg.len() == 1 {
        let Some(port) = u16::try_from(cfg[0]).ok() else {
            return ax_err!(InvalidInput, "virtio-net port_id out of range");
        };
        return Ok(([0x02, 0x00, 0x00, 0x00, 0x00, hint], Some(PortId(port))));
    }
    if cfg.len() < 6 {
        return ax_err!(
            InvalidInput,
            "virtio-net cfg_list must be empty, [port], [mac;6], or [mac;6, port]"
        );
    }
    let mut mac = [0u8; 6];
    for (i, v) in cfg.iter().take(6).enumerate() {
        if *v > 0xff {
            return ax_err!(InvalidInput, "virtio-net MAC byte out of range");
        }
        mac[i] = *v as u8;
    }
    let port = if cfg.len() >= 7 {
        let Some(port) = u16::try_from(cfg[6]).ok() else {
            return ax_err!(InvalidInput, "virtio-net port_id out of range");
        };
        Some(PortId(port))
    } else {
        None
    };
    Ok((mac, port))
}

trait ConfigIdHint {
    fn id_hint(&self) -> u8;
}

impl ConfigIdHint for EmulatedDeviceConfig {
    fn id_hint(&self) -> u8 {
        (self.base_gpa >> 9) as u8
    }
}

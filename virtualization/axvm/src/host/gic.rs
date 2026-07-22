//! AArch64 GIC host operations for the ArceOS-backed AxVM runtime.

use arm_gic_driver::{
    IntId,
    v3::{
        Affinity, Gic, ICH_ELRSR_EL2, ICH_HCR_EL2, ICH_LR_EL2, ICH_VTR_EL2, ReadWriteable,
        Readable, ich_lr_el2_get, ich_lr_el2_write,
    },
};
use ax_memory_addr::{PhysAddr, VirtAddr};

use super::{HostMemory, arceos, default_host};

fn with_gic<T>(f: impl FnOnce(&mut rdif_intc::Intc) -> T) -> T {
    let mut gic = rdrive::get_one::<rdif_intc::Intc>()
        .expect("failed to get GIC driver")
        .lock()
        .expect("failed to lock GIC driver");
    f(&mut gic)
}

/// Marks an SPI (DT `interrupts` cell 1) pending on the physical GIC.
pub(crate) fn set_pending_spi(spi_index: usize) {
    set_pending_spi_on_cpu(spi_index, None);
}

/// Marks an SPI pending and optionally routes it to `host_cpu_id`'s affinity.
///
/// When `host_cpu_id` is `None`, routes to the current CPU (for local injection).
/// Cross-VM VirtioNet kicks must pass the peer vCPU's host CPU so the SPI is
/// not accidentally steered to the sender's core.
pub(crate) fn set_pending_spi_on_cpu(spi_index: usize, host_cpu_id: Option<usize>) {
    let intid = arm_gic_driver::IntId::spi(spi_index as u32);
    let target = host_cpu_id.map(|cpu| Affinity {
        aff0: (cpu & 0xff) as u8,
        aff1: ((cpu >> 8) & 0xff) as u8,
        aff2: ((cpu >> 16) & 0xff) as u8,
        aff3: ((cpu >> 24) & 0xff) as u8,
    });
    trace!("GIC set_pending SPI {spi_index} -> {intid:?} target={target:?}");
    with_gic(|gic| {
        let id = intid;
        if let Some(gic) = gic.typed_mut::<arm_gic_driver::v2::Gic>() {
            gic.set_pending(id, false);
            gic.set_pending(id, true);
            return;
        }
        if let Some(gic) = gic.typed_mut::<Gic>() {
            if !id.is_private() {
                gic.set_target_cpu(id, Some(target.unwrap_or_else(Affinity::current)));
            }
            gic.set_pending(id, false);
            gic.set_pending(id, true);
            return;
        }
        panic!("no GIC driver found");
    });
}

/// Marks a GIC interrupt pending using an architectural INTID (SGI/PPI).
pub(crate) fn set_pending_irq(intid: usize) {
    trace!("GIC set_pending INTID {intid}");
    with_gic(|gic| {
        let id = unsafe { IntId::raw(intid as u32) };
        if let Some(gic) = gic.typed_mut::<arm_gic_driver::v2::Gic>() {
            gic.set_pending(id, false);
            gic.set_pending(id, true);
            return;
        }
        if let Some(gic) = gic.typed_mut::<Gic>() {
            if !id.is_private() {
                gic.set_target_cpu(id, Some(Affinity::current()));
            }
            gic.set_pending(id, false);
            gic.set_pending(id, true);
            return;
        }
        panic!("no GIC driver found");
    });
}

pub(crate) fn inject_interrupt(irq: usize) {
    debug!("Injecting virtual interrupt: {irq}");

    with_gic(|gic| {
        if let Some(gic) = gic.typed_mut::<arm_gic_driver::v2::Gic>() {
            use arm_gic_driver::{
                IntId,
                v2::{VirtualInterruptConfig, VirtualInterruptState},
            };

            let gich = gic.hypervisor_interface().expect("failed to get GICH");
            gich.enable();
            gich.set_virtual_interrupt(
                0,
                VirtualInterruptConfig::software(
                    unsafe { IntId::raw(irq as _) },
                    None,
                    0,
                    VirtualInterruptState::Pending,
                    false,
                    true,
                ),
            );
            return;
        }

        if gic.typed_mut::<arm_gic_driver::v3::Gic>().is_some() {
            inject_interrupt_gic_v3(irq);
            return;
        }

        panic!("no GIC driver found");
    });
}

fn inject_interrupt_gic_v3(vector: usize) {
    debug!("Injecting virtual interrupt: vector={vector}");
    let elsr = ICH_ELRSR_EL2.read(ICH_ELRSR_EL2::STATUS);
    let lr_num = ICH_VTR_EL2.read(ICH_VTR_EL2::LISTREGS) as usize + 1;

    let mut free_lr = None;
    for i in 0..lr_num {
        if (1 << i) & elsr > 0 {
            free_lr.get_or_insert(i);
            continue;
        }

        let lr_val = ich_lr_el2_get(i);
        if lr_val.read(ICH_LR_EL2::VINTID) == vector as u64
            && lr_val.matches_any(&[ICH_LR_EL2::STATE::Pending, ICH_LR_EL2::STATE::Active])
        {
            debug!("Virtual interrupt {vector} already pending/active in LR{i}, skipping");
            return;
        }
    }

    let free_lr = free_lr
        .or_else(|| {
            (0..lr_num).find(|&i| ich_lr_el2_get(i).matches_all(ICH_LR_EL2::STATE::Invalid))
        })
        .unwrap_or_else(|| panic!("no free list register to inject IRQ {vector}"));

    ich_lr_el2_write(
        free_lr,
        ICH_LR_EL2::VINTID.val(vector as u64) + ICH_LR_EL2::STATE::Pending + ICH_LR_EL2::GROUP::SET,
    );

    if !ICH_HCR_EL2.is_set(ICH_HCR_EL2::EN) {
        warn!("Virtual interrupt interface not enabled, enabling now");
        ICH_HCR_EL2.modify(ICH_HCR_EL2::EN::SET);
    }

    debug!("Virtual interrupt {vector} injected successfully in LR{free_lr}");
}

pub(crate) fn read_gicd_iidr() -> u32 {
    with_gic(|gic| {
        if let Some(gic) = gic.typed_mut::<arm_gic_driver::v2::Gic>() {
            return gic.iidr_raw();
        }
        if let Some(gic) = gic.typed_mut::<arm_gic_driver::v3::Gic>() {
            return gic.iidr_raw();
        }
        panic!("no GIC driver found");
    })
}

pub(crate) fn read_gicd_typer() -> u32 {
    with_gic(|gic| {
        if let Some(gic) = gic.typed_mut::<arm_gic_driver::v2::Gic>() {
            return gic.typer_raw();
        }
        if let Some(gic) = gic.typed_mut::<arm_gic_driver::v3::Gic>() {
            return gic.typer_raw();
        }
        panic!("no GIC driver found");
    })
}

pub(crate) fn host_gicd_base() -> PhysAddr {
    with_gic(|gic| {
        if let Some(gic) = gic.typed_mut::<arm_gic_driver::v2::Gic>() {
            return default_host().virt_to_phys(VirtAddr::from(usize::from(gic.gicd_addr())));
        }
        if let Some(gic) = gic.typed_mut::<arm_gic_driver::v3::Gic>() {
            return default_host().virt_to_phys(VirtAddr::from(usize::from(gic.gicd_addr())));
        }
        panic!("no GIC driver found");
    })
}

pub(crate) fn host_gicr_base() -> PhysAddr {
    with_gic(|gic| {
        if let Some(gic) = gic.typed_mut::<arm_gic_driver::v3::Gic>() {
            return default_host().virt_to_phys(VirtAddr::from(usize::from(gic.gicr_addr())));
        }
        panic!("no GICv3 driver found");
    })
}

pub(crate) fn handle_current_irq() -> Option<usize> {
    // AArch64 ArceOS platform IRQ handlers acknowledge the current IRQ
    // internally. The raw vector argument is ignored by current GIC-backed
    // platforms, so keep the ack/EOI ownership inside the platform handler.
    arceos::handle_host_irq(0)
}

pub(crate) fn fetch_irq() -> usize {
    handle_current_irq().unwrap_or(0)
}

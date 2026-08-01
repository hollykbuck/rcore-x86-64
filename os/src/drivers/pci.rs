//! PCI bus enumeration for QEMU `q35`.
//!
//! The `q35` machine exposes the PCIe Enhanced Configuration Access Mechanism
//! (ECAM) as MMIO at a base programmed in the MCH's `PCIEXBAR` register
//! (config space offset `0x60`). OVMF may move it away from QEMU's default
//! `0xB0000000`, so we read it at runtime through the legacy IO-port CAM
//! (`0xCF8`/`0xCFC`), map the window through the physmap, and then use
//! `virtio-drivers`' `PciRoot`/`PciTransport` over that MMIO ECAM.

use crate::mm::ioremap;
use core::arch::asm;
use log::*;
use virtio_drivers::transport::pci::bus::{BarInfo, Cam, DeviceFunction, MemoryBarType, PciRoot};
use virtio_drivers::transport::pci::virtio_device_type;
use virtio_drivers::transport::DeviceType;

/// Bus 0 of the ECAM: 32 devices * 8 functions * 256 B configuration space
/// = 1 MiB per bus.
const ECAM_BUS0_SIZE: usize = 0x10_0000;

/// Read a 32-bit word from PCI configuration space through the legacy IO
/// port CAM (works before the ECAM is mapped).
fn pci_read_config_io(bus: u8, dev: u8, func: u8, reg: u8) -> u32 {
    let address = 0x8000_0000u32
        | (bus as u32) << 16
        | (dev as u32) << 11
        | (func as u32) << 8
        | (reg as u32 & 0xFC);
    unsafe {
        asm!(
            "out dx, eax",
            in("dx") 0xCF8u16,
            in("eax") address,
            options(nostack, preserves_flags)
        );
        let value: u32;
        asm!(
            "in eax, dx",
            out("eax") value,
            in("dx") 0xCFCu16,
            options(nostack, preserves_flags)
        );
        value
    }
}

/// The physical address of the PCIe ECAM, read from the MCH's `PCIEXBAR`
/// register (which OVMF may have reprogrammed).
fn ecam_base() -> usize {
    // MCH is bus 0, device 0, function 0; PCIEXBAR is at config offset 0x60.
    let low = pci_read_config_io(0, 0, 0, 0x60);
    let high = pci_read_config_io(0, 0, 0, 0x64);
    let value = ((high as u64) << 32) | low as u64;
    info!("[pci] MCH PCIEXBAR = {:#x}", value);
    assert_ne!(value & 1, 0, "PCIEXBAR not enabled by the firmware");
    (value & 0xFFFF_FFFF_FFFF_FF00) as usize
}

pub struct Pci {
    root: PciRoot,
}

impl Pci {
    /// Map the ECAM window and wrap it in a `PciRoot`.
    pub fn new() -> Self {
        let ecam = ecam_base();
        info!("[pci] ECAM at {:#x}", ecam);
        ioremap(ecam, ECAM_BUS0_SIZE);
        let mmio_base = crate::mm::phys_to_virt(ecam) as *mut u8;
        Self {
            root: unsafe { PciRoot::new(mmio_base, Cam::Ecam) },
        }
    }
    /// Find the first VirtIO block device on bus 0 and map its BARs into the
    /// physmap window (so that `PciTransport::new` can reach them via
    /// `H::phys_to_virt`). Returns its bus/device/function.
    pub fn find_block(&mut self) -> Option<DeviceFunction> {
        let mut found = None;
        for (bdf, info) in self.root.enumerate_bus(0) {
            if virtio_device_type(&info) == Some(DeviceType::Block) {
                info!(
                    "[pci] found virtio block at {:02x}:{:02x}.{} (device {:#06x})",
                    bdf.bus, bdf.device, bdf.function, info.device_id
                );
                found = Some(bdf);
                break;
            }
        }
        let bdf = found?;
        let mut bar_index = 0u8;
        while bar_index < 6 {
            match self.root.bar_info(bdf, bar_index) {
                Ok(BarInfo::Memory {
                    address_type,
                    address,
                    size,
                    ..
                }) if size != 0 && address != 0 => {
                    let start = (address as usize) & !(0xfff);
                    let end = ((address as usize) + size as usize + 0xfff) & !(0xfff);
                    ioremap(start, end - start);
                    if address_type == MemoryBarType::Width64 {
                        // 64-bit BAR occupies two consecutive slots; skip the
                        // upper half so it is not treated as a separate BAR.
                        bar_index += 2;
                    } else {
                        bar_index += 1;
                    }
                }
                Ok(_) => bar_index += 1,
                Err(_) => bar_index += 1,
            }
        }
        Some(bdf)
    }
    pub fn root(&mut self) -> &mut PciRoot {
        &mut self.root
    }
}

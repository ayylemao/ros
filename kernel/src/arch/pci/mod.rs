use core::fmt::Display;

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use crate::{
    fs::vfs::{Vfs, VfsNode},
    serial::{inl, outl},
};

#[derive(Debug)]
pub struct PCI {
    pub devices: Vec<PCIDevice>,
}

impl Display for PCI {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        _ = writeln!(
            f,
            "{:<8} {:<8}     {:<24} {:<24}",
            "VENDOR ID", "DEVICE ID", "VENDOR", "DEVICE"
        );
        for d in &self.devices {
            _ = write!(f, "{}", d);
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct PCIDevice {
    pub vendor_id: u16,
    pub device_id: u16,
    pub status: u16,
    pub command: u16,
    pub header_type: u8,
    pub bist: u8,
    pub prog_if: u8,
    pub rev_id: u8,
    pub latency_timer: u8,
    pub cache_line_size: u8,
    pub vendor_name: String,
    pub device_name: String,
    pub gen_header: GenHeader,
}

#[derive(Debug, Default)]
pub struct GenHeader {
    pub bar0: u32,
    pub bar1: u32,
    pub bar2: u32,
    pub bar3: u32,
    pub bar4: u32,
    pub bar5: u32,
    pub cb_cis_ptr: u32,
    pub subsys_vndr_id: u16,
    pub subsys_id: u16,
    pub exp_rom_bar: u32,
    pub capa_ptr: u8,
    pub intrpt_line: u8,
    pub intrpt_pin: u8,
    pub min_grant: u8,
    pub max_lat: u8,
}

impl Display for PCIDevice {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(
            f,
            "{:04x}      {:04x}          {:<24} {:<24}",
            self.vendor_id, self.device_id, self.vendor_name, self.device_name
        )
    }
}

impl PCI {
    pub fn pci_config_read_word(bus: u8, slot: u8, func: u8, offset: u8) -> u16 {
        let address: u32 = (0x8000_0000u32)
            | ((bus as u32) << 16)
            | ((slot as u32) << 11)
            | ((func as u32) << 8)
            | ((offset as u32) & 0xFC);

        unsafe { outl(0xCF8, address) };
        let data = unsafe { inl(0xCFC) };

        let shift = ((offset & 2) as u32) * 8; // 0 or 16
        ((data >> shift) & 0xFFFF) as u16
    }

    pub fn pci_config_read_dword(bus: u8, slot: u8, func: u8, offset: u8) -> u32 {
        let address: u32 = 0x8000_0000
            | ((bus as u32) << 16)
            | ((slot as u32) << 11)
            | ((func as u32) << 8)
            | ((offset as u32) & 0xFC);

        unsafe { outl(0xCF8, address) };
        unsafe { inl(0xCFC) }
    }

    pub fn get_vendor_id(bus: u8, device: u8, func: u8) -> u16 {
        let r0: u16 = Self::pci_config_read_word(bus, device, func, 0);
        r0 as u16
    }

    pub fn get_device_id(bus: u8, device: u8, func: u8) -> u16 {
        let r0: u16 = Self::pci_config_read_word(bus, device, func, 2);
        r0 as u16
    }

    pub fn get_command(bus: u8, device: u8, func: u8) -> u16 {
        let r0: u16 = Self::pci_config_read_word(bus, device, func, 4);
        r0 as u16
    }

    pub fn get_status(bus: u8, device: u8, func: u8) -> u16 {
        let r0: u16 = Self::pci_config_read_word(bus, device, func, 6);
        r0 as u16
    }

    pub fn get_class_id(bus: u8, device: u8, func: u8) -> u16 {
        let r0: u16 = Self::pci_config_read_word(bus, device, func, 0xA);
        r0 & !0x00FF
    }

    pub fn get_sub_class_id(bus: u8, device: u8, func: u8) -> u16 {
        let r0: u16 = Self::pci_config_read_word(bus, device, func, 0xA);
        r0 & !0xFF00
    }

    pub fn get_prog_if(bus: u8, device: u8, func: u8) -> u8 {
        let r0: u16 = Self::pci_config_read_word(bus, device, func, 8);
        (r0 >> 8) as u8
    }

    pub fn get_rev_id(bus: u8, device: u8, func: u8) -> u8 {
        let r0: u16 = Self::pci_config_read_word(bus, device, func, 8);
        (r0 & 0xFF) as u8
    }

    pub fn get_cache_line_size(bus: u8, device: u8, func: u8) -> u8 {
        let r0: u16 = Self::pci_config_read_word(bus, device, func, 0xC);
        (r0 & 0xFF) as u8
    }

    pub fn get_latency_timer(bus: u8, device: u8, func: u8) -> u8 {
        let r0: u16 = Self::pci_config_read_word(bus, device, func, 0xC);
        (r0 >> 8) as u8
    }

    pub fn get_header_type(bus: u8, device: u8, func: u8) -> u8 {
        let r0: u16 = Self::pci_config_read_word(bus, device, func, 0xE);
        (r0 & 0xFF) as u8
    }

    pub fn get_bist(bus: u8, device: u8, func: u8) -> u8 {
        let r0: u16 = Self::pci_config_read_word(bus, device, func, 0xE);
        (r0 >> 8) as u8
    }

    pub fn pci_init() -> Self {
        let mut devices: Vec<PCIDevice> = Vec::new();
        for bus in 0..=255 {
            for slot in 0..32 {
                for func in 0..8 {
                    let vendor_id = Self::get_vendor_id(bus, slot, func);
                    if vendor_id == 0xFFFF {
                        continue;
                    }
                    let device_id = Self::get_device_id(bus, slot, func);
                    let (vendor_name, device_name) = match Self::lookup_device(vendor_id, device_id)
                    {
                        Some(v) => v,
                        None => {
                            continue;
                        }
                    };

                    let header_type = Self::get_header_type(bus, slot, func);
                    let gen_header: GenHeader = if header_type == 0 {
                        let ss = Self::pci_config_read_dword(bus, slot, func, 0x2C);
                        let il = Self::pci_config_read_dword(bus, slot, func, 0x3C);
                        GenHeader {
                            bar0: Self::pci_config_read_dword(bus, slot, func, 0x10),
                            bar1: Self::pci_config_read_dword(bus, slot, func, 0x14),
                            bar2: Self::pci_config_read_dword(bus, slot, func, 0x18),
                            bar3: Self::pci_config_read_dword(bus, slot, func, 0x1C),
                            bar4: Self::pci_config_read_dword(bus, slot, func, 0x20),
                            bar5: Self::pci_config_read_dword(bus, slot, func, 0x24),
                            cb_cis_ptr: Self::pci_config_read_dword(bus, slot, func, 0x28),
                            subsys_vndr_id: (ss & 0xFFFF) as u16,
                            subsys_id: (ss >> 16) as u16,
                            exp_rom_bar: Self::pci_config_read_dword(bus, slot, func, 0x30),
                            capa_ptr: (Self::pci_config_read_word(bus, slot, func, 0x34) & 0xFF)
                                as u8,
                            intrpt_line: (il & 0xFF) as u8,
                            intrpt_pin: ((il >> 8) & 0xFF) as u8,
                            min_grant: ((il >> 16) & 0xFF) as u8,
                            max_lat: ((il >> 24) & 0xFF) as u8,
                        }
                    } else {
                        GenHeader::default()
                    };

                    let pci_device = PCIDevice {
                        vendor_id,
                        device_id,
                        vendor_name,
                        device_name,
                        command: Self::get_command(bus, slot, func),
                        status: Self::get_status(bus, slot, func),
                        header_type,
                        bist: Self::get_bist(bus, slot, func),
                        prog_if: Self::get_prog_if(bus, slot, func),
                        rev_id: Self::get_rev_id(bus, slot, func),
                        latency_timer: Self::get_latency_timer(bus, slot, func),
                        cache_line_size: Self::get_cache_line_size(bus, slot, func),
                        gen_header,
                    };
                    devices.push(pci_device);
                }
            }
        }
        PCI { devices }
    }

    pub fn lookup_device(vendor: u16, device: u16) -> Option<(String, String)> {
        let bytes = {
            let vfs = Vfs::get().lock();
            let node = vfs
                .resolve(VfsNode { mount: 0, node: 0 }, "/usr/share/hwdata/pci.ids")
                .unwrap();
            vfs.read_all(node).unwrap()
        };

        let file_content: &str = unsafe { str::from_utf8_unchecked(bytes.as_slice()) };

        let vendor_id_str = format!("{:x}", vendor);
        let device_id_str = format!("{:x}", device);

        let mut vendor = String::new();
        let mut device = String::new();

        let mut vendor_found: bool = false;
        for raw in file_content.lines() {
            if raw.is_empty() || raw.starts_with('#') {
                continue;
            }
            let tabs = raw.bytes().take_while(|&b| b == b'\t').count();
            let line = &raw[tabs..];
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if vendor_found == false && tabs == 0 && line.starts_with(vendor_id_str.as_str()) {
                vendor = line.split("  ").last().unwrap().trim().to_string();
                vendor_found = true;
                continue;
            }
            if vendor_found == true && tabs == 1 && line.starts_with(device_id_str.as_str()) {
                device = line.split("  ").last().unwrap().trim().to_string();
                break;
            }
        }

        if vendor.len() == 0 || device.len() == 0 {
            return None;
        }
        Some((vendor, device))
    }
}

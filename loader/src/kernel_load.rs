use alloc::vec::Vec;
use elf::ElfBytes;
use elf::endian::LittleEndian;
use uefi::fs::FileSystem;
use uefi::{prelude::*, println};

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct LoadedSegment {
    pub phys_addr: u64,
    pub virt_addr: u64,
    pub file_size: u64,
    pub mem_size: u64,
    pub align: u64,
    pub flags: u32,
    pub data: Vec<u8>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct KernelLoadInfo {
    pub entry_point: u64,
    pub segments: Vec<LoadedSegment>,
}

pub fn load_kernel() -> Option<KernelLoadInfo> {
    let fs = uefi::boot::get_image_file_system(uefi::boot::image_handle()).ok()?;
    let mut fs = FileSystem::new(fs);
    let path = cstr16!("kernel.elf");
    let kernel = fs.read(path).ok()?; // <-- Entire ELF file in memory

    // Parse ELF
    let elf = ElfBytes::<LittleEndian>::minimal_parse(&kernel).ok()?;
    let phdrs = elf.segments().unwrap();

    println!("ELF program header count: {}", phdrs.len());

    let mut segments: Vec<LoadedSegment> = Vec::new();

    for ph in phdrs {
        if ph.p_type != elf::abi::PT_LOAD {
            continue;
        }

        println!(
            "Loading kernel segment: vaddr=0x{:x}, memsz={}, filesz={}, offset=0x{:x}",
            ph.p_vaddr, ph.p_memsz, ph.p_filesz, ph.p_offset
        );

        // --- Extract actual bytes belonging to this segment ---
        let file_bytes = if ph.p_filesz > 0 {
            let start = ph.p_offset as usize;
            let end = start + ph.p_filesz as usize;

            if end > kernel.len() {
                println!("Segment out of bounds!");
                return None;
            }

            kernel[start..end].to_vec()
        } else {
            Vec::new()
        };

        segments.push(LoadedSegment {
            phys_addr: 0u64,
            virt_addr: ph.p_vaddr,
            file_size: ph.p_filesz,
            mem_size: ph.p_memsz,
            flags: ph.p_flags,
            align: ph.p_align,
            data: file_bytes,
        });
    }

    let entry = elf.ehdr.e_entry;

    Some(KernelLoadInfo {
        entry_point: entry,
        segments,
    })
}

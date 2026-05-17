use core::ptr;

use alloc::vec::Vec;
use elf::{endian::LittleEndian, ElfBytes};
use shared::{
    align_down, align_up, PAGE_SIZE, PROCESS_STACK_PAGES, USER_SPACE_BOTTOM, USER_SPACE_TOP,
};
use sys::syscall::errors::{ElfLoadError, MapElfError};
use x86_64::{
    structures::paging::{FrameAllocator, Mapper, OffsetPageTable, Page, PageTableFlags, Size4KiB},
    VirtAddr,
};

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct LoadedSegment {
    pub virt_addr: u64,
    pub file_offset: u64,
    pub file_size: u64,
    pub mem_size: u64,
    pub align: u64,
    pub flags: u32,
    pub data: Vec<u8>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ElfLoadInfo {
    pub entry_point: u64,
    pub phoff: u64,
    pub phentsize: u16,
    pub phnum: u16,
    pub segments: Vec<LoadedSegment>,
}

pub fn parse_elf(buf: &[u8]) -> Result<ElfLoadInfo, ElfLoadError> {
    let elf =
        ElfBytes::<LittleEndian>::minimal_parse(buf).map_err(|_x| ElfLoadError::ParserError)?;
    let phdrs = elf
        .segments()
        .ok_or(ElfLoadError::PhdrError)
        .map_err(|_x| ElfLoadError::PhdrError)?;

    let mut segments: Vec<LoadedSegment> = Vec::new();

    for ph in phdrs {
        if ph.p_type != elf::abi::PT_LOAD {
            continue;
        }

        if ph.p_memsz < ph.p_filesz {
            return Err(ElfLoadError::FileSizeLargerThanMemSize);
        }

        if !is_power_two(ph.p_align) {
            return Err(ElfLoadError::AlignNotPowerTwo);
        }

        if ph.p_vaddr.checked_add(ph.p_memsz).is_none() {
            return Err(ElfLoadError::VirtAddrOverflow);
        }

        if !(USER_SPACE_BOTTOM..USER_SPACE_TOP).contains(&ph.p_vaddr)
            || !(USER_SPACE_BOTTOM..USER_SPACE_TOP).contains(&(ph.p_vaddr + ph.p_memsz - 1))
        {
            return Err(ElfLoadError::SegmentVirtOutOfBounds);
        }

        let file_bytes = if ph.p_filesz > 0 {
            let start = ph.p_offset as usize;
            let end = start + ph.p_filesz as usize;

            if end > buf.len() {
                return Err(ElfLoadError::SegmentFileRangeOutOfBounds);
            }

            buf[start..end].to_vec()
        } else {
            Vec::new()
        };
        segments.push(LoadedSegment {
            virt_addr: ph.p_vaddr,
            file_offset: ph.p_offset,
            file_size: ph.p_filesz,
            mem_size: ph.p_memsz,
            flags: ph.p_flags,
            align: ph.p_align,
            data: file_bytes,
        });
    }

    let entry = elf.ehdr.e_entry;

    if !segments
        .iter()
        .any(|s| (s.virt_addr..s.virt_addr + s.mem_size).contains(&entry))
    {
        return Err(ElfLoadError::NoEntryInAnySegment);
    }

    Ok(ElfLoadInfo {
        entry_point: entry,
        phoff: elf.ehdr.e_phoff,
        phentsize: elf.ehdr.e_phentsize,
        phnum: elf.ehdr.e_phnum,
        segments,
    })
}

#[inline]
fn is_power_two(x: u64) -> bool {
    if x == 0 {
        return true;
    }
    return (x != 0) && ((x & (x - 1)) == 0);
}

#[inline]
fn flags_from_elf(p_flags: u32) -> PageTableFlags {
    let mut f = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;

    if (p_flags & elf::abi::PF_W) != 0 {
        f |= PageTableFlags::WRITABLE;
    }

    if (p_flags & elf::abi::PF_X) == 0 {
        f |= PageTableFlags::NO_EXECUTE;
    }

    f
}

pub fn map_elf(
    eli: &ElfLoadInfo,
    alloc: &mut impl FrameAllocator<Size4KiB>,
    mapper: &mut OffsetPageTable,
) -> Result<u64, MapElfError> {
    let mut mapped_pages: Vec<u64> = Vec::new();
    let mut max_end_va: u64 = 0;

    for seg in &eli.segments {
        let seg_va = seg.virt_addr;
        let memsz = seg.mem_size;
        let filesz = seg.data.len();
        let end = seg_va + memsz;
        max_end_va = max_end_va.max(end);

        if memsz == 0 {
            continue;
        }

        let seg_end = seg_va.checked_add(memsz).ok_or(MapElfError::Overflow)?;

        let map_base = align_down(seg_va);
        let map_end = align_up(seg_end);

        let pages = ((map_end - map_base) / PAGE_SIZE) as u64;

        let final_flags = flags_from_elf(seg.flags);

        let temp_flags = final_flags | PageTableFlags::WRITABLE;

        for i in 0..pages {
            let va = map_base
                .checked_add(i.checked_mul(PAGE_SIZE).ok_or(MapElfError::Overflow)?)
                .ok_or(MapElfError::Overflow)?;

            if mapped_pages.iter().any(|&p| p == va) {
                return Err(MapElfError::OverlappingPage { va });
            }

            let frame = alloc.allocate_frame().ok_or(MapElfError::OutOfFrames)?;

            unsafe {
                mapper
                    .map_to(
                        Page::<Size4KiB>::containing_address(VirtAddr::new(va)),
                        frame,
                        temp_flags,
                        alloc,
                    )
                    .map_err(|_| MapElfError::MapFailed)?
                    .flush();
            }
            mapped_pages.push(va);
        }

        if filesz > 0 {
            if filesz > memsz as usize {
                return Err(MapElfError::CopyTooLarge);
            }
            unsafe {
                ptr::copy_nonoverlapping(seg.data.as_ptr(), seg_va as *mut u8, filesz as usize)
            };
        }

        if memsz > filesz as u64 {
            let zero_start = seg_va
                .checked_add(filesz as u64)
                .ok_or(MapElfError::Overflow)?;
            let zero_len = memsz - filesz as u64;
            unsafe {
                ptr::write_bytes(zero_start as *mut u8, 0, zero_len as usize);
            }
        }

        if temp_flags != final_flags {
            for i in 0..pages {
                let va = map_base
                    .checked_add(i.checked_mul(PAGE_SIZE).ok_or(MapElfError::Overflow)?)
                    .ok_or(MapElfError::Overflow)?;

                unsafe {
                    mapper
                        .update_flags(
                            Page::<Size4KiB>::containing_address(VirtAddr::new(va)),
                            final_flags,
                        )
                        .map_err(|_| MapElfError::FlagUpdateFailed)?
                        .flush();
                }
            }
        }
    }
    Ok(max_end_va)
}

pub fn map_user_stack(
    alloc: &mut impl FrameAllocator<Size4KiB>,
    mapper: &mut OffsetPageTable,
) -> Result<(u64, u64), MapElfError> {
    let stack_top = align_down(USER_SPACE_TOP);
    let stack_bottom = stack_top - PROCESS_STACK_PAGES * PAGE_SIZE;

    for i in 1..PROCESS_STACK_PAGES {
        let frame = alloc.allocate_frame().ok_or(MapElfError::OutOfFrames)?;
        let va = stack_bottom + PAGE_SIZE * i;

        unsafe {
            mapper
                .map_to(
                    Page::<Size4KiB>::containing_address(VirtAddr::new(va)),
                    frame,
                    PageTableFlags::PRESENT
                        | PageTableFlags::USER_ACCESSIBLE
                        | PageTableFlags::WRITABLE
                        | PageTableFlags::NO_EXECUTE,
                    alloc,
                )
                .map_err(|_| MapElfError::MapFailed)?
                .flush();
        }
    }
    let stack_bottom_guard = align_down(stack_bottom - PAGE_SIZE);
    Ok((stack_top, stack_bottom_guard))
}

pub fn compute_phdr_addr(eli: &ElfLoadInfo) -> Result<u64, ElfLoadError> {
    let phoff = eli.phoff;

    for s in &eli.segments {
        if s.file_size == 0 {
            continue;
        }

        let seg_file_start = s.file_offset;
        let seg_file_end = seg_file_start
            .checked_add(s.file_size)
            .ok_or(ElfLoadError::VirtAddrOverflow)?;

        if seg_file_start <= phoff && phoff < seg_file_end {
            let delta = phoff - seg_file_start;
            return Ok(s.virt_addr + delta);
        }
    }

    Err(ElfLoadError::PhdrNotMappedByLoad)
}

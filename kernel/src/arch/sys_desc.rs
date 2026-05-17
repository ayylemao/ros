use alloc::vec::Vec;
use shared::{PAGE_SIZE, PHYS_OFFSET};
use x86_64::{
    structures::paging::{
        FrameAllocator, Mapper, OffsetPageTable, Page, PageTableFlags, PhysFrame, Size4KiB,
        Translate,
    },
    PhysAddr, VirtAddr,
};

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct RsdpV1 {
    signature: [u8; 8],
    checksum: u8,
    oemid: [u8; 6],
    revision: u8,
    rsdt_adress: u32,
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct RsdpV2 {
    v1: RsdpV1,
    length: u32,
    xsdt_address: u64,
    extended_checksum: u8,
    reserved: [u8; 3],
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct AcpiSdtHeader {
    signature: [u8; 4],
    length: u32,
    revision: u8,
    cheksum: u8,
    oem_id: [u8; 6],
    oem_table_id: [u8; 8],
    oem_revision: u32,
    creator_id: u32,
    creater_revision: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct SdtRef {
    pub phys_addr: u64,
    pub hdr: AcpiSdtHeader,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Madt {
    hdr: AcpiSdtHeader,
    lapic_addr: u32,
    flags: u32,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MadtEntryHeader {
    pub entry_type: u8,
    pub length: u8,
}

#[allow(dead_code)]
#[repr(C, packed)] // 0
#[derive(Debug, Clone, Copy)]
pub struct LapicEntry {
    pub hdr: MadtEntryHeader,
    pub proc_id: u8,
    pub apic_id: u8,
    pub flags: u32,
}

#[repr(C, packed)] // 1
#[derive(Debug, Clone, Copy)]
pub struct IoApic {
    pub hdr: MadtEntryHeader,
    pub proc_id: u8,
    pub reserved: u8,
    pub addr: u32,
    pub global_sys_int_base: u32,
}

#[repr(C, packed)] // 2
#[derive(Debug, Clone, Copy)]
pub struct IoApicIntSourceOverride {
    pub hdr: MadtEntryHeader,
    pub bus_src: u8,
    pub irq_src: u8,
    pub global_sys_interrupt: u32,
    pub flags: u16,
}

#[allow(dead_code)]
#[repr(C, packed)] // 3
#[derive(Debug, Clone, Copy)]
pub struct IoApicNonMaskableInterruptSource {
    hdr: MadtEntryHeader,
    nmi_src: u8,
    reserved: u8,
    flags: u16,
    global_system_interrupt: u32,
}

#[allow(dead_code)]
#[repr(C, packed)] // 4
#[derive(Debug, Clone, Copy)]
pub struct LapicNonMaskableInterrupts {
    pub hdr: MadtEntryHeader,
    pub proc_id: u8,
    pub flags: u16,
    pub lint: u8,
}

#[repr(C, packed)] // 5
#[derive(Debug, Clone, Copy)]
pub struct LapicAdressOverride {
    pub hdr: MadtEntryHeader,
    pub reserved: u16,
    pub lapic_addr: u64,
}

#[allow(dead_code)]
#[repr(C, packed)] // 9
#[derive(Debug, Clone, Copy)]
pub struct LocalxTwoApic {
    pub hdr: MadtEntryHeader,
    pub reserved: u16,
    pub proc_id: u32,
    pub flags: u32,
    pub apic_id: u32,
}

#[derive(Debug)]
pub struct ApicInfo {
    pub ioapics: Vec<IoApic>,
    pub isos: Vec<IoApicIntSourceOverride>,
    pub lapic_addr_override: Option<u64>,
}

#[inline]
fn checksum8(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |acc, &b| acc.wrapping_add(b))
}

#[inline]
fn checksum_ok(bytes: &[u8]) -> bool {
    checksum8(bytes) == 0
}

pub(crate) fn map_to_phys_offset(
    phys_addr: u64,
    len: usize,
    mapper: &mut OffsetPageTable,
    alloc: &mut impl FrameAllocator<Size4KiB>,
) -> u64 {
    let page_sz = PAGE_SIZE as u64;

    let phys_start = phys_addr & !(page_sz - 1);
    let phys_end = (phys_addr + len as u64 + page_sz - 1) & !(page_sz - 1);
    let pages = (phys_end - phys_start) / page_sz;

    for i in 0..pages {
        let p = PhysAddr::new(phys_start + i * page_sz);
        let v = VirtAddr::new(PHYS_OFFSET + phys_start + i * page_sz);

        if let Some(existing) = mapper.translate_addr(v) {
            assert_eq!(existing.align_down(page_sz), p);
            continue;
        }

        unsafe {
            mapper
                .map_to(
                    Page::<Size4KiB>::containing_address(v),
                    PhysFrame::<Size4KiB>::containing_address(p),
                    PageTableFlags::PRESENT
                        | PageTableFlags::NO_EXECUTE
                        | PageTableFlags::NO_CACHE
                        | PageTableFlags::WRITE_THROUGH,
                    alloc,
                )
                .unwrap()
                .flush();
        }
    }

    PHYS_OFFSET + phys_addr
}

pub(crate) fn map_mmio(
    phys_addr: u64,
    len: usize,
    mapper: &mut OffsetPageTable,
    alloc: &mut impl FrameAllocator<Size4KiB>,
) -> u64 {
    let page_sz = PAGE_SIZE as u64;

    let phys_start = phys_addr & !(page_sz - 1);
    let phys_end = (phys_addr + len as u64 + page_sz - 1) & !(page_sz - 1);
    let pages = (phys_end - phys_start) / page_sz;

    for i in 0..pages {
        let p = PhysAddr::new(phys_start + i * page_sz);
        let v = VirtAddr::new(PHYS_OFFSET + phys_start + i * page_sz);

        if let Some(existing) = mapper.translate_addr(v) {
            assert_eq!(existing.align_down(page_sz), p);
            continue;
        }

        unsafe {
            mapper
                .map_to(
                    Page::<Size4KiB>::containing_address(v),
                    PhysFrame::<Size4KiB>::containing_address(p),
                    PageTableFlags::PRESENT
                        | PageTableFlags::WRITABLE
                        | PageTableFlags::NO_EXECUTE
                        | PageTableFlags::NO_CACHE
                        | PageTableFlags::WRITE_THROUGH,
                    alloc,
                )
                .unwrap()
                .flush();
        }
    }

    PHYS_OFFSET + phys_addr
}

pub struct SystemDescriptors {
    xsdt_headers: Vec<SdtRef>,
}

impl SystemDescriptors {
    pub fn init(
        rsdp_phys: u64,
        mapper: &mut OffsetPageTable,
        alloc: &mut impl FrameAllocator<Size4KiB>,
    ) -> SystemDescriptors {
        let rsdp = Self::load_rsdp(rsdp_phys, mapper, alloc);
        let xsdt_headers = Self::parse_xsdt_tbl_hdrs(&rsdp, mapper, alloc);
        Self { xsdt_headers }
    }

    pub fn validate_rsdp_v1(rsdp: &RsdpV1) -> bool {
        let bytes = unsafe {
            core::slice::from_raw_parts(
                (rsdp as *const RsdpV1) as *const u8,
                core::mem::size_of::<RsdpV1>(),
            )
        };
        checksum_ok(bytes)
    }

    pub fn validate_rsdp_v2(rsdp: &RsdpV2) -> bool {
        if !Self::validate_rsdp_v1(&rsdp.v1) {
            return false;
        }

        let len = rsdp.length as usize;
        let bytes =
            unsafe { core::slice::from_raw_parts((rsdp as *const RsdpV2) as *const u8, len) };
        checksum_ok(bytes)
    }

    pub fn load_rsdp(
        rsdp_phys: u64,
        mapper: &mut OffsetPageTable,
        alloc: &mut impl FrameAllocator<Size4KiB>,
    ) -> RsdpV2 {
        let rsdp_virt =
            map_to_phys_offset(rsdp_phys, core::mem::size_of::<RsdpV2>(), mapper, alloc);
        let rsdp = unsafe { &*(rsdp_virt as *const RsdpV2) };

        assert_eq!(&rsdp.v1.signature, b"RSD PTR ");
        assert!(rsdp.v1.revision >= 2);

        let _ = map_to_phys_offset(rsdp_phys, rsdp.length as usize, mapper, alloc);
        assert!(Self::validate_rsdp_v2(rsdp));

        *rsdp
    }

    pub fn parse_xsdt_tbl_hdrs(
        rsdp: &RsdpV2,
        mapper: &mut OffsetPageTable,
        alloc: &mut impl FrameAllocator<Size4KiB>,
    ) -> Vec<SdtRef> {
        let xsdt_hdr_virt = map_to_phys_offset(
            rsdp.xsdt_address,
            core::mem::size_of::<AcpiSdtHeader>(),
            mapper,
            alloc,
        );

        let xsdt_hdr = unsafe { &*(xsdt_hdr_virt as *const AcpiSdtHeader) };
        assert_eq!(&xsdt_hdr.signature, b"XSDT");

        let total_len = xsdt_hdr.length as usize;
        let header_len = core::mem::size_of::<AcpiSdtHeader>();
        assert!(total_len >= header_len);

        let xsdt_virt = map_to_phys_offset(rsdp.xsdt_address, total_len, mapper, alloc);

        let payload_len = total_len - header_len;
        assert!(payload_len & 7 == 0);

        let entry_count = payload_len / 8;
        let entries_ptr = (xsdt_virt as usize + header_len) as *const u64;

        let mut headers: Vec<SdtRef> = Vec::new();
        for i in 0..entry_count {
            let phys_addr = unsafe { core::ptr::read_unaligned(entries_ptr.add(i)) };

            let entry_hdr_virt = map_to_phys_offset(
                phys_addr,
                core::mem::size_of::<AcpiSdtHeader>(),
                mapper,
                alloc,
            );

            let sdt_hdr = unsafe { &*(entry_hdr_virt as *const AcpiSdtHeader) };

            headers.push(SdtRef {
                phys_addr,
                hdr: *sdt_hdr,
            });
        }

        headers
    }

    pub fn madt_for_each_entry<F>(madt_virt: u64, madt_len: u64, mut f: F)
    where
        F: FnMut(&MadtEntryHeader, u64), // (header, entry_ptr)
    {
        let entries_start = madt_virt + core::mem::size_of::<Madt>() as u64;
        let entries_end = madt_virt + madt_len;

        let mut p = entries_start;
        while p < entries_end {
            let eh = unsafe { &*(p as *const MadtEntryHeader) };
            if eh.length < 2 {
                break;
            }
            let next = p + eh.length as u64;
            if next > entries_end {
                break;
            }

            f(eh, p);

            p = next;
        }
    }

    pub fn parse_apic_info(
        &self,
        mapper: &mut OffsetPageTable,
        alloc: &mut impl FrameAllocator<Size4KiB>,
    ) -> Option<ApicInfo> {
        let madt_ref = self
            .xsdt_headers
            .iter()
            .find(|x| x.hdr.signature == *b"APIC")?;

        let madt_hdr_virt = map_to_phys_offset(
            madt_ref.phys_addr,
            core::mem::size_of::<AcpiSdtHeader>(),
            mapper,
            alloc,
        );
        let madt_hdr = unsafe { &*(madt_hdr_virt as *const AcpiSdtHeader) };
        if &madt_hdr.signature != b"APIC" {
            return None;
        }

        let madt_len = madt_hdr.length as usize;
        let madt_virt = map_to_phys_offset(madt_ref.phys_addr, madt_len, mapper, alloc);

        let bytes = unsafe { core::slice::from_raw_parts(madt_virt as *const u8, madt_len) };
        assert!(checksum_ok(bytes));

        let mut info = ApicInfo {
            ioapics: Vec::new(),
            isos: Vec::new(),
            lapic_addr_override: None,
        };

        Self::madt_for_each_entry(madt_virt, madt_len as u64, |eh, p| match eh.entry_type {
            1 => {
                let e = unsafe { &*(p as *const IoApic) };
                info.ioapics.push(*e);
            }
            2 => {
                let e = unsafe { &*(p as *const IoApicIntSourceOverride) };
                info.isos.push(*e);
            }
            5 => {
                let e = unsafe { &*(p as *const LapicAdressOverride) };
                info.lapic_addr_override = Some(e.lapic_addr);
            }
            _ => {}
        });

        Some(info)
    }

    #[allow(dead_code)]
    pub fn resolve_isa_irq(info: &ApicInfo, irq: u8) -> (u32, u16) {
        if let Some(iso) = info
            .isos
            .iter()
            .find(|x| x.bus_src == 0 && x.irq_src == irq)
        {
            (iso.global_sys_interrupt, iso.flags)
        } else {
            (irq as u32, 0)
        }
    }
}

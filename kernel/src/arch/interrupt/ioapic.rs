use core::ptr::{read_volatile, write_volatile};

use x86_64::VirtAddr;

#[derive(Debug, Clone, Copy)]
pub struct IoApicMmio {
    base: VirtAddr,
    gsi_base: u32,
}

impl IoApicMmio {
    pub unsafe fn new(base: VirtAddr, gsi_base: u32) -> Self {
        Self { base, gsi_base }
    }

    #[inline(always)]
    unsafe fn reg_sel(&self) -> *mut u32 {
        self.base.as_mut_ptr::<u32>()
    }

    #[inline(always)]
    unsafe fn reg_win(&self) -> *mut u32 {
        self.base.as_u64().wrapping_add(0x10) as *mut u32 // offset 0x10
    }
    unsafe fn read(&self, reg: u8) -> u32 {
        write_volatile(self.reg_sel(), reg as u32);
        read_volatile(self.reg_win())
    }

    unsafe fn write(&self, reg: u8, val: u32) {
        write_volatile(self.reg_sel(), reg as u32);
        write_volatile(self.reg_win(), val);
    }

    pub unsafe fn max_redirection_entry(&self) -> u8 {
        // IOAPICVER register (0x01): bits 16..23 = max redir entry
        let ver = self.read(0x01);
        ((ver >> 16) & 0xFF) as u8
    }

    /// Route one GSI to a vector on a destination LAPIC ID.
    /// `flags` are from MADT ISO (MPS INTI flags): polarity/trigger.
    pub unsafe fn set_gsi_redirect(&self, gsi: u32, vector: u8, dest_lapic_id: u8, flags: u16) {
        let idx = gsi
            .checked_sub(self.gsi_base)
            .expect("GSI below this IOAPIC base");

        let max = self.max_redirection_entry() as u32;
        assert!(idx <= max, "GSI not covered by this IOAPIC");

        // MPS INTI flags decoding (ACPI/MADT ISO):
        // Polarity: bits 0..1  (0 conform, 1 active-high, 3 active-low)
        // Trigger : bits 2..3  (0 conform, 1 edge,       3 level)
        let pol = flags & 0b11;
        let trg = (flags >> 2) & 0b11;

        // Defaults (conforming for ISA IRQs): active high + edge
        let active_low = pol == 0b11;
        let level_triggered = trg == 0b11;

        // Redirection entry (64-bit)
        // low dword:
        //  bits 0..7   vector
        //  bits 8..10  delivery mode (0 = Fixed)
        //  bit  11     destination mode (0 = physical)
        //  bit  13     pin polarity (1 = active low)
        //  bit  15     trigger mode (1 = level)
        //  bit  16     mask (1 = masked)
        let mut low: u32 = vector as u32; // Fixed, physical, unmasked
        if active_low {
            low |= 1 << 13;
        }
        if level_triggered {
            low |= 1 << 15;
        }

        // high dword: destination in bits 56..63 => high bits 24..31
        let high: u32 = (dest_lapic_id as u32) << 24;

        let redir_reg = 0x10 + (idx as u8) * 2;
        self.write(redir_reg, low);
        self.write(redir_reg + 1, high);
    }
}

/// Pick the IOAPIC that covers `gsi`.
pub unsafe fn find_ioapic_for_gsi<'a>(
    ioapics: &'a [IoApicMmio],
    gsi: u32,
) -> Option<&'a IoApicMmio> {
    for ioa in ioapics {
        let max = ioa.max_redirection_entry() as u32;
        let start = ioa.gsi_base;
        let end = start + max;
        if (start..=end).contains(&gsi) {
            return Some(ioa);
        }
    }
    None
}

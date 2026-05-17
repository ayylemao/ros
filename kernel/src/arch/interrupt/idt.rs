use core::sync::atomic::AtomicU64;

use x86_64::{
    instructions::{interrupts::enable_and_hlt, port::Port},
    registers::control::Cr2,
    structures::{
        idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode},
        paging::{FrameAllocator, OffsetPageTable, Size4KiB},
    },
    PrivilegeLevel, VirtAddr,
};

use crate::{
    arch::{
        gdt,
        interrupt::{
            lapic::LocalApic,
            syscall_entry::int80_entry,
            timer_trap::timer_entry,
            types::{self, BreakpointException},
        },
    },
    kprintln,
    proc::sched,
    utils::ringbuffer::SpscRing,
};

static mut IDT: InterruptDescriptorTable = InterruptDescriptorTable::new();
pub static TICKS: AtomicU64 = AtomicU64::new(0);
pub static KEYBOARD_VEC: u8 = 0x41;

pub static BR_BUF: SpscRing<types::BreakpointException, 256> = SpscRing::new();
pub static KEYBOARD_BUF: SpscRing<u8, 256> = SpscRing::new();

#[allow(static_mut_refs)]
pub fn init_idt(mapper: &mut OffsetPageTable, allocator: &mut impl FrameAllocator<Size4KiB>) {
    unsafe {
        IDT.breakpoint.set_handler_fn(breakpoint_handler);
        IDT.double_fault
            .set_handler_fn(double_fault_handler)
            .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
        IDT.page_fault.set_handler_fn(page_fault_handler);
        IDT.divide_error.set_handler_fn(divide_error_handler);
        IDT.general_protection_fault.set_handler_fn(gp_handler);

        IDT[0x40].set_handler_addr(VirtAddr::new(timer_entry as *const () as u64));

        IDT[KEYBOARD_VEC].set_handler_fn(keyboard_interrupt_handler);

        IDT[0x80]
            .set_handler_addr(VirtAddr::new(int80_entry as *const () as u64))
            .set_privilege_level(PrivilegeLevel::Ring3);

        IDT[0xFF].set_handler_fn(spurious_interrupt_handler);
        IDT.load();
    }
    LocalApic::init(mapper, allocator);
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    let sf = BreakpointException::new(&stack_frame);
    let _ = BR_BUF.push(sf);
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    kprintln!(
        "EXCEPTION: Double Fault\nError Code: {}\n{:#?}",
        _error_code,
        stack_frame
    );
    loop {}
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    kprintln!(
        "EXCEPTION: PAGE FAULT\nAddress: {:?}\nError: {:#?}\n{:#?}",
        Cr2::read(),
        error_code,
        stack_frame
    );
    loop {
        enable_and_hlt();
    }
}

extern "x86-interrupt" fn gp_handler(stack_frame: InterruptStackFrame, error_code: u64) {
    kprintln!(
        "#GP fault! err=0x{:x} rip={:?}",
        error_code,
        stack_frame.instruction_pointer,
    );
    kprintln!("rsp 0x{:x}", stack_frame.stack_pointer);
    loop {
        enable_and_hlt();
    }
}

extern "x86-interrupt" fn divide_error_handler(_stack_frame: InterruptStackFrame) {
    kprintln!("Division by zero!");
}

extern "x86-interrupt" fn spurious_interrupt_handler(_stack_frame: InterruptStackFrame) {}

extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    let sc = unsafe { Port::<u8>::new(0x60).read() };
    let _ = KEYBOARD_BUF.push(sc);
    // Keep IRQ handler minimal: decoding/echoing can take locks and can deadlock
    // if we interrupt code already holding them. Just wake blocked readers so
    // they can drain/decode in syscall context.
    sched::wake_tty_readers();
    LocalApic::eoi();
}

use x86_64::{
    registers::rflags::RFlags,
    structures::{gdt::SegmentSelector, idt::InterruptStackFrame},
    VirtAddr,
};

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
struct InterruptStackFrameCopy {
    instruction_pointer: VirtAddr,
    code_segment: SegmentSelector,
    cpu_flags: RFlags,
    stack_pointer: VirtAddr,
    stack_segment: SegmentSelector,
}

impl InterruptStackFrameCopy {
    #[inline]
    pub fn from_isf(isf: &InterruptStackFrame) -> Self {
        Self {
            instruction_pointer: (*isf).instruction_pointer,
            code_segment: (*isf).code_segment,
            cpu_flags: (*isf).cpu_flags,
            stack_pointer: (*isf).stack_pointer,
            stack_segment: (*isf).stack_segment,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct BreakpointException {
    stack_frame: InterruptStackFrameCopy,
}

impl BreakpointException {
    #[inline]
    pub fn new(isf: &InterruptStackFrame) -> Self {
        Self {
            stack_frame: InterruptStackFrameCopy::from_isf(isf),
        }
    }
}

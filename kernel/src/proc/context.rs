use core::arch::global_asm;

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct KernelContext {
    pub rsp: u64,
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbx: u64,
    pub rbp: u64,
}

extern "C" {
    pub fn context_switch(old: *mut KernelContext, new: *const KernelContext);
    pub fn iret_trampoline() -> !;
}

global_asm!(
    r#"
.global context_switch
.type context_switch, @function
context_switch:
    // rdi = old, rsi = new
    // Save callee-saved registers + stack pointer
    mov [rdi + 0x00], rsp
    mov [rdi + 0x08], r15
    mov [rdi + 0x10], r14
    mov [rdi + 0x18], r13
    mov [rdi + 0x20], r12
    mov [rdi + 0x28], rbx
    mov [rdi + 0x30], rbp

    // Restore callee-saved registers + stack pointer
    mov rsp, [rsi + 0x00]
    mov r15, [rsi + 0x08]
    mov r14, [rsi + 0x10]
    mov r13, [rsi + 0x18]
    mov r12, [rsi + 0x20]
    mov rbx, [rsi + 0x28]
    mov rbp, [rsi + 0x30]

    ret

.global iret_trampoline
.type iret_trampoline, @function
iret_trampoline:
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rsi
    pop rdi
    pop rbp
    pop rdx
    pop rcx
    pop rbx
    pop rax
    add rsp, 16

    test byte ptr [rsp + 8], 3
    jz 1f
    swapgs
    1:
    iretq
"#
);

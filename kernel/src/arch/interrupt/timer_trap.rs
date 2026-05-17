use core::{arch::global_asm, sync::atomic::Ordering};

use crate::{
    arch::interrupt::{idt::TICKS, lapic::LocalApic},
    console,
    proc::{self},
    syscall::syscall_dispatch::TrapFrame,
};

extern "C" {
    pub fn timer_entry();
}

fn timer_interrupt_handler(tf: &mut TrapFrame) {
    let now = TICKS.fetch_add(1, Ordering::AcqRel);

    proc::sched::on_timer_tick(tf, now);

    // Send EOI early so a bug in the scheduling path can't permanently block
    // further interrupts by keeping this vector "in service".
    LocalApic::eoi();

    proc::sched::schedule_irq(tf);

    if now % 10 == 0 {
        console::console::try_flush_console();
    }
}

#[no_mangle]
pub extern "C" fn timer_dispatch(tf: *mut TrapFrame) {
    let tf = unsafe { &mut *tf };
    timer_interrupt_handler(tf);
}

global_asm!(
    r#"
.global timer_entry
.type timer_entry, @function
timer_entry:
    cli
    cld

    test byte ptr [rsp + 8], 3
    jz 1f
    swapgs
    1:

    push 0
    push 0x40

    push rax
    push rbx
    push rcx
    push rdx
    push rbp
    push rdi
    push rsi
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15

    mov rbp, rsp

    and rsp, -16
    mov rdi, rbp
    call timer_dispatch
    mov rsp, rbp

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
    jz 2f
    swapgs
2:
    iretq
"#
);

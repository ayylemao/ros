use core::arch::global_asm;

use x86_64::{instructions::interrupts, registers::model_specific::KernelGsBase};

use crate::{
    proc::{sched::in_user, task_manager::TaskManager},
    syscall::syscall_dispatch::{syscall_dispatch, TrapFrame},
};

extern "C" {
    pub fn int80_entry();
    pub fn syscall_entry();
}

#[no_mangle]
pub extern "C" fn trap_dispatch(tf: *mut TrapFrame) {
    let tf = unsafe { &mut *tf };

    if in_user(tf) {
        let current_task = TaskManager::current_tid();
        let mut tm = TaskManager::get().lock();
        let task = tm
            .tasks
            .get_mut(&current_task)
            .expect("No current user task entering syscall, bailing out...");
        task.user_gs_base = KernelGsBase::read().as_u64();
    }

    interrupts::enable();
    syscall_dispatch(tf);
}

global_asm!(
    r#"
.global syscall_entry
.type syscall_entry, @function
syscall_entry:
    cld

    // Enter kernel GS (percpu)
    swapgs

    // Save user RSP into percpu scratch WITHOUT clobbering GPRs:
    // PerCpu.user_rsp @ gs:8
    mov qword ptr gs:[8], rsp

    // Switch to kernel RSP (PerCpu.kernel_rsp @ gs:0)
    mov rsp, qword ptr gs:[0]

    // --- Build synthetic IRET frame for iretq ---
    // Order for iretq pop: RIP, CS, RFLAGS, RSP, SS
    // So we push reverse: SS, RSP, RFLAGS, CS, RIP

    // SS
    push 0x23

    // RSP (user)
    push qword ptr gs:[8]

    // RFLAGS (user) is in r11 per SYSCALL contract
    push r11

    // CS
    push 0x1B

    // RIP (user) is in rcx per SYSCALL contract
    push rcx

    // --- Match your existing TrapFrame header ---
    push 0            // error_code
    push 0x80         // vector (fake "syscall vector")

    // --- Save GPRs exactly like int80_entry ---
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

    // rbp = &TrapFrame (like you do today)
    mov rbp, rsp

    // Keep 16-byte alignment for the Rust call
    and rsp, -16
    mov rdi, rbp
    call trap_dispatch
    mov rsp, rbp

    // Restore regs
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

    // Drop vector + error
    add rsp, 16

    // Return GS to user view before returning to CPL3
    swapgs

    iretq
"#
);

global_asm!(
    r#"
.global int80_entry
.type int80_entry, @function
int80_entry:
    cld

    swapgs

    push 0
    push 0x80

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
    call trap_dispatch
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

    swapgs
    iretq
"#
);

use core::ptr;

use alloc::{
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use shared::{align_up, PAGE_SIZE, USER_SPACE_MAX_HEAP_SIZE};
use spin::Mutex;
use sys::{syscall::errors::ProcessError, FileOpenFlags};
use x86_64::{
    align_down,
    structures::paging::{PhysFrame, Size4KiB},
};

use crate::{
    fs::{
        file_descriptor::{FileDescriptor, FileKind, OpenFile},
        vfs::{Vfs, VfsNode},
    },
    kglobal::bmp_alloc,
    memory::bmp_alloc::TrackingAlloc,
    proc::{
        address_space::AddressSpace,
        elf::{compute_phdr_addr, map_elf, map_user_stack, parse_elf},
        mmap_region::MmapRegion,
        signal::{UserSigAction, NSIG},
        task_manager::TaskManager,
    },
    syscall::syscall_dispatch::{GprFrame, IretFrame},
};

const RFLAGS_IF: u64 = 1 << 9;
const AT_NULL: u64 = 0;
const AT_PHDR: u64 = 3;
const AT_PHENT: u64 = 4;
const AT_PHNUM: u64 = 5;
const AT_PAGESZ: u64 = 6;
const AT_ENTRY: u64 = 9;
const AT_RANDOM: u64 = 25;

#[derive(Debug, Clone, Copy)]
pub struct UserContext {
    pub regs: GprFrame,
    pub iret: IretFrame,
}

#[derive(Debug)]
pub struct InitialTaskInfo {
    pub entry_rip: u64,
    pub user_rsp: u64,
    pub argc: u64,
    pub argv_ptr: u64,
    pub kstack_top: u64,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Process {
    pub cmd: String,
    pub pid: u64,
    pub address_space: AddressSpace,
    pub fds: Vec<Option<Arc<FileDescriptor>>>,
    pub cwd_node: VfsNode,
    pub allocated_frames: Vec<PhysFrame<Size4KiB>>,
    pub parent_pid: Option<u64>,
    pub tasks: Vec<u64>,
    pub ticks_run: u64,
    pub sigactions: [UserSigAction; NSIG],
    pub sigmask: u64,
    pub brk_start: u64,
    pub brk: u64,
    pub brk_max: u64,
    pub mmaps: Vec<MmapRegion>,
    pub mmap_top: u64,
    pub umask: u16,
}

impl Process {
    pub fn new(
        pid: u64,
        image_path: &str,
        parent_pid: Option<u64>,
        args: &[&str],
        cwd_node: VfsNode,
        tid: u64,
    ) -> Result<(Self, InitialTaskInfo), ProcessError> {
        let mut bmp_guard = bmp_alloc().lock();
        let mut tracking_alloc = TrackingAlloc::new(&mut *bmp_guard);

        let address_space =
            AddressSpace::new_user(&mut tracking_alloc).map_err(|_| ProcessError::OutOfMemory)?;

        let name = image_path.rsplit('/').next().unwrap().to_string();

        let elf_buf: Vec<u8> = {
            let vfs = Vfs::get().lock();
            let node = vfs.resolve(cwd_node, image_path)?;
            vfs.read_all(node)?
        };

        let elf_load_info = parse_elf(elf_buf.as_slice())?;

        let (stack_top, brk, _stack_bottom, argc, argv_ptr, kstack_top) =
            x86_64::instructions::interrupts::without_interrupts(|| {
                let old_pml4 = address_space.activate();

                let mut mapper = unsafe { address_space.mapper() };

                let max_mapped_va = map_elf(&elf_load_info, &mut tracking_alloc, &mut mapper)?;
                let heap_start = align_up(max_mapped_va + PAGE_SIZE);

                let (stack_top, stack_bottom) = map_user_stack(&mut tracking_alloc, &mut mapper)?;

                let phdr_addr = compute_phdr_addr(&elf_load_info)?;
                let (stack_top, argc, argv_ptr) = Self::build_user_argv(
                    stack_top,
                    stack_bottom,
                    args,
                    elf_load_info.entry_point,
                    phdr_addr,
                    elf_load_info.phentsize as u64,
                    elf_load_info.phnum as u64,
                )?;

                let kstack_top = TaskManager::alloc_and_map_kstack_for_task(
                    tid,
                    &mut mapper,
                    &mut tracking_alloc,
                )?;

                AddressSpace::restore(old_pml4);
                Ok::<(u64, u64, u64, u64, u64, u64), ProcessError>((
                    stack_top,
                    heap_start,
                    stack_bottom,
                    argc,
                    argv_ptr,
                    kstack_top,
                ))
            })?;

        let mut fds: Vec<Option<Arc<FileDescriptor>>> = Vec::new();
        fds.push(Some(Arc::new(FileDescriptor {
            file: Mutex::new(OpenFile {
                kind: FileKind::Tty,
                offset: 0,
                flags: FileOpenFlags::READ,
                cache: None,
            }),
        })));
        fds.push(Some(Arc::new(FileDescriptor {
            file: Mutex::new(OpenFile {
                kind: FileKind::Tty,
                offset: 0,
                flags: FileOpenFlags::WRITE,
                cache: None,
            }),
        })));

        fds.push(Some(Arc::new(FileDescriptor {
            file: Mutex::new(OpenFile {
                kind: FileKind::Tty,
                offset: 0,
                flags: FileOpenFlags::WRITE,
                cache: None,
            }),
        })));
        let mmap_top = stack_top - 0x0100_0000;
        let brk_max = (brk + USER_SPACE_MAX_HEAP_SIZE).min(mmap_top - PAGE_SIZE);
        let mut tasks = Vec::new();
        tasks.push(tid);
        Ok((
            Self {
                cmd: name,
                pid,
                address_space,
                allocated_frames: Vec::new(),
                fds,
                parent_pid,
                brk_start: brk,
                brk,
                brk_max: brk_max,
                cwd_node,
                tasks,
                ticks_run: 0,
                sigactions: [UserSigAction::default(); NSIG],
                sigmask: 0,
                mmaps: Vec::new(),
                mmap_top,
                umask: 0o022,
            },
            InitialTaskInfo {
                entry_rip: elf_load_info.entry_point,
                user_rsp: stack_top,
                argc,
                argv_ptr,
                kstack_top,
            },
        ))
    }

    #[allow(dead_code)]
    pub fn alloc_fd(&mut self, f: FileDescriptor) -> usize {
        for (i, slot) in self.fds.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(Arc::new(f));
                return i as usize;
            }
        }
        self.fds.push(Some(Arc::new(f)));
        self.fds.len() - 1
    }

    pub fn make_initial_user_ctx(
        entry_rip: u64,
        user_stack_top: u64,
        argc: u64,
        argv_ptr: u64,
    ) -> UserContext {
        let mut regs = GprFrame::default();

        regs.rdi = argc;
        regs.rsi = argv_ptr;

        let user_cs = crate::arch::gdt::user_code_selector().0 as u64;
        let user_ss = crate::arch::gdt::user_data_selector().0 as u64;

        let iret = IretFrame {
            rip: entry_rip,
            cs: user_cs,
            rflags: 0x2 | (1 << 9), // IF=1
            rsp: user_stack_top,
            ss: user_ss,
        };

        UserContext { regs, iret }
    }

    pub fn get_cwd(&self) -> VfsNode {
        return self.cwd_node;
    }

    pub fn set_cwd(&mut self, node: VfsNode) {
        self.cwd_node = node;
    }

    pub fn build_user_argv(
        stack_top: u64,
        stack_bottom: u64,
        args: &[&str],
        entry: u64,
        phdr_addr: u64,
        phent: u64,
        phnum: u64,
    ) -> Result<(u64, u64, u64), ProcessError> {
        let argc = args.len() as u64;
        let mut sp = stack_top;

        let mut arg_ptrs: alloc::vec::Vec<u64> = alloc::vec::Vec::with_capacity(args.len());

        for s in args.iter() {
            let bytes = s.as_bytes();
            let n = bytes.len() + 1;

            sp = sp.checked_sub(n as u64).ok_or(ProcessError::OutOfMemory)?;
            if sp < stack_bottom + 256 {
                return Err(ProcessError::OutOfMemory);
            }

            unsafe {
                ptr::copy_nonoverlapping(bytes.as_ptr(), sp as *mut u8, bytes.len());
                *(sp as *mut u8).add(bytes.len()) = 0;
            }

            arg_ptrs.push(sp);
        }

        sp = align_down(sp, 8);

        let random_ptr = {
            sp = sp.checked_sub(16).ok_or(ProcessError::OutOfMemory)?;
            if sp < stack_bottom + 256 {
                return Err(ProcessError::OutOfMemory);
            }
            unsafe {
                let p = sp as *mut u8;
                for i in 0..16 {
                    *p.add(i) = (0xA5u8).wrapping_add(i as u8);
                }
            }
            sp
        };

        unsafe {
            push_u64(&mut sp, stack_bottom, 0)?;
            push_u64(&mut sp, stack_bottom, AT_NULL)?;

            push_u64(&mut sp, stack_bottom, random_ptr)?;
            push_u64(&mut sp, stack_bottom, AT_RANDOM)?;

            push_u64(&mut sp, stack_bottom, entry)?;
            push_u64(&mut sp, stack_bottom, AT_ENTRY)?;

            push_u64(&mut sp, stack_bottom, phnum)?;
            push_u64(&mut sp, stack_bottom, AT_PHNUM)?;

            push_u64(&mut sp, stack_bottom, phent)?;
            push_u64(&mut sp, stack_bottom, AT_PHENT)?;

            push_u64(&mut sp, stack_bottom, phdr_addr)?;
            push_u64(&mut sp, stack_bottom, AT_PHDR)?;

            push_u64(&mut sp, stack_bottom, PAGE_SIZE)?;
            push_u64(&mut sp, stack_bottom, AT_PAGESZ)?;
        }

        unsafe {
            push_u64(&mut sp, stack_bottom, 0)?;
        }

        unsafe {
            push_u64(&mut sp, stack_bottom, 0)?;
        }

        for p in arg_ptrs.iter().rev() {
            unsafe {
                push_u64(&mut sp, stack_bottom, *p)?;
            }
        }

        let argv_ptr = sp;

        unsafe {
            push_u64(&mut sp, stack_bottom, argc)?;
        }

        let rsp_final = sp;
        Ok((rsp_final, argc, argv_ptr))
    }
}

#[inline(always)]
unsafe fn push_u64(sp: &mut u64, stack_bottom: u64, val: u64) -> Result<(), ProcessError> {
    *sp = sp.checked_sub(8).ok_or(ProcessError::OutOfMemory)?;
    if *sp < stack_bottom + 64 {
        return Err(ProcessError::OutOfMemory);
    }
    *(*sp as *mut u64) = val;
    Ok(())
}

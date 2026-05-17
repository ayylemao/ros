use core::sync::atomic::Ordering;

use alloc::{string::String, vec::Vec};
use shared::{fmt_bytes, HZ};
use spin::Once;
use sys::syscall::{errors::FsError, wrappers::Sysno, FileKindTag};

use crate::{
    arch::interrupt::idt::TICKS,
    fs::{DirEntry, FsBackend},
    kglobal,
    proc::{proc_manager::ProcessManager, task_manager::TaskManager},
    syscall::syscall_dispatch::STRACE_BUF,
    utils::irq_lock::IrqMutex,
};

static PROCFS: Once<IrqMutex<Procfs>> = Once::new();

const TAG_SHIFT: usize = 60;
const TAG_MASK: usize = 0xF << TAG_SHIFT;
const TAG_ROOT: usize = 0;
const TAG_FIXED_FILE: usize = 1 << TAG_SHIFT;
const TAG_PID_DIR: usize = 2 << TAG_SHIFT;
const TAG_PID_FILE: usize = 3 << TAG_SHIFT;

#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FixedFile {
    MemInfo = 1,
    Uptime = 2,
    Strace = 3,
}

fn fixed_id(f: FixedFile) -> usize {
    TAG_FIXED_FILE | (f as usize)
}

fn pid_dir_id(pid: usize) -> usize {
    TAG_PID_DIR | (pid as usize)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcNode {
    Root,
    Fixed(FixedFile),
    PidDir(usize),
}

pub struct Procfs {
    pm: &'static IrqMutex<ProcessManager>,
    tm: &'static IrqMutex<TaskManager>,
}

impl Procfs {
    pub fn new() -> Procfs {
        Procfs {
            pm: ProcessManager::procman(),
            tm: TaskManager::get(),
        }
    }

    pub fn get() -> &'static IrqMutex<Procfs> {
        PROCFS.call_once(|| IrqMutex::new(Procfs::new()))
    }
    fn decode(id: usize) -> Option<ProcNode> {
        if id == TAG_ROOT {
            return Some(ProcNode::Root);
        }

        match id & TAG_MASK {
            TAG_FIXED_FILE => {
                let v = (id & !TAG_MASK) as u16;
                let f = match v {
                    1 => FixedFile::MemInfo,
                    2 => FixedFile::Uptime,
                    3 => FixedFile::Strace,
                    _ => return None,
                };
                return Some(ProcNode::Fixed(f));
            }
            TAG_PID_DIR => {
                let pid = id & !TAG_PID_DIR;
                return Some(ProcNode::PidDir(pid));
            }
            _ => None,
        }
    }

    fn lookup_child(dir: usize, name: &str) -> Result<usize, FsError> {
        match Self::decode(dir).ok_or(FsError::NotFound)? {
            ProcNode::Root => match name {
                "meminfo" => Ok(fixed_id(FixedFile::MemInfo)),
                "uptime" => Ok(fixed_id(FixedFile::Uptime)),
                "strace" => Ok(fixed_id(FixedFile::Strace)),
                _ => Err(FsError::NotFound),
            },
            ProcNode::Fixed(_) => Err(FsError::NotADirectory),
            ProcNode::PidDir(pid) => Ok(pid_dir_id(pid)),
        }
    }
}

#[allow(unused_variables)]
impl FsBackend for Procfs {
    type NodeId = usize;

    fn append(
        &mut self,
        file: Self::NodeId,
        bytes: &[u8],
    ) -> Result<(), sys::syscall::errors::FsError> {
        Err(FsError::PermissionDenied)
    }

    fn create(
        &mut self,
        dir: Self::NodeId,
        name: &str,
    ) -> Result<Self::NodeId, sys::syscall::errors::FsError> {
        Err(FsError::PermissionDenied)
    }

    fn file_read_borrow(&self, file: Self::NodeId) -> Result<&[u8], sys::syscall::errors::FsError> {
        Err(FsError::PermissionDenied)
    }

    fn mkdir(
        &mut self,
        dir: Self::NodeId,
        name: &str,
    ) -> Result<Self::NodeId, sys::syscall::errors::FsError> {
        Err(FsError::PermissionDenied)
    }

    fn write_at(
        &mut self,
        file: Self::NodeId,
        off: usize,
        bytes: &[u8],
    ) -> Result<usize, sys::syscall::errors::FsError> {
        Err(FsError::PermissionDenied)
    }

    fn write_trunc(
        &mut self,
        file: Self::NodeId,
        bytes: &[u8],
    ) -> Result<usize, sys::syscall::errors::FsError> {
        Err(FsError::PermissionDenied)
    }

    fn root(&self) -> Self::NodeId {
        0
    }

    fn lookup(
        &self,
        dir: Self::NodeId,
        name: &str,
    ) -> Result<Self::NodeId, sys::syscall::errors::FsError> {
        Self::lookup_child(dir, name)
    }

    fn node_type(&self, id: Self::NodeId) -> Result<sys::syscall::FileKindTag, FsError> {
        match Self::decode(id).ok_or(FsError::NotFound)? {
            ProcNode::Root => Ok(FileKindTag::Dir),
            ProcNode::Fixed(_) => Ok(FileKindTag::File),
            ProcNode::PidDir(_) => Ok(FileKindTag::Dir),
        }
    }

    fn readdir(&self, dir: usize) -> Result<Vec<DirEntry>, FsError> {
        match Self::decode(dir).ok_or(FsError::NotFound)? {
            ProcNode::Root => {
                let mut v: Vec<DirEntry> = Vec::new();
                v.push(DirEntry {
                    id: fixed_id(FixedFile::MemInfo),
                    name: "meminfo".into(),
                    ftype: FileKindTag::File,
                });
                v.push(DirEntry {
                    id: fixed_id(FixedFile::Uptime),
                    name: "uptime".into(),
                    ftype: FileKindTag::File,
                });
                v.push(DirEntry {
                    id: fixed_id(FixedFile::Strace),
                    name: "strace".into(),
                    ftype: FileKindTag::File,
                });

                for (id, proc) in &self.pm.lock().procs {
                    v.push(DirEntry {
                        id: pid_dir_id(*id as usize),
                        name: format!("{}", id),
                        ftype: FileKindTag::Dir,
                    });
                }

                Ok(v)
            }
            ProcNode::Fixed(_) => Err(FsError::NotADirectory),
            ProcNode::PidDir(..) => {
                let v: Vec<DirEntry> = Vec::new();
                Ok(v)
            }
        }
    }

    fn parent(&self, id: usize) -> Result<Option<usize>, FsError> {
        match Self::decode(id).ok_or(FsError::NotFound)? {
            ProcNode::Root => Ok(None),
            ProcNode::Fixed(_) => Ok(Some(TAG_ROOT)),
            ProcNode::PidDir(_) => Ok(Some(TAG_ROOT)),
        }
    }

    fn name(&self, id: Self::NodeId) -> Result<alloc::string::String, FsError> {
        match Self::decode(id).ok_or(FsError::NotFound)? {
            ProcNode::Root => Ok("".into()),
            ProcNode::Fixed(FixedFile::MemInfo) => Ok("meminfo".into()),
            ProcNode::Fixed(FixedFile::Uptime) => Ok("uptime".into()),
            ProcNode::Fixed(FixedFile::Strace) => Ok("strace".into()),
            ProcNode::PidDir(pid) => Ok(format!("{}", pid)),
        }
    }

    fn read(&self, file: usize) -> Result<Vec<u8>, FsError> {
        match Self::decode(file).ok_or(FsError::NotFound)? {
            ProcNode::Fixed(FixedFile::MemInfo) => Ok(cmd_mem()),
            ProcNode::Fixed(FixedFile::Uptime) => Ok(uptime_ms()),
            ProcNode::Fixed(FixedFile::Strace) => Ok(flush_strace()),
            ProcNode::Root => Err(FsError::IsADirectory),
            ProcNode::PidDir(_) => Err(FsError::IsADirectory),
        }
    }

    fn len(&self, file: Self::NodeId) -> Result<usize, FsError> {
        match Self::decode(file).ok_or(FsError::NotFound)? {
            ProcNode::Fixed(FixedFile::MemInfo) => Ok(cmd_mem().len()),
            ProcNode::Fixed(FixedFile::Uptime) => Ok(uptime_ms().len()),
            ProcNode::Fixed(FixedFile::Strace) => Ok(0),
            ProcNode::Root => Err(FsError::IsADirectory),
            ProcNode::PidDir(_) => Err(FsError::IsADirectory),
        }
    }
}

fn cmd_mem() -> Vec<u8> {
    let mem_alloc = kglobal::bmp_alloc().lock();
    let free = mem_alloc.get_available_memory() as u64;
    let total = mem_alloc.get_total_memory() as u64;
    let used = total.saturating_sub(free);
    drop(mem_alloc);

    let (t, tu) = fmt_bytes(total);
    let (u, uu) = fmt_bytes(used);
    let (f, fu) = fmt_bytes(free);

    let mut output = String::new();
    output.push_str(&format!("Memory\n"));
    output.push_str(&format!("  total: {:>6} {}\n", t, tu));
    output.push_str(&format!("  used : {:>6} {}\n", u, uu));
    output.push_str(&format!("  free : {:>6} {}\n", f, fu));
    output.as_bytes().to_vec()
}

pub fn uptime_ms() -> Vec<u8> {
    let ticks: u64 = TICKS.load(Ordering::Relaxed);
    let hz: u64 = HZ as u64;

    let uptime = ticks.saturating_mul(1000) / hz;
    format!("{uptime}\n").as_bytes().to_vec()
}

pub fn flush_strace() -> Vec<u8> {
    let mut output = String::new();
    while !STRACE_BUF.is_empty() {
        let strace = STRACE_BUF.pop().unwrap();
        let pname = if let Some(proc) = ProcessManager::procman().lock().get(strace.pid) {
            proc.cmd.clone()
        } else {
            "#NA".into()
        };
        if pname == "shell" {
            continue;
        }

        match Sysno::try_from(strace.sysno) {
            Ok(v) => output.push_str(&format!(
                "{:<6} {:<6} {:<6} {:?}\n",
                strace.pid, pname, strace.sysno, v
            )),
            Err(_e) => output.push_str(&format!(
                "{:<6} {:<6} {:<6} #NA\n",
                strace.pid, pname, strace.sysno
            )),
        }
    }
    output.as_bytes().to_vec()
}

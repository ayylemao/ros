#![allow(dead_code)]
use alloc::vec::Vec;
use spin::Mutex;
use sys::FileOpenFlags;

use crate::fs::vfs::VfsNode;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum FileKind {
    Tty,
    File { node: VfsNode },
    Dir { node: VfsNode },
}

#[derive(Debug, Clone)]
pub struct OpenFile {
    pub kind: FileKind,
    pub offset: usize,
    pub flags: FileOpenFlags,
    pub cache: Option<Vec<u8>>,
}

#[derive(Debug)]
pub struct FileDescriptor {
    pub file: Mutex<OpenFile>,
}

mod procfs;
mod ramfs;

pub mod file_descriptor;
pub mod vfs;

use core::fmt::Debug;

use alloc::{string::String, vec::Vec};
use sys::syscall::{errors::FsError, FileKindTag};

#[derive(Debug, Clone)]
pub struct DirEntry {
    pub id: usize,
    pub name: String,
    pub ftype: FileKindTag,
}
pub trait FsBackend {
    type NodeId: Copy + Debug + Eq;

    fn root(&self) -> Self::NodeId;
    fn node_type(&self, id: Self::NodeId) -> Result<FileKindTag, FsError>;

    fn lookup(&self, dir: Self::NodeId, name: &str) -> Result<Self::NodeId, FsError>;
    fn readdir(&self, dir: Self::NodeId) -> Result<Vec<DirEntry>, FsError>;
    fn mkdir(&mut self, dir: Self::NodeId, name: &str) -> Result<Self::NodeId, FsError>;
    fn create(&mut self, dir: Self::NodeId, name: &str) -> Result<Self::NodeId, FsError>;

    fn read(&self, file: Self::NodeId) -> Result<Vec<u8>, FsError>;
    fn file_read_borrow(&self, file: Self::NodeId) -> Result<&[u8], FsError>;
    fn write_trunc(&mut self, file: Self::NodeId, bytes: &[u8]) -> Result<usize, FsError>;
    fn write_at(&mut self, file: Self::NodeId, off: usize, bytes: &[u8]) -> Result<usize, FsError>;
    fn len(&self, file: Self::NodeId) -> Result<usize, FsError>;
    fn append(&mut self, file: Self::NodeId, bytes: &[u8]) -> Result<(), FsError>;

    fn parent(&self, id: Self::NodeId) -> Result<Option<Self::NodeId>, FsError>;
    fn name(&self, id: Self::NodeId) -> Result<String, FsError>;
}

use num_enum::TryFromPrimitive;

pub mod errors;
pub mod wrappers;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum FileKind {
    Tty,
    File { node_id: usize },
    Dir { node_id: usize },
}

#[repr(u8)]
#[derive(Debug, TryFromPrimitive, Clone, Copy, PartialEq, Eq)]
pub enum FileKindTag {
    Unknown = 0,
    Tty = 1,
    File = 2,
    Dir = 3,
}

impl From<FileKind> for FileKindTag {
    fn from(value: FileKind) -> Self {
        match value {
            FileKind::Tty => FileKindTag::Tty,
            FileKind::File { .. } => FileKindTag::File,
            FileKind::Dir { .. } => FileKindTag::Dir,
        }
    }
}

#[repr(C)]
pub struct DirentHeader {
    pub inode: u64,
    pub ftype: u8,
    pub fname_len: u16,
}

impl DirentHeader {
    pub unsafe fn write_dirent(
        offset: usize,
        dst: *mut u8,
        inode: u64,
        ftype: u8,
        fname: &[u8],
        out_buf_len: usize,
    ) -> Option<usize> {
        let entry_size = core::mem::size_of::<DirentHeader>() + fname.len();

        if offset + entry_size > out_buf_len {
            return None;
        }
        unsafe {
            let hdr_ptr = dst.add(offset) as *mut DirentHeader;

            (*hdr_ptr).inode = inode;
            (*hdr_ptr).ftype = ftype;
            (*hdr_ptr).fname_len = fname.len() as u16;

            let name_dst = dst.add(offset + core::mem::size_of::<DirentHeader>());

            core::ptr::copy_nonoverlapping(fname.as_ptr(), name_dst, fname.len());
        }

        Some(entry_size)
    }
}

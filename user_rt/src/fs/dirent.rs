use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use sys::{
    FileOpenFlags,
    syscall::{
        DirentHeader, FileKindTag,
        errors::Errno,
        wrappers::{close, getdents, open},
    },
};

#[derive(Debug)]
pub struct DirEntry {
    pub fname: String,
    pub ftype: FileKindTag,
}

pub fn parse_dirents(buf: &[u8], bytes_written: usize) -> Result<Vec<DirEntry>, ()> {
    let mut entries = Vec::new();
    let mut offset = 0usize;

    while offset < bytes_written {
        if buf.len() - offset < core::mem::size_of::<DirentHeader>() {
            break;
        }

        let hdr = unsafe { &*(buf.as_ptr().add(offset) as *const DirentHeader) };

        let name_len = hdr.fname_len as usize;
        let entry_size = core::mem::size_of::<DirentHeader>() + name_len;

        if offset + entry_size > buf.len() {
            break;
        }

        let name_start = offset + core::mem::size_of::<DirentHeader>();
        let name_bytes = &buf[name_start..name_start + name_len];

        let fname = core::str::from_utf8(name_bytes)
            .map_err(|_| ())?
            .to_string();

        entries.push(DirEntry {
            fname,
            ftype: FileKindTag::try_from(hdr.ftype).unwrap_or(FileKindTag::Unknown),
        });

        offset += entry_size;
    }

    Ok(entries)
}

pub fn readdir(path: &str) -> Result<Vec<DirEntry>, Errno> {
    let fd = open(path, FileOpenFlags::READ)?;
    let mut output: Vec<DirEntry> = Vec::new();

    loop {
        let mut buf: [u8; 512] = [0; 512];
        let bytes_written = getdents(fd as u64, &mut buf)?;

        if bytes_written == 0 {
            break;
        }

        let dirents = parse_dirents(&buf, bytes_written as usize).unwrap();
        output.extend(dirents);
    }

    close(fd)?;
    Ok(output)
}

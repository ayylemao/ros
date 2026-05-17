use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use sys::{
    FileOpenFlags,
    syscall::{self, errors::Errno},
};

#[derive(Debug, Clone)]
pub struct File {
    pub fpath: String,
    pub fhandle: u64,
}

impl File {
    pub fn open(path: &str, flags: FileOpenFlags) -> Result<Self, Errno> {
        let fh = syscall::wrappers::open(path, flags)?;
        Ok(File {
            fhandle: fh as u64,
            fpath: path.to_string(),
        })
    }

    pub fn read(&self) -> Result<Vec<u8>, Errno> {
        let mut out = Vec::new();
        let mut buf = [0u8; 512];

        loop {
            let n = syscall::wrappers::read(self.fhandle, &mut buf)?;

            if n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n as usize]);
        }
        Ok(out)
    }

    pub fn write(&self, input: String) -> Result<i64, Errno> {
        let buf = input.as_bytes();
        syscall::wrappers::write(self.fhandle, buf)
    }

    pub fn close(&self) -> Result<(), Errno> {
        syscall::wrappers::close(self.fhandle as i64)?;
        Ok(())
    }
}

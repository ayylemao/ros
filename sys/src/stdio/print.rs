use core::fmt;

use crate::syscall::wrappers::write;

pub struct FdWriter {
    fd: u32,
}
impl FdWriter {
    pub const fn new(fd: u32) -> Self {
        Self { fd }
    }
}

impl fmt::Write for FdWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let rc = write(self.fd as u64, s.as_bytes()).unwrap();
        if rc < 0 { Err(fmt::Error) } else { Ok(()) }
    }
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    let mut w = FdWriter::new(1);
    let _ = w.write_fmt(args);
}

#[macro_export]
macro_rules! print {
  ($($arg:tt)*) => { $crate::stdio::print::_print(core::format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! println {
  () => { $crate::print!("\n") };
  ($($arg:tt)*) => { $crate::print!("{}\n", core::format_args!($($arg)*)) };
}

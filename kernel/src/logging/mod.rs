//use core::fmt;
//
//use alloc::vec::Vec;
//
//use crate::{fs::ramfs::Ramfs, kernel_cmds::uptime_ms};
//
//#[macro_export]
//macro_rules! kdebug {
//    ($($arg:tt)*) => {
//        $crate::logging::_log_print(
//            core::format_args!($($arg)*)
//        )
//    };
//}
//
//#[macro_export]
//macro_rules! debug {
//    () => {
//        $crate::kdebug!("\n")
//    };
//    ($($arg:tt)*) => {
//        $crate::kdebug!("{}\n", core::format_args!($($arg)*))
//    };
//}
//
//#[doc(hidden)]
//pub fn _log_print(args: fmt::Arguments) {
//    use core::fmt::Write;
//
//    struct ByteWriter {
//        buf: Vec<u8>,
//    }
//
//    impl Write for ByteWriter {
//        fn write_str(&mut self, s: &str) -> fmt::Result {
//            self.buf.extend_from_slice(s.as_bytes());
//            Ok(())
//        }
//    }
//
//    let ms: u64 = uptime_ms();
//
//    let mut writer = ByteWriter { buf: Vec::new() };
//
//    // Prefix: [00001234] (8 digits, zero-padded)
//    // If you want a space after, include it here.
//    write!(&mut writer, "[{:08}] ", ms).unwrap();
//
//    // Then the actual formatted message
//    fmt::write(&mut writer, args).unwrap();
//
//    let mut ramfs_guard = Ramfs::ramfs().lock();
//    let _ = ramfs_guard.append_file_from_path("/var/log/messages", &writer.buf);
//}

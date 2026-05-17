#![no_std]
#![no_main]

use user_rt::{self as _, env, fs::dirent::readdir};

use sys::{print, println, syscall::FileKindTag};

#[unsafe(no_mangle)]
pub fn main(argc: u64, argv: *const *const u8) -> i64 {
    let args = match env::parse_argv(argc, argv) {
        Ok(v) => v,
        Err(_) => {
            println!("ls: invalid arguments");
            return 1;
        }
    };
    let path = if args.len() == 1 { "." } else { args[1] };

    match readdir(path) {
        Ok(entries) => {
            for e in entries {
                if e.ftype == FileKindTag::Dir {
                    print!("{}/   ", e.fname);
                } else {
                    print!("{}    ", e.fname);
                }
            }
            print!("\n");
        }
        Err(_) => {
            println!("ls: cannot access '{}'", path);
            return 1;
        }
    }

    0
}

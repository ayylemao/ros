#![no_std]
#![no_main]

use user_rt::{self as _, env, fs::file::File};

use sys::{FileOpenFlags, print, println};

#[unsafe(no_mangle)]
pub fn main(argc: u64, argv: *const *const u8) -> i64 {
    let args = match env::parse_argv(argc, argv) {
        Ok(v) => v,
        Err(_) => {
            println!("cat: invalid arguments");
            return 1;
        }
    };

    if args.len() < 2 {
        println!("cat: missing file operand");
        return 1;
    }

    let path = args[1];

    let file = match File::open(path, FileOpenFlags::READ) {
        Ok(f) => f,
        Err(_) => {
            println!("cat: cannot open {}", path);
            return 1;
        }
    };

    let contents = match file.read() {
        Ok(v) => v,
        Err(_) => {
            println!("cat: read error {}", path);
            return 1;
        }
    };

    // stdout = fd 1

    match core::str::from_utf8(&contents) {
        Ok(s) => {
            print!("{}", s);
        }
        Err(_) => {
            println!("cat: file is not valid utf-8");
            return -1;
        }
    }

    0
}

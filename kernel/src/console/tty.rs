use alloc::vec::Vec;
use pc_keyboard::DecodedKey;
use spin::{Mutex, Once};

use crate::{
    arch::interrupt::idt::KEYBOARD_BUF,
    console::{self, console::CONSOLE, keyboard::KeyboardState},
    kprint, kprintln,
    utils::ringbuffer::SpscRing,
};

static TTY: Once<Mutex<Tty>> = Once::new();
static TTY_IN: Once<SpscRing<u8, 1024>> = Once::new();
static KEYBOARD_DECODER: Once<Mutex<KeyboardState>> = Once::new();

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub struct TtyFlags: u64 {
        const CANON = 1;
        const ECHO = 1 << 1;
    }
}

pub enum TtyAction {
    PutByte(u8),
    Backspace,
    Newline,
    None,
}

pub struct Tty {
    flags: TtyFlags,
    command_buf: Vec<char>,
}

impl Tty {
    pub fn tty() -> &'static Mutex<Tty> {
        TTY.call_once(|| Mutex::new(Tty::new()))
    }

    pub fn tty_in() -> &'static SpscRing<u8, 1024> {
        TTY_IN.call_once(|| SpscRing::new())
    }

    pub fn new() -> Self {
        Self {
            flags: TtyFlags::CANON | TtyFlags::ECHO,
            command_buf: Vec::new(),
        }
    }

    pub fn on_decode_key(&mut self, key: DecodedKey) -> TtyAction {
        match key {
            DecodedKey::Unicode('\u{8}') | DecodedKey::Unicode('\u{7f}') => TtyAction::Backspace,
            DecodedKey::Unicode('\n') => TtyAction::Newline,
            DecodedKey::Unicode(c) if c >= ' ' => TtyAction::PutByte(c as u8),
            _ => TtyAction::None,
        }
    }

    pub fn clear_line(&mut self) {
        self.command_buf.clear();
    }

    pub fn apply_action(&mut self, action: TtyAction) {
        let canon = self.flags.contains(TtyFlags::CANON);
        let echo = self.flags.contains(TtyFlags::ECHO);

        match action {
            TtyAction::PutByte(b) => {
                if canon {
                    self.command_buf.push(b as char);
                    if echo {
                        kprint!("{}", b as char);
                    }
                } else {
                    if echo {
                        kprint!("{}", b as char);
                    }
                    let _ = Tty::tty_in().push(b);
                }
            }

            TtyAction::Backspace => {
                if canon {
                    if self.command_buf.pop().is_some() && echo {
                        // erase one char visually
                        {
                            let mut guard = CONSOLE.lock();
                            if let Some(con) = guard.as_mut() {
                                con.backspace();
                            }
                        }
                        console::console::flush_console();
                    }
                } else {
                    // raw: deliver backspace as a byte
                    if echo {
                        // optional: visually erase in raw+echo too
                        {
                            let mut guard = CONSOLE.lock();
                            if let Some(con) = guard.as_mut() {
                                con.backspace();
                            }
                        }
                        console::console::flush_console();
                    }
                    let _ = Tty::tty_in().push(0x08);
                }
            }

            TtyAction::Newline => {
                if canon {
                    // echo newline
                    if echo {
                        kprintln!();
                    }
                    // flush cooked line into tty_in
                    for &c in self.command_buf.iter() {
                        let _ = Tty::tty_in().push(c as u8);
                    }
                    self.command_buf.clear();
                    let _ = Tty::tty_in().push(b'\n');
                } else {
                    if echo {
                        kprintln!();
                    }
                    let _ = Tty::tty_in().push(b'\n');
                }
            }

            TtyAction::None => {}
        }
    }

    pub fn keyboard() -> &'static Mutex<KeyboardState> {
        KEYBOARD_DECODER.call_once(|| Mutex::new(KeyboardState::new()))
    }

    pub fn pump_tty() {
        while let Some(sc) = KEYBOARD_BUF.pop() {
            if let Some(key) = Tty::keyboard().lock().feed(sc) {
                let mut tty = Tty::tty().lock();
                let action = tty.on_decode_key(key);
                tty.apply_action(action);
            }
        }
    }
}

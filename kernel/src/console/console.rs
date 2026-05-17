use core::fmt::Write;

use alloc::{fmt, vec::Vec};
use spin::Mutex;

use crate::{
    console::gop_framebuffer::{GopFramebuffer, FONT_H, FONT_W, FRAMEBUFFER},
    syscall::Winsize,
};

pub static CONSOLE: Mutex<Option<Console>> = Mutex::new(None);

#[derive(Copy, Clone, Debug)]
pub enum Color {
    Black = 0x00000000,
    White = 0x00FFFFFF,

    Red = 0x00FF0000,
    Green = 0x0000FF00,
    Blue = 0x000000FF,

    Yellow = 0x00FFFF00,
    Cyan = 0x0000FFFF,
    Magenta = 0x00FF00FF,

    Gray = 0x00808080,
}
impl From<Color> for u32 {
    #[inline]
    fn from(c: Color) -> u32 {
        c as u32
    }
}

impl Default for Color {
    fn default() -> Self {
        Color::Black
    }
}

pub const DEFAULT_COLOR_BG: Color = Color::Black;
const DEFAULT_COLOR_FG: Color = Color::Gray;

#[derive(Clone, Copy, Debug)]
pub struct Cell {
    pub c: char,
    pub fg: Color,
    pub bg: Color,
    pub dirty: bool,
}

impl Cell {
    fn new(c: char, dirty: bool) -> Self {
        Self {
            c,
            fg: DEFAULT_COLOR_FG,
            bg: DEFAULT_COLOR_BG,
            dirty,
        }
    }
    const fn empty() -> Self {
        Self {
            c: ' ',
            fg: DEFAULT_COLOR_FG,
            bg: DEFAULT_COLOR_BG,
            dirty: false,
        }
    }
}

impl Default for Cell {
    fn default() -> Self {
        Cell::empty()
    }
}

#[derive(Debug)]
pub struct Console {
    cells: Vec<Cell>,
    rows: usize,
    cols: usize,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub dirty: bool,
    xpixel: usize,
    ypixel: usize,
}

impl Console {
    pub fn init_console(xpixel: usize, ypixel: usize) {
        let console = Console::new(xpixel, ypixel);
        *CONSOLE.lock() = Some(console);
    }

    fn new(xpixel: usize, ypixel: usize) -> Self {
        let rows = ypixel / FONT_H;
        let cols = xpixel / FONT_W;
        let size = rows * cols;

        Console {
            cells: vec![Cell::default(); size],
            rows,
            cols,
            cursor_x: 0,
            cursor_y: 0,
            dirty: false,
            xpixel: xpixel,
            ypixel: ypixel,
        }
    }

    pub fn get_winsize(&self) -> Winsize {
        Winsize {
            rows: self.rows as i16,
            cols: self.cols as i16,
            xpixel: self.xpixel as i16,
            ypixel: self.ypixel as i16,
        }
    }

    fn newline(&mut self) {
        self.cursor_x = 0;
        self.cursor_y += 1;

        if self.cursor_y >= self.rows {
            self.scroll_up(1);
            self.cursor_y = self.rows - 1;
        }
    }

    fn mark_all_dirty(&mut self) {
        for cell in &mut self.cells {
            cell.dirty = true;
        }
        self.dirty = true;
    }

    fn scroll_up(&mut self, lines: usize) {
        let cols = self.cols;
        let rows = self.rows;
        let shift = lines.min(rows);

        for y in 0..(rows - shift) {
            let dst = y * cols;
            let src = (y + shift) * cols;
            self.cells.copy_within(src..src + cols, dst);
        }

        let clear_start = (rows - shift) * cols;
        for cell in &mut self.cells[clear_start..] {
            *cell = Cell::empty();
            cell.dirty = true;
        }

        self.mark_all_dirty();
    }

    pub fn putchar(&mut self, ch: char) {
        match ch {
            '\n' => self.newline(),
            _ => {
                if self.cursor_x >= self.cols {
                    self.newline();
                }

                let idx = self.cursor_y * self.cols + self.cursor_x;
                self.cells[idx] = Cell::new(ch, true);

                self.cursor_x += 1;
                if self.cursor_x >= self.cols {
                    self.newline();
                }
            }
        }
        self.dirty = true;
    }

    pub fn backspace(&mut self) {
        if self.cursor_x >= self.cols && self.cols != 0 {
            self.cursor_x = self.cols - 1;
        }

        if self.cursor_x == 0 && self.cursor_y == 0 {
            return;
        }

        if self.cursor_x > 0 {
            self.cursor_x -= 1;
        } else {
            self.cursor_y -= 1;
            self.cursor_x = self.cols - 1;
        }

        let idx = self.cursor_y * self.cols + self.cursor_x;
        self.cells[idx] = Cell::new(' ', true);
        self.dirty = true;
    }

    pub fn flush(&mut self, fb: &mut GopFramebuffer) {
        for y in 0..self.rows {
            for x in 0..self.cols {
                let idx = y * self.cols + x;
                let cell = &mut self.cells[idx];
                if cell.dirty {
                    fb.draw_cell(x, y, cell);
                    cell.dirty = false;
                }
            }
        }
    }
}

impl Write for Console {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for ch in s.chars() {
            self.putchar(ch);
        }
        self.dirty = true;
        Ok(())
    }
}

pub fn flush_console() {
    let mut console_guard = CONSOLE.lock();
    let console = match console_guard.as_mut() {
        Some(c) => c,
        None => return,
    };
    if !console.dirty {
        return;
    }

    let mut fb = FRAMEBUFFER.lock();
    console.flush(&mut fb);
}

pub fn try_flush_console() {
    if let Some(mut console_guard) = CONSOLE.try_lock() {
        let console = match console_guard.as_mut() {
            Some(c) => c,
            None => return,
        };
        if !console.dirty {
            return;
        }

        if let Some(mut fb) = FRAMEBUFFER.try_lock() {
            console.flush(&mut fb);
        }
    }
}

#[macro_export]
macro_rules! kprint {
    ($($arg:tt)*) => ($crate::console::console::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! kprintln {
    () => ($crate::kprint!("\n"));
    ($($arg:tt)*) => ($crate::kprint!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;

    let mut console_guard = CONSOLE.lock();
    let console = match console_guard.as_mut() {
        Some(c) => c,
        None => return,
    };
    console.write_fmt(args).unwrap();
    drop(console_guard);
    flush_console();
}

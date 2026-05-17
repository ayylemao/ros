#![allow(dead_code)]
use shared::{BootInfo, FramebufferInfo};

use lazy_static::lazy_static;
use spin::Mutex;

use crate::console::console::{Cell, Color, DEFAULT_COLOR_BG};

lazy_static! {
    pub static ref FRAMEBUFFER: Mutex<GopFramebuffer> = Mutex::new(GopFramebuffer::default());
}

const FONT_BUFFER: &[u8] = include_bytes!("../../ARMSCII8.F16");

pub const FONT_W: usize = 8;
pub const FONT_H: usize = 16;

#[allow(dead_code)]
#[derive(Debug, Default)]
pub struct GopFramebuffer {
    buffer: u64,
    width: usize,
    height: usize,
    stride: usize,
}

pub struct FbInfo {
    pub width: usize,
    pub height: usize,
}

impl GopFramebuffer {
    pub fn init_fb(bi: &BootInfo) -> FbInfo {
        let width: usize;
        let height: usize;
        if let Some(ref fb) = bi.framebuffer {
            let mut gop_fb = FRAMEBUFFER.lock();
            gop_fb.from_fb(fb);
            gop_fb.clear();
            width = gop_fb.width;
            height = gop_fb.height;
        } else {
            panic!("CONSOLE not able to be loaded!")
        };

        FbInfo { width, height }
    }

    pub fn init_font(&mut self) {}

    pub fn from_fb(&mut self, fb: &FramebufferInfo) {
        self.buffer = fb.addr;
        self.width = fb.width;
        self.height = fb.height;
        self.stride = fb.stride;
    }

    fn putpixel(&mut self, x: usize, y: usize, color: impl Into<u32>) {
        if x >= self.width || y >= self.height {
            panic!("Pixel coords outside of screen!")
        }
        let buffer = self.buffer as *mut u32;
        let index = y * self.stride + x;
        unsafe {
            buffer.add(index).write_volatile(color.into());
        }
    }

    fn draw_glyph(&mut self, c: char, x: usize, y: usize, fc: Color, bc: Color) {
        let c: u8 = c as u8;
        let mask: [u8; 8] = [128, 64, 32, 16, 8, 4, 2, 1];
        let glyph_offset = (c as usize) * 16;
        for cy in 0..16usize {
            for cx in 0..8usize {
                if (FONT_BUFFER[glyph_offset + cy] & mask[cx]) != 0 {
                    self.putpixel(x + cx, y + cy, fc);
                } else {
                    self.putpixel(x + cx, y + cy, bc);
                }
            }
        }
    }

    pub fn draw_cell(&mut self, x: usize, y: usize, cell: &Cell) {
        let px = x * 8;
        let py = y * 16;

        self.draw_glyph(cell.c, px, py, cell.fg, cell.bg);
    }

    pub fn clear(&mut self) {
        for y in 0..self.height {
            for x in 0..self.width {
                self.putpixel(x, y, DEFAULT_COLOR_BG);
            }
        }
    }
}

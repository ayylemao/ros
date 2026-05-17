#![allow(dead_code)]
pub const SERIAL_PORT: u16 = 0x3F8; // COM1 base

pub unsafe fn serial_init() {
    // Disable interrupts
    outb(SERIAL_PORT + 1, 0x00);

    // Enable DLAB (set baud rate divisor)
    outb(SERIAL_PORT + 3, 0x80);
    outb(SERIAL_PORT + 0, 0x03); // divisor low byte (38400 baud)
    outb(SERIAL_PORT + 1, 0x00); // divisor high byte

    // 8 bits, no parity, one stop bit
    outb(SERIAL_PORT + 3, 0x03);

    // Enable FIFO, clear them, 14-byte threshold
    outb(SERIAL_PORT + 2, 0xC7);

    // IRQs enabled, RTS/DSR set
    outb(SERIAL_PORT + 4, 0x0B);
}

#[inline]
pub unsafe fn outb(port: u16, value: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") value);
}

#[inline]
pub unsafe fn outl(port: u16, value: u32) {
    core::arch::asm!(
        "out dx, eax",
        in("dx") port,
        in("eax") value,
        options(nomem, nostack, preserves_flags),
    );
}

#[inline]
pub unsafe fn inl(port: u16) -> u32 {
    let value: u32;
    core::arch::asm!(
        "in eax, dx",
        in("dx") port,
        out("eax") value,
        options(nomem, nostack, preserves_flags),
    );
    value
}

unsafe fn serial_ready() -> bool {
    // Line Status Register bit 5 = Transmitter Holding Empty
    (inb(SERIAL_PORT + 5) & 0x20) != 0
}

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let mut value: u8;
    core::arch::asm!("in al, dx", out("al") value, in("dx") port);
    value
}

pub unsafe fn serial_write_byte(byte: u8) {
    while !serial_ready() {}
    outb(SERIAL_PORT, byte);
}

unsafe fn serial_write_unsafe(s: &str) {
    for b in s.bytes() {
        serial_write_byte(b);
    }
}

pub fn serial_write(s: &str) {
    unsafe {
        serial_write_unsafe(s);
    }
}

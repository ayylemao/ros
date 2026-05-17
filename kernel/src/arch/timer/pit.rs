use x86_64::instructions::port::Port;

const PIT_HZ: u32 = 1_193_182;

pub fn wait_ms(ms: u32) {
    let mut cmd = Port::<u8>::new(0x43);
    let mut ch2 = Port::<u8>::new(0x42);
    let mut ps2 = Port::<u8>::new(0x61);

    let mut count = (PIT_HZ as u64 * ms as u64) / 1000;
    if count == 0 {
        count = 1;
    }
    if count > 0xFFFF {
        count = 0xFFFF;
    }
    let count = count as u16;

    unsafe {
        // Enable gate for channel 2 (bit 0 = 1), disable speaker (bit 1 = 0)
        let mut v = ps2.read();
        v = (v | 0x01) & !0x02;
        ps2.write(v);

        // Program channel 2
        cmd.write(0xB0);
        ch2.write((count & 0xFF) as u8);
        ch2.write((count >> 8) as u8);

        // Wait until OUT2 goes high (bit 5 == 1)
        loop {
            let s = ps2.read();
            if (s & (1 << 5)) != 0 {
                break;
            }
            core::hint::spin_loop();
        }
    }
}

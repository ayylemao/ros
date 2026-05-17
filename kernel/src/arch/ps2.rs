use x86_64::instructions::port::Port;

fn wait_input_empty() {
    let mut status = Port::<u8>::new(0x64);
    for _ in 0..100_000 {
        let s = unsafe { status.read() };
        if (s & 0x02) == 0 {
            return;
        }
    }
}

fn wait_output_full() {
    let mut status = Port::<u8>::new(0x64);
    for _ in 0..100_000 {
        let s = unsafe { status.read() };
        if (s & 0x01) != 0 {
            return;
        }
    }
}

// Enables IRQ1 from the PS/2 controller and enables scanning.
pub fn enable_keyboard_irq() {
    let mut cmd = Port::<u8>::new(0x64);
    let mut data = Port::<u8>::new(0x60);

    flush_output_buffer();

    // Enable keyboard interface
    wait_input_empty();
    unsafe { cmd.write(0xAE) };

    // Read command byte
    wait_input_empty();
    unsafe { cmd.write(0x20) };
    wait_output_full();
    let mut cb = unsafe { data.read() };

    // Enable IRQ1
    cb |= 1 << 0;

    // Write command byte
    wait_input_empty();
    unsafe { cmd.write(0x60) };
    wait_input_empty();
    unsafe { data.write(cb) };

    // Enable scanning (harmless if already enabled)
    wait_input_empty();
    unsafe { data.write(0xF4) };

    flush_output_buffer();
}

fn flush_output_buffer() {
    let mut status = Port::<u8>::new(0x64);
    let mut data = Port::<u8>::new(0x60);

    // Drain while output buffer full (bit 0)
    for _ in 0..100_000 {
        let s = unsafe { status.read() };
        if (s & 0x01) == 0 {
            break;
        }
        let _ = unsafe { data.read() };
    }
}

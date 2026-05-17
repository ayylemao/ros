use pc_keyboard::{
    layouts::{self, Us104Key},
    DecodedKey, HandleControl, Keyboard, ScancodeSet1,
};

pub struct KeyboardState {
    kb: Keyboard<layouts::Us104Key, ScancodeSet1>,
}

impl KeyboardState {
    pub fn new() -> Self {
        Self {
            kb: Keyboard::new(ScancodeSet1::new(), Us104Key, HandleControl::Ignore),
        }
    }

    pub fn feed(&mut self, sc: u8) -> Option<DecodedKey> {
        if let Ok(Some(ev)) = self.kb.add_byte(sc) {
            return self.kb.process_keyevent(ev);
        }
        None
    }
}

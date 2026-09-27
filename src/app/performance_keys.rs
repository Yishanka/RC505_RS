//! egui 0.27 drops some shifted symbols before producing key events (e.g. !
//! and >), and maps Shift+/ to Questionmark. Performance controls need the
//! underlying key, not the text. Poll only these keys, only while our foreground
//! performance surface is active; leave egui's original text events untouched.
use eframe::egui::{Event, InputState, Key};
const BINDINGS: [(Key, i32); 34] = [
    (Key::Num1, 0x31),
    (Key::Num2, 0x32),
    (Key::Num3, 0x33),
    (Key::Num4, 0x34),
    (Key::Num5, 0x35),
    (Key::F1, 0x70),
    (Key::F2, 0x71),
    (Key::F3, 0x72),
    (Key::F4, 0x73),
    (Key::F5, 0x74),
    (Key::Space, 0x20),
    (Key::T, 0x54),
    (Key::Q, 0x51),
    (Key::W, 0x57),
    (Key::E, 0x45),
    (Key::R, 0x52),
    (Key::U, 0x55),
    (Key::I, 0x49),
    (Key::O, 0x4f),
    (Key::P, 0x50),
    (Key::Z, 0x5a),
    (Key::X, 0x58),
    (Key::C, 0x43),
    (Key::V, 0x56),
    (Key::B, 0x42),
    (Key::N, 0x4e),
    (Key::M, 0x4d),
    (Key::Comma, 0xbc),
    (Key::Period, 0xbe),
    (Key::Slash, 0xbf),
    (Key::Y, 0x59),
    (Key::ArrowLeft, 0x25),
    (Key::ArrowRight, 0x27),
    (Key::Delete, 0x2e),
];
pub struct PerformanceKeys {
    previous: [bool; 34],
    suppressed: [bool; 34],
    armed: bool,
}
impl Default for PerformanceKeys {
    fn default() -> Self {
        Self {
            previous: [false; 34],
            suppressed: [false; 34],
            armed: false,
        }
    }
}
impl PerformanceKeys {
    pub fn suspend(&mut self) {
        *self = Self::default();
    }
    pub fn poll(&mut self, input: &mut InputState) {
        #[cfg(windows)]
        let down = {
            #[link(name = "user32")]
            unsafe extern "system" {
                fn GetAsyncKeyState(vkey: i32) -> i16;
                fn GetForegroundWindow() -> *mut std::ffi::c_void;
                fn GetWindowThreadProcessId(
                    window: *mut std::ffi::c_void,
                    process: *mut u32,
                ) -> u32;
            }
            let mut owner = 0;
            unsafe {
                GetWindowThreadProcessId(GetForegroundWindow(), &mut owner);
            }
            if owner != std::process::id() {
                self.suspend();
                self.apply(input, [false; 34]);
                self.suspend();
                return;
            }
            // The caller has already checked foreground focus and text/editor isolation.
            std::array::from_fn(|i| unsafe { GetAsyncKeyState(BINDINGS[i].1) } < 0)
        };
        #[cfg(not(windows))]
        let down = std::array::from_fn(|i| input.key_down(BINDINGS[i].0));
        self.apply(input, down);
    }
    fn apply(&mut self, input: &mut InputState, down: [bool; 34]) {
        let was_armed = self.armed;
        if !self.armed {
            self.previous = down;
            self.suppressed = down;
            self.armed = true;
        }
        let mut quick_press = [false; 34];
        input.events.retain(|event| {
            if let Event::Key {
                key,
                physical_key,
                pressed,
                repeat,
                ..
            } = event
            {
                let key = physical_key.unwrap_or(*key);
                if let Some(index) = BINDINGS.iter().position(|b| b.0 == key) {
                    quick_press[index] |=
                        was_armed && *pressed && !*repeat && !self.previous[index];
                    return false;
                }
            }
            true
        });
        for (index, (key, _)) in BINDINGS.iter().enumerate() {
            if !down[index] {
                self.suppressed[index] = false;
            }
            input.keys_down.remove(key);
            if !self.suppressed[index] {
                if down[index] {
                    input.keys_down.insert(*key);
                }
                if (down[index] && !self.previous[index]) || quick_press[index] {
                    input.events.push(Event::Key {
                        key: *key,
                        physical_key: Some(*key),
                        pressed: true,
                        repeat: false,
                        modifiers: input.modifiers,
                    });
                }
            }
        }
        self.previous = down;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shifted_digits_and_punctuation_are_independent_of_text_and_repeat() {
        let mut state = PerformanceKeys::default();
        let mut input = InputState::default();
        state.apply(&mut input, [false; 34]);
        input.modifiers.shift = true;
        input.events = vec![Event::Text("!>?".into())];
        let mut down = [false; 34];
        for i in [0, 28, 29] {
            down[i] = true;
        }
        state.apply(&mut input, down);
        for key in [Key::Num1, Key::Period, Key::Slash] {
            assert!(input.key_down(key) && input.key_pressed(key));
        }
        input.events = vec![Event::Key {
            key: Key::Questionmark,
            physical_key: Some(Key::Slash),
            pressed: true,
            repeat: false,
            modifiers: input.modifiers,
        }];
        state.apply(&mut input, down);
        assert!(!input.key_pressed(Key::Slash));
        input.events.clear();
        state.apply(&mut input, [false; 34]);
        assert!(!input.key_down(Key::Slash));
    }
    #[test]
    fn focus_resume_suppresses_already_held_keys_until_release() {
        let mut state = PerformanceKeys::default();
        let mut input = InputState::default();
        let mut down = [false; 34];
        down[0] = true;
        state.apply(&mut input, down);
        assert!(!input.key_down(Key::Num1) && !input.key_pressed(Key::Num1));
        state.apply(&mut input, [false; 34]);
        state.apply(&mut input, down);
        assert!(input.key_pressed(Key::Num1));
        state.suspend();
        input.events.clear();
        state.apply(&mut input, down);
        assert!(!input.key_down(Key::Num1));
    }
}

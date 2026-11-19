#![allow(dead_code)]
use crate::common::protocol::{InputEvent, KeyCode, MouseBtn};
use enigo::{Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};

pub struct HostInputExecutor {
    enigo: Enigo,
    screen_width: u32,
    screen_height: u32,
}

impl HostInputExecutor {
    pub fn new(screen_width: u32, screen_height: u32) -> Result<Self, anyhow::Error> {
        let enigo = Enigo::new(&Settings::default())
            .map_err(|e| anyhow::anyhow!("Failed to initialize Enigo: {:?}", e))?;

        Ok(Self {
            enigo,
            screen_width,
            screen_height,
        })
    }

    /// Sends a wheel movement of `delta` units (120 = one notch; +up / +right).
    ///
    /// On Windows the raw delta is sent, so small finger movements on a touch screen turn
    /// into smooth sub-notch scrolling. Other platforms only accept whole notches.
    fn wheel(&mut self, delta: i32, horizontal: bool) {
        #[cfg(windows)]
        {
            use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
                SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_WHEEL, MOUSEINPUT,
            };
            let _ = &self.enigo;
            let input = INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: 0,
                        dy: 0,
                        mouseData: delta as u32,
                        dwFlags: if horizontal { MOUSEEVENTF_HWHEEL } else { MOUSEEVENTF_WHEEL },
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            };
            // SAFETY: `input` is a valid INPUT and the size matches the struct passed.
            unsafe {
                SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
            }
        }
        #[cfg(not(windows))]
        {
            let notches = (delta as f32 / 120.0).round() as i32;
            let notches = if notches == 0 { delta.signum() } else { notches };
            if horizontal {
                let _ = self.enigo.scroll(notches, enigo::Axis::Horizontal);
            } else {
                // enigo: positive scrolls down
                let _ = self.enigo.scroll(-notches, enigo::Axis::Vertical);
            }
        }
    }

    pub fn update_resolution(&mut self, width: u32, height: u32) {
        self.screen_width = width;
        self.screen_height = height;
    }

    pub fn execute(&mut self, event: InputEvent) {
        match event {
            InputEvent::MouseMove { x_ratio, y_ratio } => {
                let x = (x_ratio.clamp(0.0, 1.0) * self.screen_width as f32).round() as i32;
                let y = (y_ratio.clamp(0.0, 1.0) * self.screen_height as f32).round() as i32;
                let _ = self.enigo.move_mouse(x, y, Coordinate::Abs);
            }
            InputEvent::MouseDown { button } => {
                let btn = match button {
                    MouseBtn::Left => Button::Left,
                    MouseBtn::Right => Button::Right,
                    MouseBtn::Middle => Button::Middle,
                };
                let _ = self.enigo.button(btn, Direction::Press);
            }
            InputEvent::MouseUp { button } => {
                let btn = match button {
                    MouseBtn::Left => Button::Left,
                    MouseBtn::Right => Button::Right,
                    MouseBtn::Middle => Button::Middle,
                };
                let _ = self.enigo.button(btn, Direction::Release);
            }
            InputEvent::MouseScroll { delta_x, delta_y } => {
                // Wire units are mouse-wheel units: 120 per notch, +y = up, +x = right.
                if delta_y != 0 {
                    self.wheel(delta_y, false);
                }
                if delta_x != 0 {
                    self.wheel(delta_x, true);
                }
            }
            InputEvent::KeyDown { key } => {
                if let Some(enigo_key) = map_key(&key) {
                    let _ = self.enigo.key(enigo_key, Direction::Press);
                }
            }
            InputEvent::KeyUp { key } => {
                if let Some(enigo_key) = map_key(&key) {
                    let _ = self.enigo.key(enigo_key, Direction::Release);
                }
            }
        }
    }
}

fn map_key(key: &KeyCode) -> Option<Key> {
    match key {
        KeyCode::Unicode(c) => Some(Key::Unicode(*c)),
        KeyCode::Backspace => Some(Key::Backspace),
        KeyCode::Tab => Some(Key::Tab),
        KeyCode::Enter => Some(Key::Return),
        KeyCode::Escape => Some(Key::Escape),
        KeyCode::Space => Some(Key::Space),
        KeyCode::Shift => Some(Key::Shift),
        KeyCode::Control => Some(Key::Control),
        KeyCode::Alt => Some(Key::Alt),
        KeyCode::Meta => Some(Key::Meta),
        KeyCode::CapsLock => Some(Key::CapsLock),
        KeyCode::LeftArrow => Some(Key::LeftArrow),
        KeyCode::RightArrow => Some(Key::RightArrow),
        KeyCode::UpArrow => Some(Key::UpArrow),
        KeyCode::DownArrow => Some(Key::DownArrow),
        KeyCode::Delete => Some(Key::Delete),
        KeyCode::Home => Some(Key::Home),
        KeyCode::End => Some(Key::End),
        KeyCode::PageUp => Some(Key::PageUp),
        KeyCode::PageDown => Some(Key::PageDown),
        KeyCode::F(num) => match num {
            1 => Some(Key::F1),
            2 => Some(Key::F2),
            3 => Some(Key::F3),
            4 => Some(Key::F4),
            5 => Some(Key::F5),
            6 => Some(Key::F6),
            7 => Some(Key::F7),
            8 => Some(Key::F8),
            9 => Some(Key::F9),
            10 => Some(Key::F10),
            11 => Some(Key::F11),
            12 => Some(Key::F12),
            _ => None,
        },
        KeyCode::Other(_) => None,
    }
}

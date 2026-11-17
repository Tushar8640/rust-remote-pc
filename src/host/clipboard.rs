#![allow(dead_code)]
use arboard::Clipboard;

pub struct HostClipboard {
    clipboard: Clipboard,
    last_text: String,
}

impl HostClipboard {
    pub fn new() -> Result<Self, anyhow::Error> {
        let mut clipboard = Clipboard::new().map_err(|e| anyhow::anyhow!("Clipboard error: {:?}", e))?;
        let last_text = clipboard.get_text().unwrap_or_default();
        Ok(Self {
            clipboard,
            last_text,
        })
    }

    pub fn set_text(&mut self, text: &str) {
        if self.last_text != text {
            self.last_text = text.to_string();
            let _ = self.clipboard.set_text(text);
        }
    }

    /// Check if local clipboard changed, return new text if changed
    pub fn check_change(&mut self) -> Option<String> {
        if let Ok(current) = self.clipboard.get_text() {
            if current != self.last_text {
                self.last_text = current.clone();
                return Some(current);
            }
        }
        None
    }
}

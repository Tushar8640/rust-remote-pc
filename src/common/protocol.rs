#![allow(dead_code)]
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: &str = "0.1.0";
pub const DEFAULT_PORT: u16 = 5885;

/// Supported video / frame compression formats
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FrameFormat {
    /// JPEG compressed frame (lightweight fallback, low CPU)
    Jpeg { quality: u8 },
    /// H.264 video stream packet (I-frame or P-frame)
    H264 { is_keyframe: bool },
    /// Raw RGBA pixels
    RawRgba,
}

/// Mouse button representations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseBtn {
    Left,
    Right,
    Middle,
}

/// Key representations for keyboard control
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyCode {
    Unicode(char),
    Backspace,
    Tab,
    Enter,
    Escape,
    Space,
    Shift,
    Control,
    Alt,
    Meta, // Windows key
    CapsLock,
    LeftArrow,
    RightArrow,
    UpArrow,
    DownArrow,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8), // F1 - F12
    Other(u32),
}

/// Input event sent from Client Viewer to Host Agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InputEvent {
    /// Mouse movement represented by normalized ratios (0.0 to 1.0)
    MouseMove { x_ratio: f32, y_ratio: f32 },
    MouseDown { button: MouseBtn },
    MouseUp { button: MouseBtn },
    MouseScroll { delta_x: i32, delta_y: i32 },
    KeyDown { key: KeyCode },
    KeyUp { key: KeyCode },
}

/// Top-level network message between Host and Client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Packet {
    /// Client initiates handshake
    AuthRequest {
        client_version: String,
        token_hash: String,
    },
    /// Host responds to handshake
    AuthResponse {
        success: bool,
        message: String,
        screen_width: u32,
        screen_height: u32,
    },
    /// Host streams video frame to Client
    VideoFrame {
        width: u32,
        height: u32,
        format: FrameFormat,
        data: Vec<u8>,
        timestamp_ms: u64,
    },
    /// Client sends user input event to Host
    Input(InputEvent),
    /// Bidirectional clipboard text synchronization
    Clipboard(String),
    /// Heartbeat / latency measurement
    Ping(u64),
    Pong(u64),
    /// Host system audio (loopback) streamed to the Client.
    /// `data` is interleaved signed 16-bit little-endian PCM, 1 or 2 channels.
    /// IMPORTANT: new variants must only ever be appended; bincode encodes the
    /// variant index, and the Android client depends on these numbers (Audio = 7).
    Audio {
        sample_rate: u32,
        channels: u16,
        data: Vec<u8>,
    },
}

impl Packet {
    /// Serialize packet into bytes with length-prefix (4 bytes BE)
    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::Error> {
        let payload = bincode::serialize(self)?;
        let len = payload.len() as u32;
        let mut buffer = Vec::with_capacity(4 + payload.len());
        buffer.extend_from_slice(&len.to_be_bytes());
        buffer.extend_from_slice(&payload);
        Ok(buffer)
    }

    /// Deserialize packet from payload bytes (excluding the 4-byte length prefix)
    pub fn from_payload(bytes: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Android client hand-codes this layout (ProtocolCodec.kt); keep them in sync.
    #[test]
    fn audio_packet_wire_layout() {
        let p = Packet::Audio { sample_rate: 48000, channels: 2, data: vec![1, 2, 3, 4] };
        let b = p.to_bytes().unwrap();
        let payload = &b[4..];
        assert_eq!(u32::from_be_bytes(b[0..4].try_into().unwrap()) as usize, payload.len());
        assert_eq!(&payload[0..4], &7u32.to_le_bytes()); // variant index
        assert_eq!(&payload[4..8], &48000u32.to_le_bytes());
        assert_eq!(&payload[8..10], &2u16.to_le_bytes());
        assert_eq!(&payload[10..18], &4u64.to_le_bytes()); // length prefix
        assert_eq!(&payload[18..], &[1, 2, 3, 4]);
    }

    /// bincode writes `char` as UTF-8, not as a 4-byte scalar.
    #[test]
    fn unicode_key_is_utf8() {
        let ev = Packet::Input(InputEvent::KeyDown { key: KeyCode::Unicode('c') });
        let b = ev.to_bytes().unwrap();
        let payload = &b[4..];
        // Packet::Input = 3, InputEvent::KeyDown = 4, KeyCode::Unicode = 0, then 'c'
        assert_eq!(&payload[0..4], &3u32.to_le_bytes());
        assert_eq!(&payload[4..8], &4u32.to_le_bytes());
        assert_eq!(&payload[8..12], &0u32.to_le_bytes());
        assert_eq!(&payload[12..], b"c");
        let e = Packet::Input(InputEvent::KeyDown { key: KeyCode::Unicode('é') }).to_bytes().unwrap();
        assert_eq!(&e[4 + 12..], "é".as_bytes());
    }
}

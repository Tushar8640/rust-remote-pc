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

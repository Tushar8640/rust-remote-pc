use crate::common::protocol::FrameFormat;
use anyhow::Result;
use image::codecs::jpeg::JpegEncoder;
use image::ColorType;
use std::io::Cursor;

pub struct VideoEncoder {
    format: FrameFormat,
}

impl VideoEncoder {
    pub fn new(format: FrameFormat) -> Self {
        Self { format }
    }

    pub fn encode(&mut self, width: u32, height: u32, rgba_data: &[u8]) -> Result<(FrameFormat, Vec<u8>)> {
        match self.format {
            FrameFormat::Jpeg { quality } => {
                let mut buffer = Vec::new();
                let mut cursor = Cursor::new(&mut buffer);
                let mut encoder = JpegEncoder::new_with_quality(&mut cursor, quality);

                // Convert RGBA to RGB for JPEG encoding (JPEG does not support alpha)
                let rgb_len = (width * height * 3) as usize;
                let mut rgb = Vec::with_capacity(rgb_len);
                for chunk in rgba_data.chunks_exact(4) {
                    rgb.push(chunk[0]); // R
                    rgb.push(chunk[1]); // G
                    rgb.push(chunk[2]); // B
                }

                encoder.encode(&rgb, width, height, ColorType::Rgb8.into())?;
                Ok((FrameFormat::Jpeg { quality }, buffer))
            }
            #[cfg(feature = "h264")]
            FrameFormat::H264 { is_keyframe } => {
                // Future openh264 stream implementation
                let mut buffer = Vec::new();
                Ok((FrameFormat::H264 { is_keyframe }, buffer))
            }
            #[cfg(not(feature = "h264"))]
            FrameFormat::H264 { .. } => {
                // Fallback to JPEG if h264 feature is not compiled
                let quality = 75;
                let mut buffer = Vec::new();
                let mut cursor = Cursor::new(&mut buffer);
                let mut encoder = JpegEncoder::new_with_quality(&mut cursor, quality);

                let mut rgb = Vec::with_capacity((width * height * 3) as usize);
                for chunk in rgba_data.chunks_exact(4) {
                    rgb.push(chunk[0]);
                    rgb.push(chunk[1]);
                    rgb.push(chunk[2]);
                }
                encoder.encode(&rgb, width, height, ColorType::Rgb8.into())?;
                Ok((FrameFormat::Jpeg { quality }, buffer))
            }
            FrameFormat::RawRgba => {
                Ok((FrameFormat::RawRgba, rgba_data.to_vec()))
            }
        }
    }
}

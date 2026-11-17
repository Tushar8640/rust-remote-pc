use crate::common::protocol::FrameFormat;
use anyhow::{anyhow, Result};

pub struct VideoDecoder;

impl VideoDecoder {
    pub fn new() -> Self {
        Self
    }

    /// Decodes compressed packet data into raw RGBA8 pixels and (width, height)
    pub fn decode(&mut self, format: FrameFormat, data: &[u8], width: u32, height: u32) -> Result<(u32, u32, Vec<u8>)> {
        match format {
            FrameFormat::Jpeg { .. } => {
                let img = image::load_from_memory_with_format(data, image::ImageFormat::Jpeg)
                    .map_err(|e| anyhow!("Failed to decode JPEG frame: {:?}", e))?;
                let rgba = img.to_rgba8();
                Ok((rgba.width(), rgba.height(), rgba.into_raw()))
            }
            #[cfg(feature = "h264")]
            FrameFormat::H264 { .. } => {
                // Future openh264 decoder integration
                Err(anyhow!("H264 decoding is in progress"))
            }
            #[cfg(not(feature = "h264"))]
            FrameFormat::H264 { .. } => {
                let img = image::load_from_memory_with_format(data, image::ImageFormat::Jpeg)
                    .map_err(|e| anyhow!("Failed to decode fallback frame: {:?}", e))?;
                let rgba = img.to_rgba8();
                Ok((rgba.width(), rgba.height(), rgba.into_raw()))
            }
            FrameFormat::RawRgba => {
                Ok((width, height, data.to_vec()))
            }
        }
    }
}

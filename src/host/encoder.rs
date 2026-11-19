use crate::common::protocol::FrameFormat;
use anyhow::{anyhow, Result};
use jpeg_encoder::{ColorType, Encoder, SamplingFactor};

/// Encodes captured RGBA frames for the wire.
pub struct VideoEncoder {
    format: FrameFormat,
    /// Reused buffer for half-resolution frames.
    half: Vec<u8>,
}

/// One encoded frame plus its (possibly reduced) dimensions.
pub struct EncodedFrame {
    pub format: FrameFormat,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl VideoEncoder {
    pub fn new(format: FrameFormat) -> Self {
        Self { format, half: Vec::new() }
    }

    /// Encodes `rgba_data` (`width` x `height`). With `half_scale` the frame is
    /// box-filtered to half size first (about 4x fewer pixels), which is how the
    /// sender backs off on slow links. `quality` overrides the JPEG quality.
    pub fn encode(
        &mut self,
        width: u32,
        height: u32,
        rgba_data: &[u8],
        quality: u8,
        half_scale: bool,
    ) -> Result<EncodedFrame> {
        match self.format {
            FrameFormat::RawRgba => Ok(EncodedFrame {
                format: FrameFormat::RawRgba,
                width,
                height,
                data: rgba_data.to_vec(),
            }),
            // H.264 is not implemented; Jpeg and the H264 fallback both use JPEG.
            _ => {
                let (w, h, pixels): (usize, usize, &[u8]) = if half_scale && width >= 2 && height >= 2 {
                    let (nw, nh) = downscale_half(rgba_data, width as usize, height as usize, &mut self.half);
                    (nw, nh, &self.half)
                } else {
                    (width as usize, height as usize, rgba_data)
                };
                if w > u16::MAX as usize || h > u16::MAX as usize {
                    return Err(anyhow!("frame too large for JPEG: {}x{}", w, h));
                }

                let mut buffer = Vec::with_capacity(w * h / 8);
                let mut encoder = Encoder::new(&mut buffer, quality.clamp(1, 100));
                // 4:2:0 chroma: roughly a third smaller and faster; the usual choice for screen video.
                encoder.set_sampling_factor(SamplingFactor::F_2_2);
                // RGBA is encoded directly (alpha ignored), no intermediate RGB copy.
                encoder.encode(pixels, w as u16, h as u16, ColorType::Rgba)?;
                Ok(EncodedFrame {
                    format: FrameFormat::Jpeg { quality },
                    width: w as u32,
                    height: h as u32,
                    data: buffer,
                })
            }
        }
    }
}

/// 2x2 box filter. Returns the new (width, height); output is opaque RGBA in `dst`.
fn downscale_half(src: &[u8], w: usize, h: usize, dst: &mut Vec<u8>) -> (usize, usize) {
    let (nw, nh) = (w / 2, h / 2);
    dst.clear();
    dst.resize(nw * nh * 4, 255);
    for y in 0..nh {
        let r0 = &src[(2 * y) * w * 4..(2 * y) * w * 4 + w * 4];
        let r1 = &src[(2 * y + 1) * w * 4..(2 * y + 1) * w * 4 + w * 4];
        let out = &mut dst[y * nw * 4..(y + 1) * nw * 4];
        for x in 0..nw {
            let i = x * 8;
            let o = x * 4;
            for c in 0..3 {
                let sum = r0[i + c] as u16 + r0[i + 4 + c] as u16 + r1[i + c] as u16 + r1[i + 4 + c] as u16;
                out[o + c] = ((sum + 2) >> 2) as u8;
            }
        }
    }
    (nw, nh)
}

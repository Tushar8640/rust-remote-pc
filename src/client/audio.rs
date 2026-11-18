//! Client-side playback of the host's audio stream.
//!
//! Network packets (16-bit PCM at the host's rate) are resampled to the local
//! output device's rate, buffered in a small jitter buffer and played by a cpal
//! output stream that lives on a dedicated thread.
use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{info, warn};

/// Start playing once this much audio is buffered (absorbs network jitter).
const PREFILL_MS: usize = 80;
/// Never hold more than this much audio; older samples are dropped to bound latency.
const MAX_BUFFER_MS: usize = 300;

/// Shared controls the UI can read/write.
pub struct AudioControl {
    muted: AtomicBool,
    volume_bits: AtomicU32,
    receiving: AtomicBool,
}

impl AudioControl {
    fn new() -> Self {
        Self {
            muted: AtomicBool::new(false),
            volume_bits: AtomicU32::new(1.0f32.to_bits()),
            receiving: AtomicBool::new(false),
        }
    }
    pub fn muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
    }
    pub fn set_muted(&self, m: bool) {
        self.muted.store(m, Ordering::Relaxed);
    }
    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume_bits.load(Ordering::Relaxed))
    }
    pub fn set_volume(&self, v: f32) {
        self.volume_bits.store(v.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }
    /// True once audio packets have arrived from the host.
    pub fn receiving(&self) -> bool {
        self.receiving.load(Ordering::Relaxed)
    }
}

struct Buffer {
    samples: VecDeque<f32>, // interleaved, `out_channels` per frame
    primed: bool,
}

struct Resampler {
    pos: f64,
    last: [f32; 2],
}

pub struct AudioPlayer {
    buf: Arc<Mutex<Buffer>>,
    control: Arc<AudioControl>,
    out_rate: u32,
    out_channels: usize,
    resampler: Resampler,
    stop: Arc<AtomicBool>,
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl AudioPlayer {
    pub fn new() -> Result<(Self, Arc<AudioControl>)> {
        let buf = Arc::new(Mutex::new(Buffer { samples: VecDeque::new(), primed: false }));
        let control = Arc::new(AudioControl::new());
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) =
            std::sync::mpsc::channel::<std::result::Result<(u32, usize), String>>();

        {
            let buf = buf.clone();
            let control = control.clone();
            let stop = stop.clone();
            std::thread::Builder::new().name("audio-playback".into()).spawn(move || {
                match build_stream(buf, control) {
                    Ok((stream, rate, ch)) => {
                        let _ = ready_tx.send(Ok((rate, ch)));
                        while !stop.load(Ordering::Relaxed) {
                            std::thread::sleep(Duration::from_millis(50));
                        }
                        drop(stream);
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e.to_string()));
                    }
                }
            })?;
        }

        let (out_rate, out_channels) = match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => return Err(anyhow!(e)),
            Err(_) => {
                stop.store(true, Ordering::Relaxed);
                return Err(anyhow!("Timed out starting audio playback"));
            }
        };

        Ok((
            Self {
                buf,
                control: control.clone(),
                out_rate,
                out_channels,
                resampler: Resampler { pos: 0.0, last: [0.0; 2] },
                stop,
            },
            control,
        ))
    }

    /// Queue interleaved little-endian i16 PCM from the host.
    pub fn push(&mut self, sample_rate: u32, channels: u16, pcm_le: &[u8]) {
        if sample_rate == 0 || channels == 0 || channels > 2 {
            return;
        }
        self.control.receiving.store(true, Ordering::Relaxed);

        // Decode to stereo f32 frames.
        let ch = channels as usize;
        let mut frames: Vec<[f32; 2]> = Vec::with_capacity(pcm_le.len() / (2 * ch));
        for f in pcm_le.chunks_exact(2 * ch) {
            let l = i16::from_le_bytes([f[0], f[1]]) as f32 / 32768.0;
            let r = if ch == 2 { i16::from_le_bytes([f[2], f[3]]) as f32 / 32768.0 } else { l };
            frames.push([l, r]);
        }
        if frames.is_empty() {
            return;
        }

        // Linear resample to the output rate.
        let step = sample_rate as f64 / self.out_rate as f64;
        let mut src = Vec::with_capacity(frames.len() + 1);
        src.push(self.resampler.last);
        src.extend_from_slice(&frames);
        let mut out: Vec<f32> = Vec::with_capacity(
            ((frames.len() as f64 / step) as usize + 2) * self.out_channels,
        );
        let mut t = self.resampler.pos;
        while (t as usize) + 1 < src.len() {
            let i = t as usize;
            let frac = (t - i as f64) as f32;
            let l = src[i][0] + (src[i + 1][0] - src[i][0]) * frac;
            let r = src[i][1] + (src[i + 1][1] - src[i][1]) * frac;
            match self.out_channels {
                1 => out.push((l + r) * 0.5),
                _ => {
                    out.push(l);
                    out.push(r);
                    for _ in 2..self.out_channels {
                        out.push(0.0);
                    }
                }
            }
            t += step;
        }
        self.resampler.pos = t - (src.len() - 1) as f64;
        self.resampler.last = src[src.len() - 1];

        let max_samples = self.out_rate as usize * self.out_channels * MAX_BUFFER_MS / 1000;
        if let Ok(mut b) = self.buf.lock() {
            b.samples.extend(out);
            let excess = b.samples.len().saturating_sub(max_samples);
            if excess > 0 {
                // Drop whole frames only so channels stay aligned.
                let drop_n = excess - excess % self.out_channels;
                b.samples.drain(..drop_n);
            }
        }
    }
}

fn build_stream(
    buf: Arc<Mutex<Buffer>>,
    control: Arc<AudioControl>,
) -> Result<(cpal::Stream, u32, usize)> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow!("No audio output device"))?;
    let config = device.default_output_config()?;
    let rate = config.sample_rate();
    let channels = (config.channels() as usize).max(1);
    let format = config.sample_format();
    info!("Audio playback: {} Hz, {} ch, {:?}", rate, channels, format);

    let cfg: cpal::StreamConfig = config.into();
    let stream = match format {
        SampleFormat::F32 => out_stream::<f32>(&device, cfg, buf, control, rate, channels)?,
        SampleFormat::I16 => out_stream::<i16>(&device, cfg, buf, control, rate, channels)?,
        SampleFormat::I32 => out_stream::<i32>(&device, cfg, buf, control, rate, channels)?,
        SampleFormat::U16 => out_stream::<u16>(&device, cfg, buf, control, rate, channels)?,
        other => return Err(anyhow!("Unsupported audio sample format: {}", other)),
    };
    stream.play()?;
    Ok((stream, rate, channels))
}

fn out_stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    buf: Arc<Mutex<Buffer>>,
    control: Arc<AudioControl>,
    rate: u32,
    channels: usize,
) -> Result<cpal::Stream>
where
    T: SizedSample + FromSample<f32>,
{
    let prefill = rate as usize * channels * PREFILL_MS / 1000;
    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &_| {
            let gain = if control.muted() { 0.0 } else { control.volume() };
            let mut b = match buf.try_lock() {
                Ok(g) => g,
                Err(_) => {
                    for s in data.iter_mut() {
                        *s = T::from_sample(0.0f32);
                    }
                    return;
                }
            };
            if !b.primed && b.samples.len() >= prefill {
                b.primed = true;
            }
            for s in data.iter_mut() {
                let v = if b.primed {
                    match b.samples.pop_front() {
                        Some(v) => v,
                        None => {
                            // Underrun: wait for the buffer to refill before resuming.
                            b.primed = false;
                            0.0
                        }
                    }
                } else {
                    0.0
                };
                *s = T::from_sample((v * gain).clamp(-1.0, 1.0));
            }
        },
        |e| warn!("Audio playback stream error: {}", e),
        None,
    )?;
    Ok(stream)
}

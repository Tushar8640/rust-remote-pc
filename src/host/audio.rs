//! Host-side system audio capture.
//!
//! On Windows (WASAPI) opening an *input* stream on the default *output*
//! device records exactly what the speakers are playing ("loopback"), so the
//! remote client can hear the host's audio.
//!
//! The audio is sent as 20 ms packets of 16-bit PCM (stereo at most).
use crate::common::protocol::Packet;
use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{info, warn};

/// Handle to a running capture. Dropping it stops the capture.
pub struct AudioCapture {
    stop: Arc<AtomicBool>,
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl AudioCapture {
    pub fn start(tx: mpsc::Sender<Packet>) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<std::result::Result<(), String>>();
        let stop_thread = stop.clone();

        // cpal streams are not Send, so the stream lives on its own thread.
        std::thread::Builder::new()
            .name("audio-capture".into())
            .spawn(move || match build_stream(tx) {
                Ok(stream) => {
                    let _ = ready_tx.send(Ok(()));
                    while !stop_thread.load(Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    drop(stream);
                    info!("Audio capture stopped");
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e.to_string()));
                }
            })?;

        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => Ok(Self { stop }),
            Ok(Err(e)) => Err(anyhow!(e)),
            Err(_) => {
                stop.store(true, Ordering::Relaxed);
                Err(anyhow!("Timed out starting audio capture"))
            }
        }
    }
}

fn build_stream(tx: mpsc::Sender<Packet>) -> Result<cpal::Stream> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow!("No default audio output device to capture"))?;

    // Loopback: use the output device's own format as the input format.
    let config = device.default_output_config()?;
    let rate = config.sample_rate();
    let channels = config.channels() as usize;
    let format = config.sample_format();
    info!("Audio capture: {} Hz, {} ch, {:?}", rate, channels, format);

    let cfg: cpal::StreamConfig = config.into();
    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, cfg, channels, rate, tx)?,
        SampleFormat::I16 => build::<i16>(&device, cfg, channels, rate, tx)?,
        SampleFormat::I32 => build::<i32>(&device, cfg, channels, rate, tx)?,
        SampleFormat::U16 => build::<u16>(&device, cfg, channels, rate, tx)?,
        other => return Err(anyhow!("Unsupported audio sample format: {}", other)),
    };
    stream.play()?;
    Ok(stream)
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    rate: u32,
    tx: mpsc::Sender<Packet>,
) -> Result<cpal::Stream>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = channels.max(1);
    let out_ch = channels.min(2);
    // 20 ms of audio per packet.
    let chunk = (rate as usize / 50).max(1) * out_ch;
    let mut acc: Vec<i16> = Vec::with_capacity(chunk * 2);

    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &_| {
            for frame in data.chunks(channels) {
                for c in 0..out_ch {
                    let s: f32 = frame[c].to_sample::<f32>();
                    acc.push((s.clamp(-1.0, 1.0) * 32767.0) as i16);
                }
            }
            while acc.len() >= chunk {
                let mut bytes = Vec::with_capacity(chunk * 2);
                for s in acc.drain(..chunk) {
                    bytes.extend_from_slice(&s.to_le_bytes());
                }
                // Never block the audio thread; drop the packet if the queue is full.
                let _ = tx.try_send(Packet::Audio {
                    sample_rate: rate,
                    channels: out_ch as u16,
                    data: bytes,
                });
            }
        },
        |e| warn!("Audio capture stream error: {}", e),
        None,
    )?;
    Ok(stream)
}

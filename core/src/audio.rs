//! Microphone capture. Produces 16 kHz mono `f32` samples.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};

use crate::rpc;

pub const SAMPLE_RATE: u32 = 16_000;
/// Hard stop so a forgotten toggle cannot fill memory.
const MAX_SECONDS: u64 = 15 * 60;

/// An open input stream, its sample buffer, and its sample rate.
type Capture = (cpal::Stream, Arc<Mutex<Vec<f32>>>, u32);

pub struct Recorder {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<Result<Vec<f32>, String>>,
}

impl Recorder {
    /// Stops capture and returns the recording at 16 kHz mono.
    pub fn finish(self) -> Result<Vec<f32>, String> {
        self.stop.store(true, Ordering::SeqCst);
        self.handle.join().map_err(|_| "The audio thread crashed".to_string())?
    }
}

pub fn input_devices() -> Vec<String> {
    let host = cpal::default_host();
    match host.input_devices() {
        Ok(devices) => devices.filter_map(|d| d.name().ok()).collect(),
        Err(_) => Vec::new(),
    }
}

fn pick_device(name: Option<&str>) -> Result<cpal::Device, String> {
    let host = cpal::default_host();
    if let Some(name) = name {
        if let Ok(mut devices) = host.input_devices() {
            if let Some(d) = devices.find(|d| d.name().map(|n| n == name).unwrap_or(false)) {
                return Ok(d);
            }
        }
        eprintln!("input device '{name}' not found, using the default");
    }
    host.default_input_device()
        .ok_or_else(|| "No microphone found. Plug one in or check your sound settings.".to_string())
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    buffer: Arc<Mutex<Vec<f32>>>,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    let channels = config.channels.max(1) as usize;
    device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let mut buf = buffer.lock().unwrap();
            for frame in data.chunks(channels) {
                let sum: f32 = frame.iter().map(|s| f32::from_sample(*s)).sum();
                buf.push(sum / frame.len() as f32);
            }
        },
        |err| eprintln!("audio stream error: {err}"),
        None,
    )
}

/// Starts recording. Returns once the device is actually capturing.
///
/// With `live`, 16 kHz audio is also sent down the channel every 50 ms while recording; the
/// channel closes when the recording ends.
pub fn start(device_name: Option<String>, live: Option<mpsc::Sender<Vec<f32>>>) -> Result<Recorder, String> {
    let stop = Arc::new(AtomicBool::new(false));
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();

    let stop_flag = stop.clone();
    let handle = std::thread::spawn(move || -> Result<Vec<f32>, String> {
        // The stream is created, used and dropped on this thread: it is not `Send` everywhere.
        let setup = || -> Result<Capture, String> {
            let device = pick_device(device_name.as_deref())?;
            let supported = device
                .default_input_config()
                .map_err(|e| format!("Could not open the microphone: {e}"))?;
            let format = supported.sample_format();
            let config: cpal::StreamConfig = supported.into();
            let rate = config.sample_rate.0;
            let buffer = Arc::new(Mutex::new(Vec::<f32>::with_capacity(rate as usize * 30)));
            let stream = match format {
                cpal::SampleFormat::F32 => build_stream::<f32>(&device, &config, buffer.clone()),
                cpal::SampleFormat::I16 => build_stream::<i16>(&device, &config, buffer.clone()),
                cpal::SampleFormat::U16 => build_stream::<u16>(&device, &config, buffer.clone()),
                cpal::SampleFormat::I32 => build_stream::<i32>(&device, &config, buffer.clone()),
                cpal::SampleFormat::U8 => build_stream::<u8>(&device, &config, buffer.clone()),
                other => return Err(format!("Unsupported microphone sample format: {other:?}")),
            }
            .map_err(|e| format!("Could not open the microphone: {e}"))?;
            stream
                .play()
                .map_err(|e| format!("Could not start the microphone: {e}"))?;
            Ok((stream, buffer, rate))
        };

        let (stream, buffer, rate) = match setup() {
            Ok(v) => {
                let _ = ready_tx.send(Ok(()));
                v
            }
            Err(e) => {
                let _ = ready_tx.send(Err(e.clone()));
                return Err(e);
            }
        };

        let started = Instant::now();
        let window = (rate as usize / 20).max(1); // 50 ms
                                                  // Streaming resampler for live chunks: it carries filter state from one chunk to the next.
        let streamer = if live.is_some() && rate != SAMPLE_RATE {
            sherpa_onnx::LinearResampler::create(rate as i32, SAMPLE_RATE as i32)
        } else {
            None
        };
        let mut sent = 0usize;
        let mut send_fresh = |last: bool| {
            let Some(tx) = &live else { return };
            let fresh: Vec<f32> = {
                let buf = buffer.lock().unwrap();
                let fresh = buf[sent..].to_vec();
                sent = buf.len();
                fresh
            };
            if fresh.is_empty() && !last {
                return;
            }
            let chunk = match &streamer {
                Some(r) => r.resample(&fresh, last),
                None => linear_resample(&fresh, rate, SAMPLE_RATE),
            };
            let _ = tx.send(chunk);
        };
        while !stop_flag.load(Ordering::SeqCst) && started.elapsed().as_secs() < MAX_SECONDS {
            std::thread::sleep(Duration::from_millis(50));
            let level = {
                let buf = buffer.lock().unwrap();
                let tail = &buf[buf.len().saturating_sub(window)..];
                rms(tail)
            };
            rpc::emit("level", level);
            send_fresh(false);
        }
        drop(stream);
        send_fresh(true);

        let samples = std::mem::take(&mut *buffer.lock().unwrap());
        Ok(resample(&samples, rate))
    });

    match ready_rx.recv_timeout(Duration::from_secs(8)) {
        Ok(Ok(())) => Ok(Recorder { stop, handle }),
        Ok(Err(e)) => Err(e),
        Err(_) => {
            stop.store(true, Ordering::SeqCst);
            Err("The microphone did not respond.".to_string())
        }
    }
}

pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

pub fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0f32, |m, s| m.max(s.abs()))
}

/// Converts mono audio at `rate` Hz to 16 kHz.
pub fn resample(samples: &[f32], rate: u32) -> Vec<f32> {
    if rate == SAMPLE_RATE || samples.is_empty() {
        return samples.to_vec();
    }
    match sherpa_onnx::LinearResampler::create(rate as i32, SAMPLE_RATE as i32) {
        Some(r) => r.resample(samples, true),
        None => linear_resample(samples, rate, SAMPLE_RATE),
    }
}

/// Fallback resampler: plain linear interpolation.
pub fn linear_resample(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || samples.is_empty() {
        return samples.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let out_len = (samples.len() as f64 / ratio).floor() as usize;
    (0..out_len)
        .map(|i| {
            let pos = i as f64 * ratio;
            let idx = pos.floor() as usize;
            let frac = (pos - idx as f64) as f32;
            let a = samples[idx];
            let b = samples[(idx + 1).min(samples.len() - 1)];
            a + (b - a) * frac
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rms_and_peak() {
        assert_eq!(rms(&[]), 0.0);
        assert!((rms(&[0.5, -0.5, 0.5, -0.5]) - 0.5).abs() < 1e-6);
        assert_eq!(peak(&[0.1, -0.8, 0.3]), 0.8);
    }

    #[test]
    fn linear_resample_halves_length() {
        let input: Vec<f32> = (0..3200).map(|i| i as f32).collect();
        let out = linear_resample(&input, 32_000, 16_000);
        assert_eq!(out.len(), 1600);
        assert_eq!(out[10], 20.0);
    }
}

//! Output devices and mixing callbacks (cpal).

use super::{Levels, Tap};
use crate::model::AudioDevice;
use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};
use crossbeam_channel::{bounded, Receiver, Sender};
use parking_lot::Mutex;
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::sync::Arc;

#[derive(Debug, Clone, Default)]
pub struct OutputConfig {
    pub master_device: Option<String>,
    pub master_channels: [u16; 2],
    pub cue_device: Option<String>,
    pub cue_channels: [u16; 2],
    /// Output latency (device buffer) in ms.
    pub buffer_ms: u32,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StreamInfo {
    pub device: String,
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OutputStatus {
    pub master: Option<StreamInfo>,
    pub cue: Option<StreamInfo>,
    /// Headphone cue shares the master channels: preview would be audible in the room.
    pub cue_on_master_channels: bool,
    pub errors: Vec<String>,
}

pub struct Assign {
    tap: Arc<Tap>,
    ch: [usize; 2],
    cue: bool,
}

enum Cmd {
    Apply {
        config: OutputConfig,
        master_taps: Vec<Arc<Tap>>,
        cue_taps: Vec<Arc<Tap>>,
        reply: Sender<(u32, OutputStatus)>,
    },
}

/// Owns the cpal streams on a dedicated thread (streams aren't `Send` on
/// every platform).
pub struct Outputs {
    tx: Sender<Cmd>,
    pub status: Arc<Mutex<OutputStatus>>,
}

impl Outputs {
    pub fn new(levels: Arc<Levels>) -> Self {
        let (tx, rx) = bounded::<Cmd>(4);
        let status = Arc::new(Mutex::new(OutputStatus::default()));
        let st = status.clone();
        std::thread::Builder::new()
            .name("audio-outputs".into())
            .spawn(move || output_thread(rx, levels, st))
            .expect("spawn output thread");
        Self { tx, status }
    }

    /// (Re)opens output devices. Returns the engine sample rate.
    pub fn apply(&self, config: OutputConfig, master_taps: Vec<Arc<Tap>>, cue_taps: Vec<Arc<Tap>>) -> (u32, OutputStatus) {
        let (reply, rx) = bounded(1);
        if self.tx.send(Cmd::Apply { config, master_taps, cue_taps, reply }).is_err() {
            return (48000, OutputStatus { errors: vec!["audio thread stopped".into()], ..Default::default() });
        }
        rx.recv().unwrap_or((48000, OutputStatus::default()))
    }
}

fn output_thread(rx: Receiver<Cmd>, levels: Arc<Levels>, status: Arc<Mutex<OutputStatus>>) {
    let mut streams: Vec<cpal::Stream> = vec![];
    while let Ok(cmd) = rx.recv() {
        match cmd {
            Cmd::Apply { config, master_taps, cue_taps, reply } => {
                streams.clear();
                let (rate, new_streams, st) = open_streams(&config, master_taps, cue_taps, &levels, status.clone());
                streams = new_streams;
                *status.lock() = st.clone();
                let _ = reply.send((rate, st));
            }
        }
    }
}

pub fn host() -> cpal::Host {
    cpal::default_host()
}

fn device_id(d: &cpal::Device) -> String {
    d.id().map(|i| i.to_string()).unwrap_or_else(|_| d.to_string())
}

fn device_name(d: &cpal::Device) -> String {
    d.to_string()
}

fn max_channels(d: &cpal::Device) -> u16 {
    d.supported_output_configs().map(|c| c.map(|c| c.channels()).max().unwrap_or(2)).unwrap_or(2)
}

pub fn list_devices() -> Vec<AudioDevice> {
    let host = host();
    let default_id = host.default_output_device().map(|d| device_id(&d));
    let mut out = vec![];
    if let Ok(devs) = host.output_devices() {
        for d in devs {
            let id = device_id(&d);
            out.push(AudioDevice {
                is_default: Some(&id) == default_id.as_ref(),
                name: device_name(&d),
                channels: max_channels(&d),
                id,
            });
        }
    }
    out
}

fn find_device(id: &Option<String>) -> Result<cpal::Device> {
    let host = host();
    match id {
        None => host.default_output_device().ok_or_else(|| anyhow!("no default output device")),
        Some(id) => host
            .output_devices()?
            .find(|d| &device_id(d) == id)
            .ok_or_else(|| anyhow!("output device not found: {id}")),
    }
}

fn choose_config(device: &cpal::Device, needed_channels: u16, preferred_rate: Option<u32>, buffer_ms: u32) -> Result<(StreamConfig, SampleFormat)> {
    let default = device.default_output_config().context("no default output config")?;
    let want_rate = preferred_rate.unwrap_or(default.sample_rate());
    let mut best: Option<cpal::SupportedStreamConfigRange> = None;
    for c in device.supported_output_configs()? {
        if c.channels() < needed_channels {
            continue;
        }
        let rate_ok = c.min_sample_rate() <= want_rate && want_rate <= c.max_sample_rate();
        let better = match &best {
            None => true,
            Some(b) => {
                let b_rate_ok = b.min_sample_rate() <= want_rate && want_rate <= b.max_sample_rate();
                (rate_ok && !b_rate_ok)
                    || (rate_ok == b_rate_ok && c.channels() < b.channels())
                    || (rate_ok == b_rate_ok && c.channels() == b.channels() && c.sample_format() == SampleFormat::F32)
            }
        };
        if better {
            best = Some(c);
        }
    }
    match best {
        Some(c) => {
            let rate = want_rate.clamp(c.min_sample_rate(), c.max_sample_rate());
            let fmt = c.sample_format();
            let sc = c.with_sample_rate(rate);
            Ok((StreamConfig { channels: sc.channels(), sample_rate: rate, buffer_size: device_buffer(c.buffer_size(), rate, buffer_ms) }, fmt))
        }
        None => {
            // Device has fewer channels than requested; use its default.
            Ok((default.config(), default.sample_format()))
        }
    }
}

/// Device buffer for a latency of `buffer_ms`: a fixed size (some systems'
/// defaults are tiny and underrun), within what the device supports.
fn device_buffer(supported: &cpal::SupportedBufferSize, rate: u32, buffer_ms: u32) -> cpal::BufferSize {
    let want = (rate as u64 * buffer_ms.clamp(10, 200) as u64 / 1000).next_power_of_two() as u32;
    match supported {
        cpal::SupportedBufferSize::Range { min, max } => cpal::BufferSize::Fixed(want.clamp(*min, *max)),
        cpal::SupportedBufferSize::Unknown => cpal::BufferSize::Default,
    }
}

fn open_streams(
    config: &OutputConfig,
    master_taps: Vec<Arc<Tap>>,
    cue_taps: Vec<Arc<Tap>>,
    levels: &Arc<Levels>,
    status: Arc<Mutex<OutputStatus>>,
) -> (u32, Vec<cpal::Stream>, OutputStatus) {
    let mut st = OutputStatus::default();
    let mut streams = vec![];
    let mut rate = 48000;

    let master_dev = match find_device(&config.master_device) {
        Ok(d) => Some(d),
        Err(e) => {
            st.errors.push(format!("Master output: {e:#}"));
            None
        }
    };
    let master_id = master_dev.as_ref().map(device_id);
    let cue_dev = match &config.cue_device {
        None => None,
        Some(_) => match find_device(&config.cue_device) {
            Ok(d) => Some(d),
            Err(e) => {
                st.errors.push(format!("Headphone output: {e:#}"));
                None
            }
        },
    };
    let cue_same_device = cue_dev.as_ref().map(device_id).is_some() && cue_dev.as_ref().map(device_id) == master_id;
    st.cue_on_master_channels = cue_same_device && config.cue_channels == config.master_channels;

    if let Some(dev) = &master_dev {
        let mut needed = config.master_channels.iter().max().copied().unwrap_or(1) + 1;
        if cue_same_device {
            needed = needed.max(config.cue_channels.iter().max().copied().unwrap_or(1) + 1);
        }
        match choose_config(dev, needed, None, config.buffer_ms) {
            Ok((sc, fmt)) => {
                rate = sc.sample_rate;
                let mut assigns: Vec<Assign> = master_taps
                    .iter()
                    .map(|t| Assign { tap: t.clone(), ch: chans(config.master_channels, sc.channels), cue: false })
                    .collect();
                if cue_same_device {
                    assigns.extend(cue_taps.iter().map(|t| Assign {
                        tap: t.clone(),
                        ch: chans(config.cue_channels, sc.channels),
                        cue: true,
                    }));
                }
                match build_stream(dev, &sc, fmt, assigns, levels.clone(), status.clone(), true) {
                    Ok(s) => {
                        st.master = Some(StreamInfo { device: device_name(dev), sample_rate: sc.sample_rate, channels: sc.channels });
                        if cue_same_device {
                            st.cue = st.master.clone();
                        }
                        streams.push(s);
                    }
                    Err(e) => st.errors.push(format!("Master output: {e:#}")),
                }
            }
            Err(e) => st.errors.push(format!("Master output: {e:#}")),
        }
    }

    if let (Some(dev), false) = (&cue_dev, cue_same_device) {
        let needed = config.cue_channels.iter().max().copied().unwrap_or(1) + 1;
        match choose_config(dev, needed, Some(rate), config.buffer_ms) {
            Ok((sc, fmt)) => {
                let assigns = cue_taps
                    .iter()
                    .map(|t| Assign { tap: t.clone(), ch: chans(config.cue_channels, sc.channels), cue: true })
                    .collect();
                match build_stream(dev, &sc, fmt, assigns, levels.clone(), status.clone(), false) {
                    Ok(s) => {
                        st.cue = Some(StreamInfo { device: device_name(dev), sample_rate: sc.sample_rate, channels: sc.channels });
                        streams.push(s);
                    }
                    Err(e) => st.errors.push(format!("Headphone output: {e:#}")),
                }
            }
            Err(e) => st.errors.push(format!("Headphone output: {e:#}")),
        }
    }
    (rate, streams, st)
}

fn chans(pair: [u16; 2], device_channels: u16) -> [usize; 2] {
    let max = device_channels.max(1) as usize - 1;
    [(pair[0] as usize).min(max), (pair[1] as usize).min(max)]
}

fn build_stream(
    dev: &cpal::Device,
    sc: &StreamConfig,
    fmt: SampleFormat,
    assigns: Vec<Assign>,
    levels: Arc<Levels>,
    status: Arc<Mutex<OutputStatus>>,
    is_master: bool,
) -> Result<cpal::Stream> {
    let stream = match fmt {
        SampleFormat::F32 => build_typed::<f32>(dev, sc, assigns, levels, status, is_master),
        SampleFormat::I16 => build_typed::<i16>(dev, sc, assigns, levels, status, is_master),
        SampleFormat::I32 => build_typed::<i32>(dev, sc, assigns, levels, status, is_master),
        SampleFormat::U16 => build_typed::<u16>(dev, sc, assigns, levels, status, is_master),
        SampleFormat::F64 => build_typed::<f64>(dev, sc, assigns, levels, status, is_master),
        SampleFormat::I8 => build_typed::<i8>(dev, sc, assigns, levels, status, is_master),
        SampleFormat::U8 => build_typed::<u8>(dev, sc, assigns, levels, status, is_master),
        other => Err(anyhow!("unsupported sample format {other:?}")),
    }?;
    stream.play()?;
    Ok(stream)
}

/// Fill level a follower ring is held at (the clock ring stays near full).
const FOLLOWER_FILL: f64 = 0.7;

#[inline]
fn pop_frame(c: &mut rtrb::Consumer<f32>) -> [f32; 2] {
    if c.slots() >= 2 {
        let l = c.pop().unwrap_or(0.0);
        let r = c.pop().unwrap_or(0.0);
        [l, r]
    } else {
        [0.0, 0.0]
    }
}

fn build_typed<T>(
    dev: &cpal::Device,
    sc: &StreamConfig,
    assigns: Vec<Assign>,
    levels: Arc<Levels>,
    status: Arc<Mutex<OutputStatus>>,
    is_master: bool,
) -> Result<cpal::Stream>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = sc.channels as usize;
    let device_rate = sc.sample_rate as f64;
    let mut mix: Vec<f32> = vec![0.0; 8192 * channels];
    let ramp_step = (1.0 / (0.008 * device_rate)) as f32;
    // One-pole smoothing with a ~5 ms time constant.
    let gain_step = (1.0 - (-1.0 / (0.005 * device_rate)).exp()) as f32;
    let err_status = status.clone();
    let mut last_start: Option<std::time::Instant> = None;
    let stream = dev.build_output_stream::<T, _, _>(
        sc.clone(),
        move |data: &mut [T], _info| {
            let start = std::time::Instant::now();
            if mix.len() < data.len() {
                mix.resize(data.len(), 0.0);
            }
            let mix = &mut mix[..data.len()];
            mix.fill(0.0);
            let frames = data.len() / channels;
            let mut master_peak = 0.0f32;
            let mut cue_peak = 0.0f32;
            for a in &assigns {
                let level = if a.cue { levels.cue.get() } else { levels.master.get() };
                let volume = a.tap.volume.get();
                let target_gain = a.tap.gain.get() * volume;
                let mut deck_peak = 0.0f32;
                let playing = a.tap.playing.load(Ordering::Relaxed);
                let Some(mut guard) = a.tap.ring.try_lock() else { continue };
                let Some(ring) = guard.as_mut() else { continue };
                let mut step = a.tap.rate.load(Ordering::Relaxed) as f64 / device_rate;
                if ring.follower {
                    // Keep the ring half full: speed up slightly when it fills,
                    // slow down when it drains (at most 0.2%, inaudible).
                    let fill = ring.consumer.slots() as f64 / ring.consumer.buffer().capacity().max(1) as f64;
                    let target = ((fill - FOLLOWER_FILL) * 0.004).clamp(-0.002, 0.002);
                    ring.drift += (target - ring.drift) * 0.01;
                    step *= 1.0 + ring.drift;
                }
                let resample = ring.follower || (step - 1.0).abs() > 1e-9;
                let mut starved = 0u64;
                for f in 0..frames {
                    let target = if playing { 1.0 } else { 0.0 };
                    if !playing && ring.ramp <= 0.0 {
                        break;
                    }
                    if ring.ramp < target {
                        ring.ramp = (ring.ramp + ramp_step).min(1.0);
                    } else if ring.ramp > target {
                        ring.ramp = (ring.ramp - ramp_step).max(0.0);
                    }
                    if playing && ring.consumer.slots() < 2 {
                        starved += 1;
                    }
                    let fr = if !resample {
                        pop_frame(&mut ring.consumer)
                    } else {
                        while ring.frac >= 1.0 {
                            ring.prev = ring.cur;
                            ring.cur = pop_frame(&mut ring.consumer);
                            ring.frac -= 1.0;
                        }
                        let t = ring.frac as f32;
                        let v = [
                            ring.prev[0] + (ring.cur[0] - ring.prev[0]) * t,
                            ring.prev[1] + (ring.cur[1] - ring.prev[1]) * t,
                        ];
                        ring.frac += step;
                        v
                    };
                    ring.gain += (target_gain - ring.gain) * gain_step;
                    let g = level * ring.gain * ring.ramp;
                    let (l, r) = (fr[0] * g, fr[1] * g);
                    // The deck's own level: after its volume, before routing.
                    deck_peak = deck_peak.max(fr[0].abs().max(fr[1].abs()) * volume * ring.ramp);
                    let base = f * channels;
                    if a.ch[0] == a.ch[1] {
                        mix[base + a.ch[0]] += (l + r) * 0.5;
                    } else {
                        mix[base + a.ch[0]] += l;
                        mix[base + a.ch[1]] += r;
                    }
                    let p = l.abs().max(r.abs());
                    if a.cue {
                        cue_peak = cue_peak.max(p);
                    } else {
                        master_peak = master_peak.max(p);
                    }
                }
                if starved > 0 {
                    a.tap.underruns.fetch_add(starved, Ordering::Relaxed);
                }
                if deck_peak > a.tap.peak.get() {
                    a.tap.peak.set(deck_peak);
                }
            }
            for (d, s) in data.iter_mut().zip(mix.iter()) {
                *d = T::from_sample(s.clamp(-1.0, 1.0));
            }
            if master_peak > levels.master_peak.get() {
                levels.master_peak.set(master_peak);
            }
            if cue_peak > levels.cue_peak.get() {
                levels.cue_peak.set(cue_peak);
            }
            let period = std::time::Duration::from_secs_f64(frames as f64 / device_rate);
            levels.timing[if is_master { 0 } else { 1 }].record(last_start.map(|l| start - l), start.elapsed(), period);
            last_start = Some(start);
        },
        move |err| {
            log::error!("audio stream error ({}): {err}", if is_master { "master" } else { "cue" });
            let mut s = err_status.lock();
            let msg = format!("{} output: {err}", if is_master { "Master" } else { "Headphone" });
            if !s.errors.contains(&msg) {
                s.errors.push(msg);
            }
        },
        None,
    )?;
    let _ = status;
    Ok(stream)
}

//! Time-stretching engines (tempo change with or without pitch change), chosen
//! in Settings → General:
//!
//! - **Signalsmith Stretch** (default): spectral; smooth on sustained sounds.
//! - **SoundTouch**: time-domain (WSOLA); keeps attacks and rhythm crisp, often
//!   better for larger slow-downs of rhythmic music.
//! - **Bungee**: modern granular phase vocoder; clean across a wide range.
//!
//! All take stereo interleaved input at the source rate and produce output at
//! the deck rate: the deck passes the frequency factor (`set_pitch`) that also
//! covers sample-rate conversion, and asks for an exact number of frames per
//! block. Engines that decide their own output size (SoundTouch, Bungee) are
//! wrapped with an output FIFO.

use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Signalsmith,
    SoundTouch,
    Bungee,
}

impl Engine {
    pub fn from_setting(s: &str) -> Self {
        match s {
            "soundtouch" => Engine::SoundTouch,
            "bungee" => Engine::Bungee,
            _ => Engine::Signalsmith,
        }
    }
}

pub trait TimeStretch: Send {
    /// Frequency factor applied to the output (sample-rate conversion and,
    /// without key lock, the tempo's pitch change).
    fn set_pitch(&mut self, factor: f32);
    /// Stretches `input` (interleaved stereo) into exactly `out.len() / 2` frames.
    fn process(&mut self, input: &[f32], out: &mut [f32]);
    /// The remaining output after the last input (end of the song).
    fn flush(&mut self, out: &mut Vec<f32>);
    /// Delay between input and output, in source frames.
    fn latency_src_frames(&self) -> f64;
    /// Forgets all state (after a seek).
    fn reset(&mut self);
}

pub fn create(engine: Engine, src_rate: u32) -> Box<dyn TimeStretch> {
    match engine {
        Engine::Signalsmith => Box::new(Signalsmith { s: signalsmith_stretch::Stretch::preset_default(2, src_rate) }),
        Engine::SoundTouch => Box::new(Fifo::new(SoundTouchEngine::new(src_rate))),
        Engine::Bungee => match BungeeEngine::new(src_rate) {
            Some(b) => Box::new(Fifo::new(b)),
            None => {
                log::warn!("Bungee unavailable; using Signalsmith Stretch");
                create(Engine::Signalsmith, src_rate)
            }
        },
    }
}

// ---------------------------------------------------------------- Signalsmith

struct Signalsmith {
    s: signalsmith_stretch::Stretch,
}

impl TimeStretch for Signalsmith {
    fn set_pitch(&mut self, factor: f32) {
        self.s.set_transpose_factor(factor, None);
    }
    fn process(&mut self, input: &[f32], out: &mut [f32]) {
        self.s.process(input, out);
    }
    fn flush(&mut self, out: &mut Vec<f32>) {
        out.resize(self.s.output_latency() * 2, 0.0);
        self.s.flush(out);
    }
    fn latency_src_frames(&self) -> f64 {
        (self.s.input_latency() + self.s.output_latency()) as f64
    }
    fn reset(&mut self) {
        self.s.reset();
    }
}

// ---------------------------------------------------------------- engines that size their own output

/// An engine that consumes input and produces however much output it has.
trait PushEngine: Send {
    /// Feeds `input` frames to be stretched by `ratio` (output / input frames)
    /// with frequency factor `pitch`; appends what is ready to `out`.
    fn push(&mut self, input: &[f32], ratio: f64, pitch: f64, out: &mut Vec<f32>);
    /// Appends the output still inside the engine after the last input.
    fn drain(&mut self, ratio: f64, pitch: f64, out: &mut Vec<f32>);
    fn reset(&mut self);
}

/// Gives a push engine the exact-size interface: output is queued, and
/// silence fills in until the engine's initial latency has passed.
struct Fifo<E: PushEngine> {
    engine: E,
    pitch: f64,
    ratio: f64,
    queue: VecDeque<f32>,
    scratch: Vec<f32>,
    /// Source frames fed in, and source-equivalent of engine output emitted.
    fed: f64,
    emitted: f64,
}

impl<E: PushEngine> Fifo<E> {
    fn new(engine: E) -> Self {
        Self { engine, pitch: 1.0, ratio: 1.0, queue: VecDeque::new(), scratch: vec![], fed: 0.0, emitted: 0.0 }
    }

    fn emit(&mut self, out: &mut [f32]) {
        let have = self.queue.len().min(out.len()) / 2 * 2;
        // Not enough engine output yet: silence first (the engine's latency).
        let pad = out.len() - have;
        out[..pad].fill(0.0);
        for (o, s) in out[pad..].iter_mut().zip(self.queue.drain(..have)) {
            *o = s;
        }
        self.emitted += (have / 2) as f64 / self.ratio.max(1e-6);
    }
}

impl<E: PushEngine> TimeStretch for Fifo<E> {
    fn set_pitch(&mut self, factor: f32) {
        self.pitch = factor as f64;
    }
    fn process(&mut self, input: &[f32], out: &mut [f32]) {
        let n = input.len() / 2;
        if n > 0 {
            self.ratio = (out.len() / 2) as f64 / n as f64;
            self.scratch.clear();
            self.engine.push(input, self.ratio, self.pitch, &mut self.scratch);
            self.queue.extend(self.scratch.drain(..));
            self.fed += n as f64;
        }
        self.emit(out);
    }
    fn flush(&mut self, out: &mut Vec<f32>) {
        self.scratch.clear();
        self.engine.drain(self.ratio, self.pitch, &mut self.scratch);
        self.queue.extend(self.scratch.drain(..));
        out.clear();
        out.extend(self.queue.drain(..));
    }
    fn latency_src_frames(&self) -> f64 {
        (self.fed - self.emitted).max(0.0)
    }
    fn reset(&mut self) {
        self.engine.reset();
        self.queue.clear();
        self.fed = 0.0;
        self.emitted = 0.0;
    }
}

// ---------------------------------------------------------------- SoundTouch

struct SoundTouchEngine {
    st: soundtouch::SoundTouch,
    rate: u32,
    settings: (f64, f64),
}

// SAFETY: the SoundTouch instance is owned by one deck thread at a time.
unsafe impl Send for SoundTouchEngine {}

impl SoundTouchEngine {
    fn new(src_rate: u32) -> Self {
        let mut s = Self { st: soundtouch::SoundTouch::new(), rate: src_rate, settings: (0.0, 0.0) };
        s.configure();
        s
    }

    fn configure(&mut self) {
        use soundtouch::Setting;
        self.st.set_channels(2).set_sample_rate(self.rate);
        // Full-quality seeking; anti-aliasing for the rate (pitch) change.
        self.st.set_setting(Setting::UseQuickseek, 0).set_setting(Setting::UseAaFilter, 1);
        self.settings = (0.0, 0.0);
    }

    /// SoundTouch's "rate" changes pitch (and length); "tempo" changes length only.
    fn apply(&mut self, ratio: f64, pitch: f64) {
        let tempo = 1.0 / (ratio * pitch);
        if (tempo, pitch) != self.settings {
            self.st.set_rate(pitch).set_tempo(tempo);
            self.settings = (tempo, pitch);
        }
    }

    fn receive(&mut self, out: &mut Vec<f32>) {
        const FRAMES: usize = 2048;
        let mut buf = [0.0f32; FRAMES * 2];
        loop {
            let got = self.st.receive_samples(&mut buf, FRAMES);
            if got == 0 {
                break;
            }
            out.extend_from_slice(&buf[..got * 2]);
        }
    }
}

impl PushEngine for SoundTouchEngine {
    fn push(&mut self, input: &[f32], ratio: f64, pitch: f64, out: &mut Vec<f32>) {
        self.apply(ratio, pitch);
        self.st.put_samples(input, input.len() / 2);
        self.receive(out);
    }
    fn drain(&mut self, ratio: f64, pitch: f64, out: &mut Vec<f32>) {
        self.apply(ratio, pitch);
        self.st.flush();
        self.receive(out);
    }
    fn reset(&mut self) {
        self.st.clear();
        self.configure();
    }
}

// ---------------------------------------------------------------- Bungee

/// Largest input block the deck passes (frames).
const BUNGEE_MAX_INPUT: usize = 2048;

struct BungeeEngine {
    stream: bungee_rs::Stream,
    rate: u32,
    input: [Vec<f32>; 2],
    output: [Vec<f32>; 2],
}

impl BungeeEngine {
    fn new(src_rate: u32) -> Option<Self> {
        let stream = bungee_rs::Stream::new(src_rate as usize, 2, BUNGEE_MAX_INPUT).ok()?;
        Some(Self { stream, rate: src_rate, input: Default::default(), output: Default::default() })
    }

    fn run(&mut self, frames: usize, ratio: f64, pitch: f64, silent: bool, out: &mut Vec<f32>) {
        let want = frames as f64 * ratio;
        let cap = want.ceil() as usize + 2;
        for ch in self.output.iter_mut() {
            ch.resize(cap, 0.0);
        }
        let input = if silent { None } else { Some(&self.input[..]) };
        let got = self.stream.process(input, &mut self.output, frames, want.max(1e-3), pitch).min(cap);
        for i in 0..got {
            out.push(self.output[0][i]);
            out.push(self.output[1][i]);
        }
    }
}

impl PushEngine for BungeeEngine {
    fn push(&mut self, input: &[f32], ratio: f64, pitch: f64, out: &mut Vec<f32>) {
        for chunk in input.chunks(BUNGEE_MAX_INPUT * 2) {
            let n = chunk.len() / 2;
            for (c, ch) in self.input.iter_mut().enumerate() {
                ch.clear();
                ch.extend(chunk.chunks_exact(2).map(|f| f[c]));
            }
            self.run(n, ratio, pitch, false, out);
        }
    }
    fn drain(&mut self, ratio: f64, pitch: f64, out: &mut Vec<f32>) {
        // Push silence through for the engine's latency.
        let mut left = self.stream.latency().ceil() as usize;
        while left > 0 {
            let n = left.min(BUNGEE_MAX_INPUT);
            self.run(n, ratio, pitch, true, out);
            left -= n;
        }
    }
    fn reset(&mut self) {
        if let Some(fresh) = BungeeEngine::new(self.rate) {
            *self = fresh;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each engine produces exactly the requested frames and keeps the tone's
    /// pitch when only the tempo changes.
    #[test]
    fn engines_stretch_a_tone() {
        let sr = 44100;
        let tone: Vec<f32> = (0..sr * 4)
            .flat_map(|i| {
                let v = (i as f32 / sr as f32 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
                [v, v]
            })
            .collect();
        for engine in [Engine::Signalsmith, Engine::SoundTouch, Engine::Bungee] {
            let mut s = create(engine, sr as u32);
            s.set_pitch(1.0);
            let ratio = 1.0 / 0.8; // slow down to 80 %
            let mut out = vec![];
            let mut acc = 0.0;
            for block in tone.chunks(512 * 2) {
                acc += (block.len() / 2) as f64 * ratio;
                let n = acc.floor() as usize;
                acc -= n as f64;
                let mut o = vec![0.0f32; n * 2];
                s.process(block, &mut o);
                out.extend(o);
            }
            let expect = (tone.len() / 2) as f64 * ratio;
            assert!(((out.len() / 2) as f64 - expect).abs() < 2.0, "{engine:?}: {} frames", out.len() / 2);
            // Estimate frequency from zero crossings in the middle of the output.
            let mid = &out[out.len() / 4..out.len() * 3 / 4];
            let left: Vec<f32> = mid.chunks_exact(2).map(|f| f[0]).collect();
            let crossings = left.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
            let hz = crossings as f64 / (left.len() as f64 / sr as f64);
            assert!((hz - 440.0).abs() < 8.0, "{engine:?}: {hz:.1} Hz");
            assert!(s.latency_src_frames() >= 0.0);
        }
    }
}

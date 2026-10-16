//! Audio sources feeding a deck.

use crate::decode::LocalDecoder;
use anyhow::Result;
use std::path::Path;

pub trait Source: Send {
    fn sample_rate(&self) -> u32;
    fn duration_secs(&self) -> Option<f64>;
    /// Appends interleaved stereo frames to `out`; returns `false` at end of stream.
    fn read(&mut self, out: &mut Vec<f32>) -> Result<bool>;
    fn seek(&mut self, secs: f64) -> Result<()>;
    /// Position (seconds) of the next frame `read` will return.
    fn position_secs(&self) -> f64;
}

pub struct LocalSource {
    dec: LocalDecoder,
}

impl LocalSource {
    pub fn open(path: &Path) -> Result<Self> {
        Ok(Self { dec: LocalDecoder::open(path)? })
    }
}

impl Source for LocalSource {
    fn sample_rate(&self) -> u32 {
        self.dec.sample_rate
    }
    fn duration_secs(&self) -> Option<f64> {
        self.dec.duration_secs
    }
    fn read(&mut self, out: &mut Vec<f32>) -> Result<bool> {
        self.dec.read(out)
    }
    fn seek(&mut self, secs: f64) -> Result<()> {
        self.dec.seek(secs)
    }
    fn position_secs(&self) -> f64 {
        self.dec.position_secs()
    }
}

/// A generated test tone (used by the "test output" button).
pub struct ToneSource {
    pos: u64,
    rate: u32,
    len: u64,
}

impl ToneSource {
    pub fn new(secs: f64) -> Self {
        Self { pos: 0, rate: 48000, len: (secs * 48000.0) as u64 }
    }
}

impl Source for ToneSource {
    fn sample_rate(&self) -> u32 {
        self.rate
    }
    fn duration_secs(&self) -> Option<f64> {
        Some(self.len as f64 / self.rate as f64)
    }
    fn read(&mut self, out: &mut Vec<f32>) -> Result<bool> {
        if self.pos >= self.len {
            return Ok(false);
        }
        for _ in 0..1024 {
            let t = self.pos as f64 / self.rate as f64;
            // 440 Hz with a 2 Hz pulse so it's clearly a test signal.
            let env = if (t * 2.0).fract() < 0.5 { 0.25 } else { 0.0 };
            let s = ((t * 440.0 * std::f64::consts::TAU).sin() * env) as f32;
            out.push(s);
            out.push(s);
            self.pos += 1;
        }
        Ok(true)
    }
    fn seek(&mut self, secs: f64) -> Result<()> {
        self.pos = (secs * self.rate as f64) as u64;
        Ok(())
    }
    fn position_secs(&self) -> f64 {
        self.pos as f64 / self.rate as f64
    }
}

/// Plays a recording with its separated stems so each stem can be muted.
///
/// Output = original − Σ (1 − gainᵢ) · stemᵢ, where "other" is the original
/// minus vocals, drums and bass. With every stem on this is the untouched
/// original; muting a stem removes just that part. Gains glide over ~20 ms to
/// avoid clicks. The stems must be sample-aligned with the original (see
/// `stems.rs`).
pub struct StemSource {
    main: Box<dyn Source>,
    stems: Vec<LocalDecoder>,
    gains: std::sync::Arc<[super::AtomicF32; 4]>,
    current: [f32; 4],
    main_buf: Vec<f32>,
    stem_bufs: [Vec<f32>; 3],
    stem_eof: [bool; 3],
    main_eof: bool,
}

impl StemSource {
    pub fn open(main: Box<dyn Source>, stem_paths: &[std::path::PathBuf; 3], gains: std::sync::Arc<[super::AtomicF32; 4]>) -> Result<Self> {
        let stems = stem_paths.iter().map(|p| LocalDecoder::open(p)).collect::<Result<Vec<_>>>()?;
        if stems.iter().any(|s| s.sample_rate != main.sample_rate()) {
            anyhow::bail!("stems don't match the song's sample rate; prepare them again");
        }
        let current = std::array::from_fn(|i| gains[i].get());
        Ok(Self {
            main,
            stems,
            gains,
            current,
            main_buf: Vec::new(),
            stem_bufs: Default::default(),
            stem_eof: [false; 3],
            main_eof: false,
        })
    }
}

impl Source for StemSource {
    fn sample_rate(&self) -> u32 {
        self.main.sample_rate()
    }
    fn duration_secs(&self) -> Option<f64> {
        self.main.duration_secs()
    }
    fn read(&mut self, out: &mut Vec<f32>) -> Result<bool> {
        if !self.main_eof && !self.main.read(&mut self.main_buf)? {
            self.main_eof = true;
        }
        if self.main_buf.is_empty() {
            return Ok(!self.main_eof);
        }
        for (i, s) in self.stems.iter_mut().enumerate() {
            while self.stem_bufs[i].len() < self.main_buf.len() && !self.stem_eof[i] {
                if !s.read(&mut self.stem_bufs[i])? {
                    self.stem_eof[i] = true;
                }
            }
        }
        let n = self.main_buf.len();
        let step = 1.0 / (0.02 * self.sample_rate() as f32);
        let targets: [f32; 4] = std::array::from_fn(|i| self.gains[i].get());
        out.reserve(n);
        for f in 0..n / 2 {
            let orig = [self.main_buf[f * 2], self.main_buf[f * 2 + 1]];
            let mut stem = [[0.0f32; 2]; 4];
            for i in 0..3 {
                for ch in 0..2 {
                    stem[i][ch] = self.stem_bufs[i].get(f * 2 + ch).copied().unwrap_or(0.0);
                }
            }
            for ch in 0..2 {
                stem[3][ch] = orig[ch] - stem[0][ch] - stem[1][ch] - stem[2][ch];
            }
            let mut o = orig;
            for i in 0..4 {
                let c = &mut self.current[i];
                *c += (targets[i] - *c).clamp(-step, step);
                let m = 1.0 - *c;
                if m > 1e-6 {
                    o[0] -= m * stem[i][0];
                    o[1] -= m * stem[i][1];
                }
            }
            out.push(o[0]);
            out.push(o[1]);
        }
        self.main_buf.clear();
        for b in self.stem_bufs.iter_mut() {
            let k = n.min(b.len());
            b.drain(..k);
        }
        Ok(true)
    }
    fn seek(&mut self, secs: f64) -> Result<()> {
        // Every decoder seeks sample-accurately to the same frame, keeping the
        // stems aligned with the original.
        self.main.seek(secs)?;
        for s in self.stems.iter_mut() {
            s.seek(secs)?;
        }
        self.main_buf.clear();
        for b in self.stem_bufs.iter_mut() {
            b.clear();
        }
        self.stem_eof = [false; 3];
        self.main_eof = false;
        Ok(())
    }
    fn position_secs(&self) -> f64 {
        self.main.position_secs()
    }
}

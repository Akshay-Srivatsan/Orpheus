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

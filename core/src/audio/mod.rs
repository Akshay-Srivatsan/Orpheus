//! Audio engine.
//!
//! Two identical decks. The crossfader sets each deck's master (room) gain; the
//! deck the room can't hear is cued in the headphone output (PFL).
//!
//! Each deck runs a worker thread: source → time-stretch/SRC → gain/fade → lock-free
//! ring buffers ("taps"). Output streams (one per physical device) mix taps
//! into channel pairs. Play/pause is applied in the output callback with a short
//! ramp so it is instant and click-free.

pub mod announcer;
pub mod deck;
pub mod output;
pub mod priority;
pub mod compressor;
pub mod stretch;
pub mod source;

use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

pub use deck::{Deck, DeckEvent, DeckSnapshot, LoadRequest};

#[derive(Default)]
pub struct AtomicF64(AtomicU64);

impl AtomicF64 {
    pub fn new(v: f64) -> Self {
        Self(AtomicU64::new(v.to_bits()))
    }
    pub fn get(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::Relaxed))
    }
    pub fn set(&self, v: f64) {
        self.0.store(v.to_bits(), Ordering::Relaxed)
    }
}

pub struct AtomicF32(AtomicU32);

impl AtomicF32 {
    pub fn new(v: f32) -> Self {
        Self(AtomicU32::new(v.to_bits()))
    }
    pub fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
    pub fn set(&self, v: f32) {
        self.0.store(v.to_bits(), Ordering::Relaxed)
    }
}

/// Consumer side of a deck → output connection.
pub struct TapRing {
    pub consumer: rtrb::Consumer<f32>,
    /// Pause/resume ramp, 0..1.
    pub ramp: f32,
    /// Linear resampler state (used when the device rate differs from the
    /// engine rate, e.g. a headphone device that can't run at the master rate).
    pub frac: f64,
    pub prev: [f32; 2],
    pub cur: [f32; 2],
    /// Smoothed routing gain (avoids clicks when a deck goes on/off air).
    pub gain: f32,
    /// This copy doesn't pace the deck (it's on another device than the
    /// clock): its consumption is nudged to keep the ring half full, which
    /// absorbs the drift between the two devices' clocks.
    pub follower: bool,
    /// Current drift correction (playback-rate factor - 1).
    pub drift: f64,
}

impl TapRing {
    pub fn new(consumer: rtrb::Consumer<f32>, follower: bool) -> Self {
        Self { consumer, ramp: 0.0, frac: 1.0, prev: [0.0; 2], cur: [0.0; 2], gain: 0.0, follower, drift: 0.0 }
    }
}

pub struct Tap {
    pub ring: Mutex<Option<TapRing>>,
    /// Shared with the deck: whether audio should flow.
    pub playing: Arc<AtomicBool>,
    /// Extra gain for this tap (e.g. PFL on/off for the main deck's cue tap).
    pub gain: AtomicF32,
    /// Rate (Hz) the deck renders at.
    pub rate: AtomicU32,
    /// Frames the output had to fill with silence while playing (diagnostics).
    pub underruns: AtomicU64,
    /// The deck's volume (linear), applied here so changes are immediate.
    pub volume: AtomicF32,
    /// Peak of the deck's signal after its volume, before routing (for the
    /// deck's meter); the poller decays it.
    pub peak: AtomicF32,
    /// Frames the deck couldn't write because this (follower) ring was full
    /// (diagnostics: heard as dropouts on that output).
    pub dropped: AtomicU64,
    /// An open output stream consumes this tap (e.g. false for the headphone
    /// taps when no headphone output is set up).
    pub attached: AtomicBool,
}

impl Tap {
    pub fn new(playing: Arc<AtomicBool>, gain: f32) -> Arc<Self> {
        Arc::new(Self {
            ring: Mutex::new(None),
            playing,
            gain: AtomicF32::new(gain),
            rate: AtomicU32::new(48000),
            underruns: AtomicU64::new(0),
            volume: AtomicF32::new(1.0),
            peak: AtomicF32::new(0.0),
            dropped: AtomicU64::new(0),
            attached: AtomicBool::new(false),
        })
    }
}

/// Global levels applied in the output callbacks.
pub struct Levels {
    pub master: AtomicF32,
    pub cue: AtomicF32,
    /// Peak meters (written by callbacks, decayed by the UI poller).
    pub master_peak: AtomicF32,
    pub cue_peak: AtomicF32,
    /// Output callback timing: [room, headphones].
    pub timing: [CallbackTiming; 2],
}

/// How punctual an output's callbacks are (diagnostics; the poller reads and
/// resets it). A late or slow callback is heard as a crackle even though no
/// deck ran out of audio.
#[derive(Default)]
pub struct CallbackTiming {
    /// Callbacks that came much later than the device buffer's period.
    pub late: AtomicU64,
    /// Callbacks that took longer than the audio they produced.
    pub slow: AtomicU64,
    /// Longest gap between two callbacks, and longest callback (µs).
    pub max_gap_us: AtomicU64,
    pub max_busy_us: AtomicU64,
    /// Device period (µs) of the last callback, for the log message.
    pub period_us: AtomicU64,
}

impl CallbackTiming {
    /// Records one callback that started `gap` after the previous one, took
    /// `busy`, and produced `period` of audio.
    pub fn record(&self, gap: Option<std::time::Duration>, busy: std::time::Duration, period: std::time::Duration) {
        let us = |d: std::time::Duration| d.as_micros() as u64;
        self.period_us.store(us(period), Ordering::Relaxed);
        if let Some(gap) = gap {
            self.max_gap_us.fetch_max(us(gap), Ordering::Relaxed);
            if gap > period * LATE_FACTOR {
                self.late.fetch_add(1, Ordering::Relaxed);
            }
        }
        self.max_busy_us.fetch_max(us(busy), Ordering::Relaxed);
        if busy > period {
            self.slow.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// A callback this many periods after the previous one counts as late.
const LATE_FACTOR: u32 = 2;

impl Default for Levels {
    fn default() -> Self {
        Self {
            master: AtomicF32::new(1.0),
            cue: AtomicF32::new(0.8),
            master_peak: AtomicF32::new(0.0),
            cue_peak: AtomicF32::new(0.0),
            timing: Default::default(),
        }
    }
}

pub fn db_to_gain(db: f64) -> f32 {
    10f64.powf(db / 20.0) as f32
}

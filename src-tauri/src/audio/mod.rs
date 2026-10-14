//! Audio engine.
//!
//! Two identical decks. The crossfader sets each deck's master (room) gain; the
//! deck the room can't hear is cued in the headphone output (PFL).
//!
//! Each deck runs a worker thread: source → time-stretch/SRC → gain/fade → lock-free
//! ring buffers ("taps"). Output streams (one per physical device) mix taps
//! into channel pairs. Play/pause is applied in the output callback with a short
//! ramp so it is instant and click-free.

pub mod deck;
pub mod output;
pub mod priority;
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
}

impl Tap {
    pub fn new(playing: Arc<AtomicBool>, gain: f32) -> Arc<Self> {
        Arc::new(Self { ring: Mutex::new(None), playing, gain: AtomicF32::new(gain), rate: AtomicU32::new(48000), underruns: AtomicU64::new(0) })
    }
}

/// Global levels applied in the output callbacks.
pub struct Levels {
    pub master: AtomicF32,
    pub cue: AtomicF32,
    /// Peak meters (written by callbacks, decayed by the UI poller).
    pub master_peak: AtomicF32,
    pub cue_peak: AtomicF32,
}

impl Default for Levels {
    fn default() -> Self {
        Self {
            master: AtomicF32::new(1.0),
            cue: AtomicF32::new(0.8),
            master_peak: AtomicF32::new(0.0),
            cue_peak: AtomicF32::new(0.0),
        }
    }
}

pub fn db_to_gain(db: f64) -> f32 {
    10f64.powf(db / 20.0) as f32
}

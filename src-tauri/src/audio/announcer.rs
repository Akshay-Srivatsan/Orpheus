//! Plays spoken announcements (see `tts.rs`) to the room and the headphones,
//! through the same mixer as the decks. A clip is pushed into fresh ring
//! buffers in one go; the output callback resamples it to the device rate.

use super::{Tap, TapRing};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct Announcer {
    pub room: Arc<Tap>,
    pub phones: Arc<Tap>,
    playing: Arc<AtomicBool>,
    /// When the current clip ends.
    until: Mutex<Option<Instant>>,
}

impl Default for Announcer {
    fn default() -> Self {
        let playing = Arc::new(AtomicBool::new(false));
        Self {
            room: Tap::new(playing.clone(), 1.0),
            phones: Tap::new(playing.clone(), 1.0),
            playing,
            until: Mutex::new(None),
        }
    }
}

impl Announcer {
    /// Plays mono `samples` at `rate`; returns the clip's length.
    pub fn play(&self, samples: &[f32], rate: u32, volume: f32) -> Duration {
        self.stop();
        // Room for the whole clip plus the resampler's lookahead.
        let frames = samples.len() + rate as usize / 10;
        for tap in [&self.room, &self.phones] {
            let (mut p, c) = rtrb::RingBuffer::<f32>::new(frames * 2);
            for &s in samples {
                let _ = p.push(s);
                let _ = p.push(s);
            }
            tap.rate.store(rate, Ordering::Relaxed);
            tap.volume.set(volume);
            let mut ring = TapRing::new(c, false);
            // Start at full level: the clip has its own silence at the start.
            ring.ramp = 1.0;
            ring.gain = 1.0;
            *tap.ring.lock() = Some(ring);
        }
        let len = Duration::from_secs_f64(samples.len() as f64 / rate.max(1) as f64);
        log::info!("announcer: playing {:.1} s", len.as_secs_f64());
        *self.until.lock() = Some(Instant::now() + len);
        self.playing.store(true, Ordering::SeqCst);
        len
    }

    pub fn stop(&self) {
        self.playing.store(false, Ordering::SeqCst);
        *self.until.lock() = None;
        for tap in [&self.room, &self.phones] {
            *tap.ring.lock() = None;
        }
    }

    /// Still speaking. Ends the clip once its time is up (so the output
    /// doesn't count the empty ring as an underrun).
    pub fn speaking(&self) -> bool {
        let until = *self.until.lock();
        match until {
            Some(t) if Instant::now() < t + Duration::from_millis(150) => true,
            Some(_) => {
                self.stop();
                false
            }
            None => false,
        }
    }
}

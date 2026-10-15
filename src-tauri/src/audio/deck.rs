//! A deck: one loaded track, its worker thread, and its output taps.

use super::source::Source;
use super::{db_to_gain, AtomicF64, Tap};
use anyhow::Result;
use crossbeam_channel::{unbounded, Receiver, RecvTimeoutError, Sender};
use parking_lot::Mutex;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub type SourceOpener = Box<dyn FnOnce() -> Result<Box<dyn Source>> + Send>;

pub struct LoadRequest {
    pub track_id: Option<String>,
    pub version_id: Option<String>,
    pub entry_id: Option<String>,
    pub open: SourceOpener,
    pub start_secs: f64,
    /// Stop reading here (cue-out / trimmed silence).
    pub end_secs: Option<f64>,
    /// Start an automatic fade-out here.
    pub fade_at_secs: Option<f64>,
    pub fade_secs: f64,
    pub tempo: f64,
    pub gain_db: f64,
    pub play: bool,
    /// Main cue point (defaults to `start_secs`).
    pub cue_secs: Option<f64>,
    /// The source mixes separated stems (gains in `DeckShared::stem_gains`).
    pub has_stems: bool,
    /// Initial on/off state of each stem (`stems::STEMS` order).
    pub stems_on: [bool; 4],
}

enum Cmd {
    Load(LoadRequest),
    Seek(f64),
    FadeOut(f64),
    CancelFade,
    SetFadeAt(Option<f64>),
    Unload,
    Reconfigure,
}

#[derive(Debug, Clone)]
pub enum DeckEvent {
    Loaded { deck: usize, track_id: Option<String> },
    /// Playback reached the end (or a fade-out completed).
    Ended { deck: usize, track_id: Option<String>, entry_id: Option<String> },
    Error { deck: usize, track_id: Option<String>, message: String },
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DeckSnapshot {
    pub deck: usize,
    /// `empty`, `loading`, `paused`, `playing`, `ended`, `error`.
    pub status: String,
    pub track_id: Option<String>,
    pub version_id: Option<String>,
    pub entry_id: Option<String>,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub start_ms: i64,
    pub end_ms: Option<i64>,
    pub fade_at_ms: Option<i64>,
    /// Main cue point (CUE button).
    pub cue_ms: i64,
    /// This deck plays to the room.
    pub on_air: bool,
    pub fading: bool,
    /// On/off per stem (`stems::STEMS` order), when the track is playing from stems.
    pub stems: Option<[bool; 4]>,
    pub tempo: f64,
    pub keylock: bool,
    pub gain_db: f64,
    pub trim_db: f64,
    pub pfl: bool,
    pub error: Option<String>,
}

#[derive(Default)]
struct Info {
    status: String,
    track_id: Option<String>,
    version_id: Option<String>,
    entry_id: Option<String>,
    duration: f64,
    start: f64,
    end: Option<f64>,
    fade_at: Option<f64>,
    cue: f64,
    fading: bool,
    error: Option<String>,
}

pub struct DeckShared {
    pub index: usize,
    pub playing: Arc<AtomicBool>,
    info: Mutex<Info>,
    position: AtomicF64,
    pub tempo: AtomicF64,
    pub keylock: AtomicBool,
    /// Time-stretch engine for the next song loaded (`audio::stretch::Engine`).
    pub stretch_engine: Mutex<String>,
    pub trim_db: AtomicF64,
    pub track_gain_db: AtomicF64,
    pub pfl: AtomicBool,
    /// Set by the app: this deck is the one the room hears.
    pub on_air: AtomicBool,
    /// Pace the deck by the headphone output (no master device available).
    pub clock_cue: Arc<AtomicBool>,
    /// Stem gains read by `StemSource` (1 = on, 0 = muted).
    pub stem_gains: Arc<[super::AtomicF32; 4]>,
    pub has_stems: AtomicBool,
    /// Engine output rate (Hz).
    pub rate: Arc<AtomicU32>,
    pub buffer_ms: Arc<AtomicU32>,
    pub master_tap: Arc<Tap>,
    pub cue_tap: Arc<Tap>,
    load_gen: AtomicU64,
}

pub struct Deck {
    pub shared: Arc<DeckShared>,
    tx: Sender<Cmd>,
}

impl Deck {
    pub fn new(
        index: usize,
        rate: Arc<AtomicU32>,
        buffer_ms: Arc<AtomicU32>,
        clock_cue: Arc<AtomicBool>,
        events: Sender<DeckEvent>,
    ) -> Self {
        let playing = Arc::new(AtomicBool::new(false));
        // Both decks feed both outputs. Tap gains (set by the app) decide what
        // is audible: only the on-air deck has a non-zero master gain.
        let shared = Arc::new(DeckShared {
            index,
            playing: playing.clone(),
            info: Mutex::new(Info { status: "empty".into(), ..Default::default() }),
            position: AtomicF64::new(0.0),
            tempo: AtomicF64::new(1.0),
            keylock: AtomicBool::new(true),
            stretch_engine: Mutex::new(String::new()),
            trim_db: AtomicF64::new(0.0),
            track_gain_db: AtomicF64::new(0.0),
            pfl: AtomicBool::new(false),
            on_air: AtomicBool::new(false),
            clock_cue,
            stem_gains: Arc::new(std::array::from_fn(|_| super::AtomicF32::new(1.0))),
            has_stems: AtomicBool::new(false),
            rate,
            buffer_ms,
            master_tap: Tap::new(playing.clone(), 0.0),
            cue_tap: Tap::new(playing, 0.0),
            load_gen: AtomicU64::new(0),
        });
        let (tx, rx) = unbounded();
        let sh = shared.clone();
        std::thread::Builder::new()
            .name(format!("deck-{index}"))
            .spawn(move || Worker::new(sh, rx, events).run())
            .expect("spawn deck thread");
        Self { shared, tx }
    }

    pub fn load(&self, req: LoadRequest) {
        self.shared.playing.store(false, Ordering::SeqCst);
        for (g, on) in self.shared.stem_gains.iter().zip(req.stems_on) {
            g.set(if on { 1.0 } else { 0.0 });
        }
        self.shared.has_stems.store(false, Ordering::SeqCst);
        self.shared.load_gen.fetch_add(1, Ordering::SeqCst);
        {
            let mut i = self.shared.info.lock();
            i.status = "loading".into();
            i.track_id = req.track_id.clone();
            i.version_id = req.version_id.clone();
            i.entry_id = req.entry_id.clone();
            i.error = None;
            i.fading = false;
        }
        let _ = self.tx.send(Cmd::Load(req));
    }

    pub fn play(&self) -> bool {
        let mut i = self.shared.info.lock();
        match i.status.as_str() {
            // "ended": callers queue a seek first; the worker resets on seek.
            "paused" | "ended" => {
                i.status = "playing".into();
                self.shared.playing.store(true, Ordering::SeqCst);
                true
            }
            "playing" => true,
            _ => false,
        }
    }

    pub fn pause(&self) {
        let mut i = self.shared.info.lock();
        if i.status == "playing" {
            i.status = "paused".into();
        }
        self.shared.playing.store(false, Ordering::SeqCst);
    }

    pub fn toggle(&self) {
        if self.status() == "playing" {
            self.pause()
        } else {
            self.play();
        }
    }

    pub fn status(&self) -> String {
        self.shared.info.lock().status.clone()
    }

    pub fn track_id(&self) -> Option<String> {
        self.shared.info.lock().track_id.clone()
    }

    pub fn version_id(&self) -> Option<String> {
        self.shared.info.lock().version_id.clone()
    }

    pub fn entry_id(&self) -> Option<String> {
        self.shared.info.lock().entry_id.clone()
    }

    pub fn set_entry_id(&self, id: Option<String>) {
        self.shared.info.lock().entry_id = id;
    }

    pub fn is_fading(&self) -> bool {
        self.shared.info.lock().fading
    }

    pub fn seek(&self, secs: f64) {
        let _ = self.tx.send(Cmd::Seek(secs));
    }

    pub fn fade_out(&self, secs: f64) {
        let _ = self.tx.send(Cmd::FadeOut(secs));
    }

    pub fn cancel_fade(&self) {
        let _ = self.tx.send(Cmd::CancelFade);
    }

    pub fn set_fade_at(&self, secs: Option<f64>) {
        let _ = self.tx.send(Cmd::SetFadeAt(secs));
    }

    pub fn unload(&self) {
        self.shared.playing.store(false, Ordering::SeqCst);
        let _ = self.tx.send(Cmd::Unload);
    }

    pub fn reconfigure(&self) {
        let _ = self.tx.send(Cmd::Reconfigure);
    }

    pub fn set_tempo(&self, ratio: f64) {
        self.shared.tempo.set(ratio.clamp(0.5, 2.0));
    }

    pub fn tempo(&self) -> f64 {
        self.shared.tempo.get()
    }

    pub fn set_keylock(&self, on: bool) {
        self.shared.keylock.store(on, Ordering::Relaxed);
    }

    /// Time-stretch engine for songs loaded from now on.
    pub fn set_stretch_engine(&self, engine: &str) {
        *self.shared.stretch_engine.lock() = engine.to_string();
    }

    /// Deck volume in dB, applied in the output callback (no buffer delay).
    pub fn set_trim_db(&self, db: f64) {
        let db = db.clamp(TRIM_MIN_DB, TRIM_MAX_DB);
        self.shared.trim_db.set(db);
        let g = db_to_gain(db) as f32;
        self.shared.master_tap.volume.set(g);
        self.shared.cue_tap.volume.set(g);
    }

    /// The deck's current output peak (0..1+), for its meter.
    pub fn peak(&self) -> f32 {
        self.shared.master_tap.peak.get().max(self.shared.cue_tap.peak.get())
    }

    /// Decays the meter peaks (called by the poller after reading them).
    pub fn decay_peak(&self, factor: f32) {
        for t in [&self.shared.master_tap, &self.shared.cue_tap] {
            t.peak.set(t.peak.get() * factor);
        }
    }

    /// Records the headphone-cue state; the app derives tap gains from it.
    pub fn set_pfl(&self, on: bool) {
        self.shared.pfl.store(on, Ordering::Relaxed);
    }

    pub fn pfl(&self) -> bool {
        self.shared.pfl.load(Ordering::Relaxed)
    }

    pub fn is_on_air(&self) -> bool {
        self.shared.on_air.load(Ordering::Relaxed)
    }

    /// Sets the main cue point (seconds).
    pub fn set_cue(&self, secs: f64) {
        self.shared.info.lock().cue = secs.max(0.0);
    }

    pub fn cue_secs(&self) -> f64 {
        self.shared.info.lock().cue
    }

    /// Lets audio flow (or not) without changing the play/pause status; used
    /// for short "scrub" previews while paused.
    pub fn set_audible(&self, on: bool) {
        self.shared.playing.store(on, Ordering::SeqCst);
    }

    /// Changes whenever a new track is loaded.
    pub fn load_generation(&self) -> u64 {
        self.shared.load_gen.load(Ordering::SeqCst)
    }

    /// Turns a stem on or off (`stems::STEMS` order). Returns false if the
    /// loaded track isn't playing from stems.
    pub fn set_stem(&self, index: usize, on: bool) -> bool {
        if !self.shared.has_stems.load(Ordering::SeqCst) || index >= 4 {
            return false;
        }
        self.shared.stem_gains[index].set(if on { 1.0 } else { 0.0 });
        true
    }

    pub fn stem_on(&self, index: usize) -> bool {
        self.shared.stem_gains.get(index).map(|g| g.get() > 0.5).unwrap_or(false)
    }

    pub fn has_stems(&self) -> bool {
        self.shared.has_stems.load(Ordering::SeqCst)
    }

    pub fn loaded(&self) -> bool {
        !matches!(self.status().as_str(), "empty" | "error")
    }

    pub fn position_secs(&self) -> f64 {
        self.shared.position.get()
    }

    pub fn snapshot(&self) -> DeckSnapshot {
        let i = self.shared.info.lock();
        let s = &self.shared;
        DeckSnapshot {
            deck: s.index,
            status: i.status.clone(),
            track_id: i.track_id.clone(),
            version_id: i.version_id.clone(),
            entry_id: i.entry_id.clone(),
            position_ms: (s.position.get() * 1000.0) as i64,
            duration_ms: (i.duration * 1000.0) as i64,
            start_ms: (i.start * 1000.0) as i64,
            end_ms: i.end.map(|e| (e * 1000.0) as i64),
            fade_at_ms: i.fade_at.map(|e| (e * 1000.0) as i64),
            cue_ms: (i.cue * 1000.0) as i64,
            stems: s
                .has_stems
                .load(Ordering::Relaxed)
                .then(|| std::array::from_fn(|k| s.stem_gains[k].get() > 0.5)),
            on_air: s.on_air.load(Ordering::Relaxed),
            fading: i.fading,
            tempo: s.tempo.get(),
            keylock: s.keylock.load(Ordering::Relaxed),
            gain_db: s.track_gain_db.get(),
            trim_db: s.trim_db.get(),
            pfl: s.pfl.load(Ordering::Relaxed),
            error: i.error.clone(),
        }
    }
}

const BLOCK: usize = 512;
/// Range of the deck volume (dB).
pub const TRIM_MIN_DB: f64 = -24.0;
pub const TRIM_MAX_DB: f64 = 12.0;

/// Transparent below 0.9; smoothly limits peaks (time-stretching and gain can
/// push samples past full scale) instead of hard clipping.
#[inline]
fn soft_clip(x: f32) -> f32 {
    const K: f32 = 0.9;
    let a = x.abs();
    if a <= K {
        x
    } else {
        x.signum() * (K + (1.0 - K) * ((a - K) / (1.0 - K)).tanh())
    }
}

struct Worker {
    sh: Arc<DeckShared>,
    rx: Receiver<Cmd>,
    events: Sender<DeckEvent>,
    src: Option<Box<dyn Source>>,
    src_rate: u32,
    stretch: Option<Box<dyn super::stretch::TimeStretch>>,
    transpose: f32,
    /// (producer, is_clock)
    prods: Vec<(rtrb::Producer<f32>, bool)>,
    clock_capacity: usize,
    inbuf: Vec<f32>,
    outbuf: Vec<f32>,
    out_acc: f64,
    eof: bool,
    tail_flushed: bool,
    ended_sent: bool,
    end: Option<f64>,
    fade_at: Option<f64>,
    fade_secs: f64,
    fade_step: Option<f32>,
    fade_gain: f32,
    /// Source position of the next frame to enter the stretcher.
    written_pos: f64,
    track_id: Option<String>,
    gen: u64,
}

impl Worker {
    fn new(sh: Arc<DeckShared>, rx: Receiver<Cmd>, events: Sender<DeckEvent>) -> Self {
        Self {
            sh,
            rx,
            events,
            src: None,
            src_rate: 44100,
            stretch: None,
            transpose: 0.0,
            prods: vec![],
            clock_capacity: 0,
            inbuf: Vec::with_capacity(BLOCK * 4),
            outbuf: vec![],
            out_acc: 0.0,
            eof: false,
            tail_flushed: false,
            ended_sent: false,
            end: None,
            fade_at: None,
            fade_secs: 5.0,
            fade_step: None,
            fade_gain: 1.0,
            written_pos: 0.0,
            track_id: None,
            gen: 0,
        }
    }

    fn active(&self) -> bool {
        self.src.is_some() && !(self.eof && self.tail_flushed && self.ended_sent)
    }

    fn run(mut self) {
        // This thread feeds the outputs: it runs at real-time priority so the
        // interface can never starve it (see `priority.rs`).
        let mut rt = super::priority::RtThread::promote(self.sh.rate.load(Ordering::Relaxed));
        loop {
            let active = self.active();
            let timeout = if active { Duration::from_millis(0) } else { Duration::from_millis(50) };
            match self.rx.recv_timeout(timeout) {
                Ok(cmd) => self.handle(cmd, &mut rt),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            if !active {
                rt.did_block();
            }
            while let Ok(cmd) = self.rx.try_recv() {
                self.handle(cmd, &mut rt);
            }
            if self.src.is_some() {
                let produced = if self.eof && self.tail_flushed { false } else { self.produce() };
                self.update_position();
                self.check_end();
                if produced {
                    rt.breathe();
                } else {
                    rt.blocked(Duration::from_millis(3));
                }
            }
        }
    }

    fn make_rings(&mut self) {
        let rate = self.sh.rate.load(Ordering::Relaxed);
        // The ring holds several device buffers' worth so a briefly delayed
        // worker never starves the output (play/pause and volume act in the
        // output callback, and seeks flush the ring, so this adds no lag there).
        let ms = (self.sh.buffer_ms.load(Ordering::Relaxed) * 4).clamp(250, 1000);
        let frames = (rate as u64 * ms as u64 / 1000) as usize;
        self.prods.clear();
        // The master output paces playback; the headphone copy follows (and
        // drops samples if that device runs slightly fast or slow).
        let cue_clock = self.sh.clock_cue.load(Ordering::Relaxed);
        let taps: Vec<(Arc<Tap>, bool)> =
            vec![(self.sh.master_tap.clone(), !cue_clock), (self.sh.cue_tap.clone(), cue_clock)];
        for (tap, clock) in taps {
            let (p, c) = rtrb::RingBuffer::<f32>::new(frames * 2);
            tap.rate.store(rate, Ordering::Relaxed);
            *tap.ring.lock() = Some(super::TapRing::new(c, !clock));
            self.prods.push((p, clock));
        }
        self.clock_capacity = frames * 2;
    }

    fn clear_rings(&mut self) {
        self.prods.clear();
        *self.sh.master_tap.ring.lock() = None;
        *self.sh.cue_tap.ring.lock() = None;
    }

    fn handle(&mut self, cmd: Cmd, rt: &mut super::priority::RtThread) {
        match cmd {
            // Opening a song can take a while; don't hold real-time priority for it.
            Cmd::Load(req) => rt.normal(|| self.load(req)),
            Cmd::Seek(secs) => {
                if self.src.is_some() {
                    self.seek(secs);
                }
            }
            Cmd::FadeOut(secs) => {
                if self.src.is_some() && self.fade_step.is_none() && !self.eof {
                    let rate = self.sh.rate.load(Ordering::Relaxed) as f64;
                    self.fade_step = Some((1.0 / (secs.max(0.05) * rate)) as f32);
                    self.sh.info.lock().fading = true;
                }
            }
            Cmd::CancelFade => {
                self.fade_step = None;
                self.fade_gain = 1.0;
                self.sh.info.lock().fading = false;
            }
            Cmd::SetFadeAt(at) => {
                self.fade_at = at;
                self.sh.info.lock().fade_at = at;
            }
            Cmd::Unload => {
                self.src = None;
                self.stretch = None;
                self.clear_rings();
                self.track_id = None;
                self.sh.has_stems.store(false, Ordering::SeqCst);
                let mut i = self.sh.info.lock();
                *i = Info { status: "empty".into(), ..Default::default() };
                self.sh.position.set(0.0);
            }
            Cmd::Reconfigure => {
                if self.src.is_some() {
                    let pos = self.sh.position.get();
                    self.seek(pos);
                } else {
                    self.clear_rings();
                }
            }
        }
    }

    fn load(&mut self, req: LoadRequest) {
        self.gen = self.sh.load_gen.load(Ordering::SeqCst);
        self.src = None;
        self.clear_rings();
        self.track_id = req.track_id.clone();
        let opened = (req.open)();
        // A newer load was requested while this one was opening.
        if self.sh.load_gen.load(Ordering::SeqCst) != self.gen {
            return;
        }
        let mut src = match opened {
            Ok(s) => s,
            Err(e) => {
                let msg = format!("{e:#}");
                log::error!("deck {} load failed: {msg}", self.sh.index);
                {
                    let mut i = self.sh.info.lock();
                    i.status = "error".into();
                    i.error = Some(msg.clone());
                }
                let _ = self.events.send(DeckEvent::Error { deck: self.sh.index, track_id: req.track_id.clone(), message: msg });
                return;
            }
        };
        if req.start_secs > 0.0 {
            if let Err(e) = src.seek(req.start_secs) {
                log::warn!("initial seek failed: {e:#}");
            }
        }
        self.src_rate = src.sample_rate();
        self.stretch = Some(super::stretch::create(super::stretch::Engine::from_setting(&self.sh.stretch_engine.lock()), self.src_rate));
        self.transpose = 0.0;
        self.written_pos = src.position_secs();
        let duration = src.duration_secs().unwrap_or(0.0);
        self.src = Some(src);
        self.end = req.end_secs.filter(|e| *e > req.start_secs);
        self.fade_at = req.fade_at_secs;
        self.fade_secs = req.fade_secs;
        self.reset_stream_state();
        self.sh.tempo.set(req.tempo.clamp(0.5, 2.0));
        self.sh.track_gain_db.set(req.gain_db);
        self.sh.position.set(self.written_pos);
        self.make_rings();
        {
            let mut i = self.sh.info.lock();
            i.duration = duration;
            i.start = req.start_secs;
            i.cue = req.cue_secs.unwrap_or(req.start_secs);
            i.end = self.end;
            i.fade_at = self.fade_at;
            i.fading = false;
            i.error = None;
            i.status = if req.play { "playing" } else { "paused" }.into();
        }
        self.sh.playing.store(req.play, Ordering::SeqCst);
        self.sh.has_stems.store(req.has_stems, Ordering::SeqCst);
        let _ = self.events.send(DeckEvent::Loaded { deck: self.sh.index, track_id: req.track_id.clone() });
    }

    fn reset_stream_state(&mut self) {
        self.inbuf.clear();
        self.out_acc = 0.0;
        self.eof = false;
        self.tail_flushed = false;
        self.ended_sent = false;
        self.fade_step = None;
        self.fade_gain = 1.0;
        if let Some(s) = self.stretch.as_mut() {
            s.reset();
        }
    }

    fn seek(&mut self, secs: f64) {
        let Some(src) = self.src.as_mut() else { return };
        let max = self.end.or(src.duration_secs()).unwrap_or(f64::MAX);
        let secs = secs.clamp(0.0, (max - 0.05).max(0.0));
        if let Err(e) = src.seek(secs) {
            log::warn!("seek failed: {e:#}");
        }
        self.written_pos = src.position_secs();
        self.reset_stream_state();
        self.make_rings();
        self.sh.position.set(self.written_pos);
        let mut i = self.sh.info.lock();
        i.fading = false;
        if i.status == "ended" {
            i.status = "paused".into();
        }
    }

    fn clock_free_frames(&self) -> usize {
        self.prods.iter().find(|(_, c)| *c).map(|(p, _)| p.slots() / 2).unwrap_or(0)
    }

    fn clock_buffered_frames(&self) -> usize {
        self.prods
            .iter()
            .find(|(_, c)| *c)
            .map(|(p, _)| (self.clock_capacity - p.slots()) / 2)
            .unwrap_or(0)
    }

    /// Produces one block of output. Returns false if there was no room.
    fn produce(&mut self) -> bool {
        let out_rate = self.sh.rate.load(Ordering::Relaxed) as f64;
        let tempo = self.sh.tempo.get();
        let keylock = self.sh.keylock.load(Ordering::Relaxed);
        let ratio = (out_rate / self.src_rate as f64) / tempo;
        let expected_out = (BLOCK as f64 * ratio).ceil() as usize + 2;
        if self.clock_free_frames() < expected_out {
            return false;
        }
        // Fill the input block.
        while self.inbuf.len() < BLOCK * 2 && !self.eof {
            let src = self.src.as_mut().unwrap();
            match src.read(&mut self.inbuf) {
                Ok(true) => {}
                Ok(false) => self.eof = true,
                Err(e) => {
                    log::warn!("deck {} read error: {e:#}", self.sh.index);
                    self.eof = true;
                }
            }
        }
        let mut n = (self.inbuf.len() / 2).min(BLOCK);
        if let Some(end) = self.end {
            let remaining = ((end - self.written_pos) * self.src_rate as f64).max(0.0) as usize;
            if remaining < n {
                n = remaining;
                self.eof = true;
                self.inbuf.truncate(n * 2);
            }
        }
        let stretch = self.stretch.as_mut().unwrap();
        let transpose = if keylock {
            (self.src_rate as f64 / out_rate) as f32
        } else {
            (self.src_rate as f64 / out_rate * tempo) as f32
        };
        if (transpose - self.transpose).abs() > 1e-6 {
            stretch.set_pitch(transpose);
            self.transpose = transpose;
        }
        if n == 0 {
            if self.eof && !self.tail_flushed {
                stretch.flush(&mut self.outbuf);
                self.tail_flushed = true;
                let mut out = std::mem::take(&mut self.outbuf);
                self.finish_block(&mut out);
                self.outbuf = out;
            }
            return true;
        }
        self.out_acc += n as f64 * ratio;
        let out_n = self.out_acc.floor() as usize;
        self.out_acc -= out_n as f64;
        self.outbuf.resize(out_n * 2, 0.0);
        stretch.process(&self.inbuf[..n * 2], &mut self.outbuf);
        self.inbuf.drain(..n * 2);
        self.written_pos += n as f64 / self.src_rate as f64;

        if let (Some(at), None) = (self.fade_at, self.fade_step) {
            if self.written_pos >= at {
                self.fade_step = Some((1.0 / (self.fade_secs.max(0.05) * out_rate)) as f32);
                self.sh.info.lock().fading = true;
            }
        }
        let mut out = std::mem::take(&mut self.outbuf);
        self.finish_block(&mut out);
        self.outbuf = out;
        true
    }

    /// Applies gain/fade and writes to the rings.
    fn finish_block(&mut self, out: &mut [f32]) {
        // The deck volume (trim) is applied in the output callback.
        let g = db_to_gain(self.sh.track_gain_db.get());
        let mut fade_done = false;
        for fr in out.chunks_exact_mut(2) {
            if let Some(step) = self.fade_step {
                self.fade_gain = (self.fade_gain - step).max(0.0);
                if self.fade_gain <= 0.0 {
                    fade_done = true;
                }
            }
            // Equal-power-ish curve for the fade.
            let fg = self.fade_gain * self.fade_gain;
            fr[0] = soft_clip(fr[0] * g * fg);
            fr[1] = soft_clip(fr[1] * g * fg);
        }
        for (i, (p, clock)) in self.prods.iter_mut().enumerate() {
            let n = if *clock { out.len() } else { (p.slots() / 2 * 2).min(out.len()) };
            if n < out.len() {
                let tap = if i == 0 { &self.sh.master_tap } else { &self.sh.cue_tap };
                tap.dropped.fetch_add(((out.len() - n) / 2) as u64, Ordering::Relaxed);
            }
            for &s in &out[..n] {
                if p.push(s).is_err() {
                    break;
                }
            }
        }
        if fade_done {
            self.eof = true;
            self.tail_flushed = true;
        }
    }

    fn update_position(&mut self) {
        let out_rate = self.sh.rate.load(Ordering::Relaxed) as f64;
        let tempo = self.sh.tempo.get();
        let buffered = self.clock_buffered_frames() as f64 / out_rate * tempo;
        let latency = self
            .stretch
            .as_ref()
            .map(|s| s.latency_src_frames() / self.src_rate as f64)
            .unwrap_or(0.0);
        let start = self.sh.info.lock().start;
        let pos = (self.written_pos - buffered - latency).max(start.min(self.written_pos));
        self.sh.position.set(pos);
    }

    fn check_end(&mut self) {
        if self.eof && self.tail_flushed && !self.ended_sent && self.clock_buffered_frames() == 0 {
            self.ended_sent = true;
            self.sh.playing.store(false, Ordering::SeqCst);
            let entry_id = {
                let mut i = self.sh.info.lock();
                i.status = "ended".into();
                i.fading = false;
                i.entry_id.clone()
            };
            let _ = self.events.send(DeckEvent::Ended { deck: self.sh.index, track_id: self.track_id.clone(), entry_id });
        }
    }
}

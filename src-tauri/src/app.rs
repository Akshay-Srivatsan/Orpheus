//! Application state and performance logic.

use crate::audio::output::{OutputConfig, OutputStatus, Outputs};
use crate::audio::source::{LocalSource, Source, ToneSource};
use crate::audio::{Deck, DeckEvent, DeckSnapshot, Levels, LoadRequest};
use crate::store::Store;
use crate::midi::{self, Midi, Value};
use crate::model::*;
use crate::settings::{ProfileCore, ProjectorSettings, Settings, BUILTIN_PROFILES};
use crate::spotify::{Spotify, SpotifySource};
use anyhow::{anyhow, Result};
use crossbeam_channel::{unbounded, Receiver, Sender};
use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

pub const MAIN: usize = 0;
pub const PREVIEW: usize = 1;

#[derive(Default)]
pub struct Perform {
    pub setlist_id: Option<Id>,
    /// Epoch ms when the main deck stopped (the pause between songs).
    pub gap_started: Option<i64>,
    /// Entry to start once the current fade-out completes.
    pub pending: Option<PendingNext>,
    /// "Play Next" was pressed once while a song was playing; press again to confirm.
    pub armed_until: Option<Instant>,
    pub auto_advance_override: Option<bool>,
    pub message: Option<String>,
}

#[derive(Clone, Debug)]
pub enum PendingNext {
    Next,
    Entry(Id),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformState {
    pub setlist_id: Option<Id>,
    pub mode: String,
    pub current_entry_id: Option<Id>,
    pub next_entry_id: Option<Id>,
    pub gap_started_at: Option<i64>,
    pub gap_seconds: f64,
    pub auto_advance: bool,
    pub auto_advance_at: Option<i64>,
    pub pending_next: bool,
    pub armed: bool,
    pub message: Option<String>,
    pub started_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjTag {
    pub id: String,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjEntry {
    pub id: Id,
    pub title: String,
    pub artist: String,
    pub style: Option<String>,
    pub style_name: String,
    pub style_short: String,
    pub color: String,
    pub tags: Vec<ProjTag>,
    pub status: String,
    pub bpm: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectorPayload {
    pub title: String,
    pub mode: String,
    pub settings: ProjectorSettings,
    pub current: Option<ProjEntry>,
    pub next: Option<ProjEntry>,
    pub history: Vec<ProjEntry>,
    pub entries: Vec<ProjEntry>,
    pub current_index: Option<usize>,
    pub in_gap: bool,
    pub playing: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelsSnapshot {
    pub master: f32,
    pub cue: f32,
    pub master_volume: f32,
    pub cue_volume: f32,
}

#[derive(Default)]
struct Takeover {
    engaged: HashSet<String>,
    last_hw: HashMap<String, f64>,
}

pub struct App {
    pub handle: AppHandle,
    pub store: Arc<Store>,
    pub local_dir: PathBuf,
    pub data_dir: PathBuf,
    pub settings: RwLock<Settings>,
    pub decks: [Deck; 2],
    pub levels: Arc<Levels>,
    pub outputs: Outputs,
    pub rate: Arc<AtomicU32>,
    pub buffer_ms: Arc<AtomicU32>,
    pub spotify: Arc<Spotify>,
    pub midi: OnceLock<Arc<Midi>>,
    pub perform: Mutex<Perform>,
    pub scan_progress: Mutex<crate::library::ScanProgress>,
    analysis_tx: Sender<(Id, bool)>,
    takeover: Mutex<Takeover>,
    pending_seek: Mutex<Option<f64>>,
    _rt: tokio::runtime::Runtime,
}

pub fn emit<S: Serialize + Clone>(h: &AppHandle, ev: &str, payload: S) {
    if let Err(e) = h.emit(ev, payload) {
        log::debug!("emit {ev}: {e}");
    }
}

/// Folds `bpm` by octaves to be as close as possible to `target`.
pub fn fold_bpm(bpm: f64, target: f64) -> f64 {
    let mut b = bpm;
    if b <= 0.0 || target <= 0.0 {
        return bpm;
    }
    while b < target / std::f64::consts::SQRT_2 {
        b *= 2.0;
    }
    while b > target * std::f64::consts::SQRT_2 {
        b /= 2.0;
    }
    b
}

impl App {
    pub fn new(handle: AppHandle, data_dir: PathBuf) -> Result<Arc<Self>> {
        std::fs::create_dir_all(&data_dir)?;
        let settings: Settings = std::fs::read_to_string(data_dir.join("settings.json"))
            .ok()
            .and_then(|s| match serde_json::from_str(&s) {
                Ok(v) => Some(v),
                Err(e) => {
                    log::warn!("settings.json is invalid ({e}); using defaults");
                    None
                }
            })
            .unwrap_or_default();
        let library_dir = settings.library_path(&data_dir);
        let store = Arc::new(Store::open(&library_dir, &data_dir)?);
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
        let spotify = Arc::new(Spotify::new(rt.handle().clone(), data_dir.join("spotify")));
        let levels = Arc::new(Levels::default());
        let rate = Arc::new(AtomicU32::new(48000));
        let buffer_ms = Arc::new(AtomicU32::new(settings.audio.buffer_ms));
        let (deck_tx, deck_rx) = unbounded();
        let decks = [
            Deck::new(MAIN, rate.clone(), buffer_ms.clone(), deck_tx.clone()),
            Deck::new(PREVIEW, rate.clone(), buffer_ms.clone(), deck_tx),
        ];
        for d in &decks {
            d.set_keylock(settings.keylock);
        }
        let outputs = Outputs::new(levels.clone());
        let (analysis_tx, analysis_rx) = unbounded();
        let app = Arc::new(Self {
            handle,
            store,
            local_dir: data_dir.clone(),
            data_dir,
            settings: RwLock::new(settings),
            decks,
            levels,
            outputs,
            rate,
            buffer_ms,
            spotify,
            midi: OnceLock::new(),
            perform: Mutex::new(Perform::default()),
            scan_progress: Mutex::new(Default::default()),
            analysis_tx,
            takeover: Mutex::new(Takeover::default()),
            pending_seek: Mutex::new(None),
            _rt: rt,
        });
        app.apply_outputs();
        app.start_threads(deck_rx, analysis_rx);
        let weak = Arc::downgrade(&app);
        let handler: midi::Handler = Arc::new(move |action, value| {
            if let Some(app) = weak.upgrade() {
                app.on_midi(action, value);
            }
        });
        let (mapping, enabled) = app.current_mapping();
        let _ = app.midi.set(Midi::new(mapping, enabled, handler));
        // Restore the last active setlist.
        if let Ok(id) = std::fs::read_to_string(app.local_dir.join("active-setlist")) {
            let id = id.trim().to_string();
            if app.store.setlist(&id).is_ok() {
                app.perform.lock().setlist_id = Some(id);
            }
        }
        Ok(app)
    }

    pub fn current_mapping(&self) -> (midi::Mapping, bool) {
        let s = self.settings.read();
        let all = self.all_mappings_with(&s);
        let m = all
            .iter()
            .find(|m| m.id == s.midi.mapping_id)
            .cloned()
            .unwrap_or_else(|| all[0].clone());
        (m, s.midi.enabled)
    }

    pub fn all_mappings_with(&self, s: &Settings) -> Vec<midi::Mapping> {
        let mut v = midi::builtin_mappings();
        for m in &s.midi.custom_mappings {
            v.retain(|b| b.id != m.id);
            v.push(m.clone());
        }
        v
    }

    // ------------------------------------------------------------ settings/profiles

    pub fn save_settings(&self, new: Settings) -> Result<()> {
        let old = self.settings.read().clone();
        crate::store::write_json(&self.local_dir.join("settings.json"), &new)?;
        *self.settings.write() = new.clone();
        let (old_lib, new_lib) = (old.library_path(&self.local_dir), new.library_path(&self.local_dir));
        if old_lib != new_lib {
            self.store.reload(&new_lib)?;
            emit(&self.handle, "library-changed", Vec::<Id>::new());
            emit(&self.handle, "setlists-changed", ());
            emit(&self.handle, "crates-changed", ());
        }
        if serde_json::to_string(&old.audio)? != serde_json::to_string(&new.audio)? {
            self.buffer_ms.store(new.audio.buffer_ms, Ordering::Relaxed);
            self.apply_outputs();
        }
        if old.keylock != new.keylock {
            for d in &self.decks {
                d.set_keylock(new.keylock);
            }
        }
        if serde_json::to_string(&old.midi)? != serde_json::to_string(&new.midi)? {
            let (m, enabled) = self.current_mapping();
            if let Some(midi) = self.midi.get() {
                midi.set_mapping(m, enabled);
            }
        }
        emit(&self.handle, "settings-changed", &new);
        self.emit_perform();
        Ok(())
    }

    pub fn profiles_json(&self) -> Result<Vec<serde_json::Value>> {
        let mut out: Vec<serde_json::Value> = BUILTIN_PROFILES
            .iter()
            .filter_map(|s| serde_json::from_str(s).ok())
            .collect();
        for v in self.store.profiles() {
            {
                let id = v["id"].as_str().unwrap_or_default().to_string();
                if let Some(existing) = out.iter_mut().find(|p| p["id"].as_str() == Some(&id)) {
                    let mut v = v;
                    v["builtin"] = serde_json::Value::Bool(true);
                    v["modified"] = serde_json::Value::Bool(true);
                    *existing = v;
                } else {
                    out.push(v);
                }
            }
        }
        Ok(out)
    }

    pub fn profile(&self, id: &str) -> ProfileCore {
        self.profiles_json()
            .ok()
            .and_then(|ps| ps.into_iter().find(|p| p["id"].as_str() == Some(id)))
            .or_else(|| serde_json::from_str(BUILTIN_PROFILES[0]).ok())
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default()
    }

    fn active_profile(&self) -> ProfileCore {
        let sid = self.perform.lock().setlist_id.clone();
        let id = sid
            .and_then(|s| self.store.setlist(&s).ok())
            .map(|s| s.profile_id)
            .unwrap_or_else(|| self.settings.read().active_profile_id.clone());
        self.profile(&id)
    }

    // ------------------------------------------------------------ audio

    pub fn apply_outputs(&self) {
        let s = self.settings.read().audio.clone();
        let cfg = OutputConfig {
            master_device: s.master_device,
            master_channels: s.master_channels,
            cue_device: s.cue_device,
            cue_channels: s.cue_channels,
        };
        let master_taps = vec![self.decks[MAIN].shared.master_tap.clone()];
        let cue_taps = vec![self.decks[MAIN].shared.cue_tap.clone(), self.decks[PREVIEW].shared.cue_tap.clone()];
        let (rate, status) = self.outputs.apply(cfg, master_taps, cue_taps);
        let changed = self.rate.swap(rate, Ordering::SeqCst) != rate;
        for d in &self.decks {
            if changed || true {
                d.reconfigure();
            }
        }
        emit(&self.handle, "output-status", &status);
    }

    pub fn output_status(&self) -> OutputStatus {
        self.outputs.status.lock().clone()
    }

    fn fade_secs(&self) -> f64 {
        let s = self.settings.read();
        match s.fade_out_ms {
            Some(ms) => ms as f64 / 1000.0,
            None => {
                drop(s);
                let p = self.active_profile();
                if p.fade_out_seconds > 0.0 { p.fade_out_seconds } else { 5.0 }
            }
        }
    }

    /// Effective playback settings: entry overrides → version → (track defaults applied by caller).
    pub fn resolve_settings(version: &Version, entry: Option<&SetlistEntry>) -> Overrides {
        let v = &version.settings;
        let e = entry.map(|e| e.overrides.clone()).unwrap_or_default();
        Overrides {
            target_bpm: e.target_bpm.or(v.target_bpm),
            cue_in_ms: e.cue_in_ms.or(v.cue_in_ms),
            cue_out_ms: e.cue_out_ms.or(v.cue_out_ms),
            fade_out_ms: e.fade_out_ms.or(v.fade_out_ms),
            gain_db: e.gain_db.or(v.gain_db),
            stems: e.stems.or_else(|| v.stems.clone()),
        }
    }

    /// Picks the source to play: the track's preferred kind if available,
    /// otherwise a file on disk, otherwise Spotify.
    fn choose_source(&self, track: &Track) -> Result<(String, String)> {
        let file = || crate::library::resolve_path(&self.store, &track.rec.id).map(|p| ("file".to_string(), p.to_string_lossy().to_string()));
        let spotify = || track.rec.sources.iter().find(|s| s.kind == "spotify").map(|s| ("spotify".to_string(), s.key.clone()));
        let pick = match track.rec.preferred_source.as_deref() {
            Some("spotify") => spotify().or_else(file),
            _ => file().or_else(spotify),
        };
        pick.ok_or_else(|| anyhow!("No playable source for \"{}\": its file is missing. Rescan your library folders or link a Spotify track.", track.rec.title))
    }

    /// Builds a deck load request for a version (optionally in the context of a setlist entry).
    pub fn load_request(&self, deck: usize, track: &Track, version: &Version, entry: Option<&SetlistEntry>, play: bool) -> Result<LoadRequest> {
        let s = self.settings.read().clone();
        let r = Self::resolve_settings(version, entry);
        let tempo = match (r.target_bpm, track.rec.bpm) {
            (Some(t), Some(b)) if b > 0.0 => (t / fold_bpm(b, t)).clamp(0.5, 2.0),
            _ => 1.0,
        };
        let start_ms = r
            .cue_in_ms
            .or(if s.trim_silence { track.rec.audio_start_ms.map(|v| (v - 50).max(0)) } else { None })
            .unwrap_or(0);
        let end_ms = r.cue_out_ms.or(if s.trim_silence { track.rec.audio_end_ms.map(|v| v + 100) } else { None });
        let mut gain = r.gain_db.unwrap_or(track.rec.gain_db);
        if s.auto_gain {
            if let Some(l) = track.rec.loudness_db {
                gain += (s.target_loudness_db - l).clamp(-12.0, 12.0);
            }
        }
        let (kind, key) = self.choose_source(track)?;
        let open: crate::audio::deck::SourceOpener = if kind == "spotify" {
            let dp = self.spotify.deck_player(deck)?;
            let dur = track.rec.duration_ms;
            let start = start_ms as f64 / 1000.0;
            Box::new(move || Ok(Box::new(SpotifySource::open(dp, &key, dur, start)?) as Box<dyn Source>))
        } else {
            let path = PathBuf::from(key);
            Box::new(move || Ok(Box::new(LocalSource::open(&path)?) as Box<dyn Source>))
        };
        Ok(LoadRequest {
            track_id: Some(track.rec.id.clone()),
            version_id: Some(version.id.clone()),
            entry_id: entry.map(|e| e.id.clone()),
            open,
            start_secs: start_ms as f64 / 1000.0,
            end_secs: end_ms.map(|v| v as f64 / 1000.0),
            fade_at_secs: r.fade_out_ms.map(|v| v as f64 / 1000.0),
            fade_secs: self.fade_secs(),
            tempo,
            gain_db: gain,
            play,
        })
    }

    pub fn preview_load(&self, version_id: &str, entry_id: Option<&str>, play: bool) -> Result<()> {
        let (version, track_id) = self.store.version(version_id)?;
        let track = self.store.track(&track_id)?;
        let entry = entry_id.and_then(|id| self.store.entry(id).ok()).map(|(e, _)| e);
        let req = self.load_request(PREVIEW, &track, &version, entry.as_ref(), play)?;
        self.untake(&["preview.tempo"]);
        self.decks[PREVIEW].load(req);
        Ok(())
    }

    pub fn test_tone(&self, deck: usize) {
        let req = LoadRequest {
            track_id: None,
            version_id: None,
            entry_id: None,
            open: Box::new(|| Ok(Box::new(ToneSource::new(4.0)) as Box<dyn Source>)),
            start_secs: 0.0,
            end_secs: None,
            fade_at_secs: None,
            fade_secs: 1.0,
            tempo: 1.0,
            gain_db: 0.0,
            play: true,
        };
        if deck == MAIN && self.decks[MAIN].status() == "playing" {
            return;
        }
        self.decks[deck].load(req);
    }

    // ------------------------------------------------------------ performance

    pub fn auto_advance(&self) -> bool {
        if let Some(v) = self.perform.lock().auto_advance_override {
            return v;
        }
        let s = self.settings.read().auto_advance;
        s.unwrap_or_else(|| self.active_profile().auto_advance)
    }

    pub fn gap_seconds(&self) -> f64 {
        let s = self.settings.read().gap_seconds;
        s.unwrap_or_else(|| self.active_profile().gap.target_seconds)
    }

    pub fn open_setlist(&self, id: Option<Id>) -> Result<()> {
        if let Some(id) = &id {
            self.store.setlist(id)?;
        }
        {
            let mut p = self.perform.lock();
            p.setlist_id = id.clone();
            p.pending = None;
        }
        crate::store::write_atomic(&self.local_dir.join("active-setlist"), id.clone().unwrap_or_default().as_bytes())?;
        self.emit_perform();
        Ok(())
    }

    fn entries(&self) -> Vec<SetlistEntry> {
        let id = self.perform.lock().setlist_id.clone();
        id.map(|id| self.store.entries(&id)).unwrap_or_default()
    }

    /// The entry that would play next.
    pub fn next_entry(&self) -> Option<SetlistEntry> {
        let entries = self.entries();
        let current = self.decks[MAIN].entry_id();
        let cursor = current
            .and_then(|c| entries.iter().position(|e| e.id == c))
            .or_else(|| entries.iter().rposition(|e| e.status != "pending"));
        let after = cursor.map(|c| c + 1).unwrap_or(0);
        entries[after.min(entries.len())..]
            .iter()
            .find(|e| e.status == "pending")
            .or_else(|| entries.iter().find(|e| e.status == "pending"))
            .cloned()
    }

    /// "Play Next": start the next entry. If a song is still playing it is
    /// faded out first (songs never overlap).
    pub fn play_next(&self) -> Result<String> {
        self.request_entry(PendingNext::Next)
    }

    pub fn play_entry(&self, entry_id: Id) -> Result<String> {
        self.request_entry(PendingNext::Entry(entry_id))
    }

    fn request_entry(&self, target: PendingNext) -> Result<String> {
        let main = &self.decks[MAIN];
        let status = main.status();
        if status == "playing" || status == "loading" {
            if main.is_fading() {
                self.perform.lock().pending = Some(target);
                self.emit_perform();
                return Ok("queued".into());
            }
            let confirm = self.settings.read().confirm_next_while_playing;
            {
                let mut p = self.perform.lock();
                let armed = p.armed_until.map(|t| t > Instant::now()).unwrap_or(false);
                if confirm && !armed {
                    p.armed_until = Some(Instant::now() + Duration::from_secs(3));
                    drop(p);
                    self.emit_perform();
                    return Ok("armed".into());
                }
                p.armed_until = None;
                p.pending = Some(target);
            }
            let ms = self.settings.read().manual_next_fade_ms;
            main.fade_out(ms as f64 / 1000.0);
            self.emit_perform();
            return Ok("fading".into());
        }
        let entry = match target {
            PendingNext::Next => self.next_entry().ok_or_else(|| anyhow!("Nothing left in the setlist"))?,
            PendingNext::Entry(id) => self.store.entry(&id)?.0,
        };
        self.start_entry(&entry)?;
        Ok("started".into())
    }

    fn start_entry(&self, entry: &SetlistEntry) -> Result<()> {
        let version_id = entry.version_id.clone().ok_or_else(|| {
            let style = entry
                .style
                .as_deref()
                .map(|s| self.active_profile().style_name(s))
                .unwrap_or_else(|| "this".into());
            anyhow!("Choose a song for the {style} slot first")
        })?;
        let (version, track_id) = self.store.version(&version_id)?;
        let track = self.store.track(&track_id)?;
        let req = self.load_request(MAIN, &track, &version, Some(entry), true)?;
        let tempo = req.tempo;
        self.finish_current_entry();
        self.untake(&["main.tempo"]);
        self.decks[MAIN].load(req);
        self.store.entry_set_playing(&entry.id)?;
        let setlist = self.perform.lock().setlist_id.clone();
        let target = Self::resolve_settings(&version, Some(entry)).target_bpm;
        let bpm = track.rec.bpm.map(|b| {
            let base = target.map(|t| fold_bpm(b, t)).unwrap_or(b);
            (base * tempo * 10.0).round() / 10.0
        });
        let style = entry.style.clone().or(version.style.clone());
        self.store.mark_played(HistoryItem {
            played_at: now_ms(),
            track_id: track.rec.id.clone(),
            version_id: Some(version.id.clone()),
            style,
            bpm,
            setlist_id: setlist.clone(),
        })?;
        {
            let mut p = self.perform.lock();
            p.gap_started = None;
            p.pending = None;
            p.armed_until = None;
        }
        if let Some(sid) = setlist {
            self.emit_setlist(&sid);
        }
        emit(&self.handle, "library-changed", vec![track.rec.id.clone()]);
        self.emit_perform();
        Ok(())
    }

    /// Marks the main deck's entry as played.
    fn finish_current_entry(&self) {
        if let Some(id) = self.decks[MAIN].entry_id() {
            if let Ok((e, _)) = self.store.entry(&id) {
                if e.status == "playing" {
                    let _ = self.store.entry_update(
                        &id,
                        &EntryPatch { status: Some("played".into()), ..Default::default() },
                    );
                }
            }
        }
    }

    pub fn skip_next(&self) -> Result<()> {
        if let Some(e) = self.next_entry() {
            let (_, sid) = self.store.entry(&e.id)?;
            self.store.entry_update(&e.id, &EntryPatch { status: Some("skipped".into()), ..Default::default() })?;
            self.emit_setlist(&sid);
            self.emit_perform();
        }
        Ok(())
    }

    pub fn main_fade_out(&self) {
        let secs = self.fade_secs();
        self.decks[MAIN].fade_out(secs);
    }

    fn on_deck_event(&self, ev: DeckEvent) {
        match ev {
            DeckEvent::Ended { deck: MAIN, entry_id, .. } => {
                self.finish_current_entry();
                let _ = entry_id;
                // Only an explicit "Play Next" during a fade starts the next song;
                // otherwise we wait in the pause between songs.
                let pending = self.perform.lock().pending.take();
                if let Some(t) = pending {
                    if let Err(e) = self.request_entry(t) {
                        emit(&self.handle, "toast", format!("{e:#}"));
                        self.perform.lock().gap_started = Some(now_ms());
                    }
                } else {
                    self.perform.lock().gap_started = Some(now_ms());
                }
                let sid = self.perform.lock().setlist_id.clone();
                if let Some(id) = sid {
                    self.emit_setlist(&id);
                }
                self.emit_perform();
            }
            DeckEvent::Error { deck, message, .. } => {
                let which = if deck == MAIN { "Main deck" } else { "Preview" };
                emit(&self.handle, "toast", format!("{which}: {message}"));
                if deck == MAIN {
                    self.perform.lock().gap_started = Some(now_ms());
                    self.emit_perform();
                }
            }
            DeckEvent::Loaded { .. } | DeckEvent::Ended { .. } => {}
        }
    }

    pub fn perform_state(&self) -> PerformState {
        let auto = self.auto_advance();
        let gap_secs = self.gap_seconds();
        let p = self.perform.lock();
        let setlist = p.setlist_id.as_ref().and_then(|id| self.store.setlist(id).ok());
        let mode = setlist.as_ref().map(|s| s.mode.clone()).unwrap_or_else(|| "live".into());
        let gap = p.gap_started;
        let pending = p.pending.is_some();
        let armed = p.armed_until.map(|t| t > Instant::now()).unwrap_or(false);
        let message = p.message.clone();
        let setlist_id = p.setlist_id.clone();
        drop(p);
        let entries = self.entries();
        let started_at = entries.iter().filter_map(|e| e.played_at).min();
        let next = self.next_entry();
        let next_ready = next.as_ref().map(|e| e.version_id.is_some()).unwrap_or(false);
        PerformState {
            setlist_id,
            mode,
            current_entry_id: self.decks[MAIN].entry_id(),
            next_entry_id: next.map(|e| e.id),
            gap_started_at: gap,
            gap_seconds: gap_secs,
            auto_advance: auto,
            auto_advance_at: if auto && next_ready { gap.map(|g| g + (gap_secs * 1000.0) as i64) } else { None },
            pending_next: pending,
            armed,
            message,
            started_at,
        }
    }

    pub fn emit_perform(&self) {
        emit(&self.handle, "perform-state", self.perform_state());
        emit(&self.handle, "projector", self.projector_payload());
    }

    pub fn emit_setlist(&self, id: &str) {
        emit(&self.handle, "setlist-changed", id);
        if self.perform.lock().setlist_id.as_deref() == Some(id) {
            emit(&self.handle, "projector", self.projector_payload());
        }
    }

    fn proj_entry(&self, e: &SetlistEntry, profile: &ProfileCore, lookup: &HashMap<Id, (Version, Track)>) -> ProjEntry {
        let vt = e.version_id.as_ref().and_then(|id| lookup.get(id));
        let style_key = e.style.clone().or_else(|| vt.and_then(|(v, _)| v.style.clone()));
        let st = style_key.as_deref().and_then(|s| profile.style(s));
        let target = vt.and_then(|(v, _)| Self::resolve_settings(v, Some(e)).target_bpm);
        ProjEntry {
            id: e.id.clone(),
            title: vt.map(|(_, t)| t.rec.title.clone()).unwrap_or_default(),
            artist: vt.map(|(_, t)| t.rec.artist.clone()).unwrap_or_default(),
            style_name: style_key.as_deref().map(|s| profile.style_name(s)).unwrap_or_default(),
            style: style_key,
            style_short: st.map(|s| s.short.clone()).unwrap_or_default(),
            color: st.map(|s| s.color.clone()).unwrap_or_else(|| "#9ca3af".into()),
            tags: e
                .tags
                .iter()
                .filter_map(|tid| profile.tags.iter().find(|t| &t.id == tid && t.announce))
                .map(|t| ProjTag { id: t.id.clone(), name: t.name.clone(), color: t.color.clone() })
                .collect(),
            status: e.status.clone(),
            bpm: target.or_else(|| vt.and_then(|(_, t)| t.rec.bpm)),
        }
    }

    pub fn projector_payload(&self) -> ProjectorPayload {
        let settings = self.settings.read().projector.clone();
        let profile = self.active_profile();
        let (setlist_id, gap, message) = {
            let p = self.perform.lock();
            (p.setlist_id.clone(), p.gap_started, p.message.clone())
        };
        let setlist = setlist_id.as_ref().and_then(|id| self.store.setlist(id).ok());
        let entries = self.entries();
        let ids: HashSet<Id> = entries.iter().filter_map(|e| e.version_id.clone()).collect();
        let mut lookup: HashMap<Id, (Version, Track)> = HashMap::new();
        for vid in ids {
            if let Ok((v, tid)) = self.store.version(&vid) {
                if let Ok(t) = self.store.track(&tid) {
                    lookup.insert(vid, (v, t));
                }
            }
        }
        let proj: Vec<ProjEntry> = entries.iter().map(|e| self.proj_entry(e, &profile, &lookup)).collect();
        let current_id = self.decks[MAIN].entry_id();
        let current_index = current_id.as_ref().and_then(|c| entries.iter().position(|e| &e.id == c));
        let main_status = self.decks[MAIN].status();
        let playing = main_status == "playing";
        let current = current_index.map(|i| proj[i].clone()).filter(|_| main_status != "empty");
        let next = self.next_entry().and_then(|n| proj.iter().find(|p| p.id == n.id).cloned());
        let mut history: Vec<ProjEntry> = entries
            .iter()
            .zip(proj.iter())
            .filter(|(e, _)| e.status == "played" && Some(&e.id) != current_id.as_ref())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .take(settings.history_count as usize)
            .map(|(_, p)| p.clone())
            .collect();
        history.retain(|h| !h.title.is_empty() || !h.style_name.is_empty());
        let mode = match settings.display_mode.as_str() {
            "live" | "preset" => settings.display_mode.clone(),
            _ => setlist.as_ref().map(|s| s.mode.clone()).unwrap_or_else(|| "live".into()),
        };
        let title = if settings.title.trim().is_empty() {
            setlist.as_ref().map(|s| s.name.clone()).unwrap_or_else(|| profile.name.clone())
        } else {
            settings.title.clone()
        };
        ProjectorPayload {
            title,
            mode,
            settings,
            current,
            next,
            history,
            entries: proj,
            current_index,
            in_gap: gap.is_some() && !playing,
            playing,
            message,
        }
    }

    pub fn deck_snapshots(&self) -> [DeckSnapshot; 2] {
        [self.decks[0].snapshot(), self.decks[1].snapshot()]
    }

    // ------------------------------------------------------------ MIDI

    fn untake(&self, actions: &[&str]) {
        let mut t = self.takeover.lock();
        for a in actions {
            t.engaged.remove(*a);
        }
    }

    /// Soft takeover: a hardware fader only takes control once it reaches the
    /// current software value, so loading a song with a preset tempo doesn't
    /// jump to wherever the fader happens to be.
    fn takeover(&self, action: &str, hw: f64, current: f64) -> bool {
        let mut t = self.takeover.lock();
        let prev = t.last_hw.insert(action.to_string(), hw);
        if t.engaged.contains(action) {
            return true;
        }
        let close = (hw - current).abs() < 0.03;
        let crossed = prev.map(|p| (p - current).signum() != (hw - current).signum()).unwrap_or(false);
        if close || crossed {
            t.engaged.insert(action.to_string());
            return true;
        }
        false
    }

    pub fn on_midi(&self, action: &str, value: Value) {
        let range = self.settings.read().tempo_range_pct / 100.0;
        let tempo_to_hw = |t: f64| ((t - 1.0) / (2.0 * range) + 0.5).clamp(0.0, 1.0);
        let hw_to_tempo = |v: f64| {
            let v = if (v - 0.5).abs() < 0.01 { 0.5 } else { v };
            1.0 + (v - 0.5) * 2.0 * range
        };
        let trim_db = |v: f64| if v <= 0.5 { -40.0 + 40.0 * (v / 0.5) } else { 6.0 * (v - 0.5) / 0.5 };
        let trim_to_hw = |db: f64| if db <= 0.0 { ((db + 40.0) / 40.0 * 0.5).max(0.0) } else { 0.5 + db / 6.0 * 0.5 };
        let mut result: Result<()> = Ok(());
        match (action, value) {
            ("perform.playNext", Value::Press) => result = self.play_next().map(|_| ()),
            ("perform.skipNext", Value::Press) => result = self.skip_next(),
            ("perform.toggleAutoAdvance", Value::Press) => {
                let v = !self.auto_advance();
                self.perform.lock().auto_advance_override = Some(v);
                self.emit_perform();
            }
            ("main.playPause", Value::Press) => {
                let d = &self.decks[MAIN];
                if d.status() == "playing" {
                    d.pause();
                } else if d.status() == "paused" {
                    d.play();
                } else {
                    result = self.play_next().map(|_| ());
                }
                self.emit_perform();
            }
            ("main.fadeOut", Value::Press) => self.main_fade_out(),
            ("main.stop", Value::Press) => {
                let d = &self.decks[MAIN];
                if d.status() == "paused" {
                    d.seek(d.snapshot().start_ms as f64 / 1000.0);
                }
            }
            ("main.tempo", Value::Abs(v)) | ("preview.tempo", Value::Abs(v)) => {
                let deck = if action.starts_with("main") { MAIN } else { PREVIEW };
                let cur = tempo_to_hw(self.decks[deck].tempo());
                if self.takeover(action, v, cur) {
                    self.decks[deck].set_tempo(hw_to_tempo(v));
                }
            }
            ("main.tempoNudge", Value::Rel(n)) => {
                let d = &self.decks[MAIN];
                d.set_tempo(d.tempo() + n as f64 * 0.001);
                self.untake(&["main.tempo"]);
            }
            ("main.tempoReset", Value::Press) => {
                let d = &self.decks[MAIN];
                if let (Some(tid), Some(vid)) = (d.track_id(), d.version_id()) {
                    if let (Ok(t), Ok((v, _))) = (self.store.track(&tid), self.store.version(&vid)) {
                        let e = d.entry_id().and_then(|id| self.store.entry(&id).ok()).map(|(e, _)| e);
                        let target = Self::resolve_settings(&v, e.as_ref()).target_bpm;
                        let tempo = match (target, t.rec.bpm) {
                            (Some(x), Some(b)) if b > 0.0 => x / fold_bpm(b, x),
                            _ => 1.0,
                        };
                        d.set_tempo(tempo);
                        self.untake(&["main.tempo"]);
                    }
                }
            }
            ("main.trim", Value::Abs(v)) | ("preview.trim", Value::Abs(v)) => {
                let deck = if action.starts_with("main") { MAIN } else { PREVIEW };
                let cur = trim_to_hw(self.decks[deck].shared.trim_db.get());
                if self.takeover(action, v, cur) {
                    self.decks[deck].set_trim_db(trim_db(v));
                }
            }
            ("main.pfl", Value::Press) => {
                let d = &self.decks[MAIN];
                d.set_pfl(!d.shared.pfl.load(Ordering::Relaxed));
            }
            ("preview.playPause", Value::Press) => self.decks[PREVIEW].toggle(),
            ("preview.cue", Value::Press) => {
                let d = &self.decks[PREVIEW];
                d.seek(d.snapshot().start_ms as f64 / 1000.0);
            }
            ("preview.seek", Value::Rel(n)) => {
                let d = &self.decks[PREVIEW];
                let mut ps = self.pending_seek.lock();
                let base = ps.unwrap_or_else(|| d.position_secs());
                *ps = Some((base + n as f64 * 0.25).max(0.0));
            }
            (a, Value::Press) if a.starts_with("preview.jump:") => {
                if let Ok(frac) = a["preview.jump:".len()..].parse::<f64>() {
                    let d = &self.decks[PREVIEW];
                    let snap = d.snapshot();
                    let end = snap.end_ms.unwrap_or(snap.duration_ms) as f64 / 1000.0;
                    d.seek(end * frac);
                }
            }
            ("master.volume", Value::Abs(v)) => self.levels.master.set((v * v) as f32),
            ("cue.volume", Value::Abs(v)) => self.levels.cue.set((v * v) as f32),
            (a, v) if a.starts_with("browse.") => {
                let payload = match v {
                    Value::Rel(n) => serde_json::json!({ "action": a, "delta": n }),
                    _ => serde_json::json!({ "action": a }),
                };
                emit(&self.handle, "midi-ui", payload);
            }
            _ => {}
        }
        if let Err(e) = result {
            emit(&self.handle, "toast", format!("{e:#}"));
        }
    }

    // ------------------------------------------------------------ background

    pub fn queue_analysis(&self, id: Id, priority: bool) {
        let _ = self.analysis_tx.send((id, priority));
    }

    fn start_threads(self: &Arc<Self>, deck_rx: Receiver<DeckEvent>, analysis_rx: Receiver<(Id, bool)>) {
        // Deck events.
        let weak = Arc::downgrade(self);
        std::thread::Builder::new()
            .name("deck-events".into())
            .spawn(move || {
                while let Ok(ev) = deck_rx.recv() {
                    let Some(app) = weak.upgrade() else { return };
                    app.on_deck_event(ev);
                }
            })
            .unwrap();

        // UI poller: deck positions, meters, LEDs, auto-advance, jog seeks.
        let weak = Arc::downgrade(self);
        std::thread::Builder::new()
            .name("poller".into())
            .spawn(move || {
                let mut last: Option<String> = None;
                let mut last_armed = false;
                loop {
                    std::thread::sleep(Duration::from_millis(50));
                    let Some(app) = weak.upgrade() else { return };
                    if let Some(secs) = app.pending_seek.lock().take() {
                        app.decks[PREVIEW].seek(secs);
                    }
                    let snaps = app.deck_snapshots();
                    let key = serde_json::to_string(&snaps).unwrap_or_default();
                    let lv = LevelsSnapshot {
                        master: app.levels.master_peak.get(),
                        cue: app.levels.cue_peak.get(),
                        master_volume: app.levels.master.get(),
                        cue_volume: app.levels.cue.get(),
                    };
                    app.levels.master_peak.set(app.levels.master_peak.get() * 0.6);
                    app.levels.cue_peak.set(app.levels.cue_peak.get() * 0.6);
                    if last.as_deref() != Some(&key) || lv.master > 0.001 || lv.cue > 0.001 {
                        emit(&app.handle, "deck-state", &snaps);
                        emit(&app.handle, "levels", &lv);
                        last = Some(key);
                    }
                    // Auto-advance after the pause.
                    let (gap, pending) = {
                        let p = app.perform.lock();
                        (p.gap_started, p.pending.is_some())
                    };
                    if let Some(g) = gap {
                        if !pending && app.auto_advance() && now_ms() - g >= (app.gap_seconds() * 1000.0) as i64 {
                            let ready = app.next_entry().map(|e| e.version_id.is_some()).unwrap_or(false);
                            if ready && app.decks[MAIN].status() != "playing" {
                                if let Err(e) = app.play_next() {
                                    emit(&app.handle, "toast", format!("{e:#}"));
                                    app.perform.lock().gap_started = None;
                                }
                            }
                        }
                    }
                    let armed = app.perform.lock().armed_until.map(|t| t > Instant::now()).unwrap_or(false);
                    if armed != last_armed {
                        last_armed = armed;
                        app.emit_perform();
                    }
                    if let Some(midi) = app.midi.get() {
                        let main = &snaps[MAIN];
                        let next_ready = main.status != "playing"
                            && app.next_entry().map(|e| e.version_id.is_some()).unwrap_or(false);
                        let mut st = HashMap::new();
                        st.insert("main.playing", main.status == "playing");
                        st.insert("main.fading", main.fading);
                        st.insert("main.pfl", main.pfl);
                        st.insert("preview.playing", snaps[PREVIEW].status == "playing");
                        st.insert("perform.nextReady", next_ready && (now_ms() / 500) % 2 == 0);
                        st.insert("perform.autoAdvance", app.auto_advance());
                        midi.set_leds(&st);
                    }
                }
            })
            .unwrap();

        // Analysis workers (two, so playback stays smooth).
        let (lo_tx, lo_rx) = unbounded::<Id>();
        let (hi_tx, hi_rx) = unbounded::<Id>();
        std::thread::spawn(move || {
            while let Ok((id, prio)) = analysis_rx.recv() {
                let _ = if prio { hi_tx.send(id) } else { lo_tx.send(id) };
            }
        });
        for i in 0..2 {
            let weak = Arc::downgrade(self);
            let (hi, lo) = (hi_rx.clone(), lo_rx.clone());
            std::thread::Builder::new()
                .name(format!("analysis-{i}"))
                .spawn(move || loop {
                    let id = match hi.try_recv() {
                        Ok(id) => id,
                        Err(_) => crossbeam_channel::select! {
                            recv(hi) -> m => match m { Ok(id) => id, Err(_) => return },
                            recv(lo) -> m => match m { Ok(id) => id, Err(_) => return },
                        },
                    };
                    let Some(app) = weak.upgrade() else { return };
                    let r = app.analyze(&id);
                    if let Err(e) = r {
                        log::warn!("analysis of track {id} failed: {e:#}");
                    }
                    emit(&app.handle, "library-changed", vec![id]);
                })
                .unwrap();
        }
    }

    fn analyze(&self, id: &str) -> Result<()> {
        if crate::library::resolve_path(&self.store, id).is_some() {
            return crate::library::analyze_track(&self.store, id);
        }
        let t = self.store.track(id)?;
        let uri = t
            .rec
            .sources
            .iter()
            .find(|s| s.kind == "spotify")
            .map(|s| s.key.clone())
            .ok_or_else(|| anyhow!("no playable source"))?;
        let (mono, sr) = self.spotify.decode_for_analysis(&uri)?;
        let a = crate::analysis::analyze(&mono, sr);
        self.store.set_analysis(id, &a)?;
        Ok(())
    }

    pub fn scan_library(self: &Arc<Self>) {
        let app = self.clone();
        std::thread::spawn(move || {
            if app.scan_progress.lock().running {
                return;
            }
            let folders = app.settings.read().library_folders.clone();
            let h = app.handle.clone();
            let app2 = app.clone();
            let progress = move |p: &crate::library::ScanProgress| {
                *app2.scan_progress.lock() = p.clone();
                emit(&h, "scan-progress", p);
            };
            match crate::library::scan(&app.store, &folders, &progress) {
                Ok(_) => {}
                Err(e) => emit(&app.handle, "toast", format!("Library scan failed: {e:#}")),
            }
            app.scan_progress.lock().running = false;
            emit(&app.handle, "library-changed", Vec::<Id>::new());
            for id in app.store.needs_analysis() {
                app.queue_analysis(id, false);
            }
        });
    }

    pub fn import_paths(self: &Arc<Self>, paths: Vec<String>) {
        let app = self.clone();
        std::thread::spawn(move || {
            let h = app.handle.clone();
            let progress = move |p: &crate::library::ScanProgress| emit(&h, "scan-progress", p);
            match crate::library::import_paths(&app.store, &paths, &progress) {
                Ok(ids) => {
                    emit(&app.handle, "library-changed", Vec::<Id>::new());
                    emit(&app.handle, "imported", &ids);
                    for id in app.store.needs_analysis() {
                        app.queue_analysis(id, true);
                    }
                }
                Err(e) => emit(&app.handle, "toast", format!("Import failed: {e:#}")),
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::fold_bpm;

    #[test]
    fn folds_octaves() {
        assert_eq!(fold_bpm(84.0, 168.0), 168.0);
        assert_eq!(fold_bpm(300.0, 150.0), 150.0);
        assert_eq!(fold_bpm(118.0, 110.0), 118.0);
        assert_eq!(fold_bpm(59.0, 118.0), 118.0);
    }
}

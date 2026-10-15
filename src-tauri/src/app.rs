//! Application state and performance logic.

use crate::audio::output::{OutputConfig, OutputStatus, Outputs};
use crate::audio::source::{LocalSource, Source, ToneSource};
use crate::audio::{Deck, DeckEvent, DeckSnapshot, Levels, LoadRequest};
use crate::club::{Club, Clubs};
use std::collections::HashMap;
use crate::store::Store;
use crate::midi::{self, Midi};
use crate::model::*;
use crate::settings::Settings;
use crate::spotify::{Spotify, SpotifySource};
use anyhow::{anyhow, Result};
use crossbeam_channel::{unbounded, Receiver, Sender};
use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

pub use crate::perform::{PendingNext, Perform, PerformState, ProjectorPayload};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelsSnapshot {
    pub master: f32,
    pub cue: f32,
    pub master_volume: f32,
    pub cue_volume: f32,
    /// Each deck's level (after its volume, before the crossfader).
    pub decks: [f32; 2],
}

/// Fade-out length when neither you nor the club set one (seconds).
const DEFAULT_FADE_SECS: f64 = 5.0;

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
    pub stems: Arc<crate::stems::Stems>,
    /// Text-to-speech (Piper) for announcements, and their playback.
    pub tts: Arc<crate::tts::Tts>,
    pub announcer: Arc<crate::audio::announcer::Announcer>,
    pub midi: OnceLock<Arc<Midi>>,
    pub perform: Mutex<Perform>,
    /// Parsed club guidelines by club and version (see `club`).
    club_cache: Mutex<HashMap<ClubRef, Arc<Club>>>,
    /// Fingerprint comparisons already made: (track fingerprint, club song fingerprint) → same version.
    fingerprint_matches: Mutex<HashMap<(u64, u64), bool>>,
    pub scan_progress: Mutex<crate::library::ScanProgress>,
    analysis_tx: Sender<(Id, bool)>,
    /// Spotify songs already queued for analysis this session (each is
    /// streamed once in the background for its waveform and BPM).
    spotify_analysed: Mutex<std::collections::HashSet<Id>>,
    pub(crate) takeover: Mutex<crate::controls::Takeover>,
    pub(crate) deck_ctl: Mutex<[crate::controls::DeckControlState; 2]>,
    /// Pace decks by the headphone output (set when no master device is open).
    pub clock_cue: Arc<std::sync::atomic::AtomicBool>,
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
        spotify.set_web_client_id(&crate::commands::spotify_web_client_id(&settings));
        let stems = crate::stems::Stems::new(&data_dir);
        let tts = crate::tts::Tts::new(&data_dir, rt.handle().clone());
        let announcer = Arc::new(crate::audio::announcer::Announcer::default());
        let levels = Arc::new(Levels::default());
        let rate = Arc::new(AtomicU32::new(48000));
        let buffer_ms = Arc::new(AtomicU32::new(settings.audio.buffer_ms));
        let (deck_tx, deck_rx) = unbounded();
        let clock_cue = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let decks = [
            Deck::new(0, rate.clone(), buffer_ms.clone(), clock_cue.clone(), deck_tx.clone()),
            Deck::new(1, rate.clone(), buffer_ms.clone(), clock_cue.clone(), deck_tx),
        ];
        for d in &decks {
            d.set_keylock(settings.keylock);
            d.set_stretch_engine(&settings.stretch_engine);
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
            stems,
            tts,
            announcer,
            midi: OnceLock::new(),
            // Crossfader centered: the room hears both decks until the DJ moves it.
            perform: Mutex::new(Perform { crossfader: crate::perform::CENTER, ..Default::default() }),
            club_cache: Mutex::new(HashMap::new()),
            fingerprint_matches: Mutex::new(HashMap::new()),
            scan_progress: Mutex::new(Default::default()),
            analysis_tx,
            spotify_analysed: Mutex::new(Default::default()),
            takeover: Mutex::new(Default::default()),
            deck_ctl: Mutex::new(Default::default()),
            clock_cue,
            _rt: rt,
        });
        // Start with deck A in the room and headphones on deck B.
        app.decks[1].set_pfl(true);
        app.update_routing();
        app.apply_outputs();
        app.start_threads(deck_rx, analysis_rx);
        let weak = Arc::downgrade(&app);
        let handler: midi::Handler = Arc::new(move |action, value| {
            if let Some(app) = weak.upgrade() {
                app.on_midi(action, value);
            }
        });
        let weak = Arc::downgrade(&app);
        app.stems.set_on_change(move |track| {
            if let Some(app) = weak.upgrade() {
                app.on_stems_changed(track);
            }
        });
        let weak = Arc::downgrade(&app);
        let last = Mutex::new(Instant::now() - Duration::from_secs(1));
        app.tts.on_change(Box::new(move || {
            // Download progress arrives per chunk: send at most 5 updates a second.
            let mut l = last.lock();
            if l.elapsed() < Duration::from_millis(200) {
                return;
            }
            *l = Instant::now();
            if let Some(app) = weak.upgrade() {
                app.emit_tts_status();
            }
        }));
        let announce = app.settings.read().announce.clone();
        if announce.enabled {
            app.tts.warm_up(announce.tts_config());
        }
        let (mapping, enabled, auto) = app.current_mapping();
        let _ = app.midi.set(Midi::new(mapping, enabled, auto, handler));
        // Restore the last active setlist.
        if let Ok(id) = std::fs::read_to_string(app.local_dir.join("active-setlist")) {
            let id = id.trim().to_string();
            if app.store.setlist(&id).is_ok() {
                app.perform.lock().setlist_id = Some(id);
            }
        }
        Ok(app)
    }

    /// The configured mapping, or (when the setting is empty = automatic) a
    /// generic fallback plus the candidates to pick from by controller name.
    pub fn current_mapping(&self) -> (midi::Mapping, bool, Option<Vec<midi::Mapping>>) {
        let s = self.settings.read();
        let all = self.all_mappings_with(&s);
        let fallback = || all.iter().find(|m| m.device_match.is_empty()).cloned().unwrap_or_else(|| all[0].clone());
        if s.midi.mapping_id.is_empty() {
            return (fallback(), s.midi.enabled, Some(all.clone()));
        }
        let m = all.iter().find(|m| m.id == s.midi.mapping_id).cloned().unwrap_or_else(fallback);
        (m, s.midi.enabled, None)
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
        self.spotify.set_web_client_id(&crate::commands::spotify_web_client_id(&new));
        if serde_json::to_string(&old.announce)? != serde_json::to_string(&new.announce)? {
            self.emit_tts_status();
            if new.announce.enabled {
                self.tts.warm_up(new.announce.tts_config());
            }
        }
        if old.keylock != new.keylock {
            for d in &self.decks {
                d.set_keylock(new.keylock);
            }
        }
        for d in &self.decks {
            d.set_stretch_engine(&new.stretch_engine);
        }
        if serde_json::to_string(&old.midi)? != serde_json::to_string(&new.midi)? {
            let (m, enabled, auto) = self.current_mapping();
            if let Some(midi) = self.midi.get() {
                midi.set_mapping(m, enabled, auto);
            }
        }
        emit(&self.handle, "settings-changed", &new);
        self.emit_perform();
        Ok(())
    }

    /// The clubs folder of the current library.
    pub fn clubs(&self) -> Clubs {
        Clubs::new(&self.store.library_dir())
    }

    /// One version of a club's guidelines (cached: versions don't change,
    /// except when edited in place, which calls `forget_clubs`).
    pub(crate) fn club(&self, r: &ClubRef) -> Option<Arc<Club>> {
        if let Some(c) = self.club_cache.lock().get(r) {
            return Some(c.clone());
        }
        let c = match self.clubs().club(&r.id, Some(&r.version)) {
            Ok(c) => Arc::new(c),
            Err(e) => {
                log::warn!("club {} ({}): {e:#}", r.id, r.version);
                return None;
            }
        };
        self.club_cache.lock().insert(r.clone(), c.clone());
        Some(c)
    }

    pub fn forget_clubs(&self) {
        self.club_cache.lock().clear();
    }

    /// Which library songs are songs a club lists for its styles (e.g. a
    /// choreography's accepted edits): track id → style ids. A song is one
    /// of them if its file hash, its Spotify track or its fingerprint (the
    /// same version, in any audio format) agrees.
    pub fn club_song_matches(&self, r: &ClubRef) -> HashMap<Id, Vec<String>> {
        let mut out: HashMap<Id, Vec<String>> = HashMap::new();
        for (style, entries) in self.club_song_entries(r) {
            for tracks in entries {
                for t in tracks {
                    let styles = out.entry(t).or_default();
                    if !styles.contains(&style) {
                        styles.push(style.clone());
                    }
                }
            }
        }
        out
    }

    /// For each style with suggested songs, each song's matching library
    /// tracks (in the style's song order).
    pub fn club_song_entries(&self, r: &ClubRef) -> HashMap<String, Vec<Vec<Id>>> {
        let mut out: HashMap<String, Vec<Vec<Id>>> = HashMap::new();
        let Some(club) = self.club(r) else { return out };
        let key = |s: &str| {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            s.hash(&mut h);
            h.finish()
        };
        // The club's songs, with their fingerprints read once.
        struct Wanted<'a> {
            style: &'a str,
            index: usize,
            song: &'a crate::club::ClubSong,
            fp: Option<(u64, Vec<u32>)>,
            spotify: Option<String>,
        }
        let wanted: Vec<Wanted> = club
            .styles
            .iter()
            .flat_map(|(id, st)| st.songs.iter().enumerate().map(move |(i, song)| (id, i, song)))
            .map(|(id, index, song)| Wanted {
                style: id,
                index,
                song,
                fp: club.fingerprint_of(song).and_then(|f| Some((key(f), crate::fingerprint::decode(f).ok()?))),
                spotify: song.spotify_id(),
            })
            .collect();
        for (id, st) in &club.styles {
            if !st.songs.is_empty() {
                out.insert(id.clone(), vec![vec![]; st.songs.len()]);
            }
        }
        if wanted.is_empty() {
            return out;
        }
        let item = crate::fingerprint::item_secs();
        for t in self.store.all_tracks() {
            let files: Vec<&str> = t.rec.sources.iter().filter(|s| s.kind == "file").map(|s| s.key.as_str()).collect();
            let spotify: Vec<String> =
                t.rec.sources.iter().filter(|s| s.kind == "spotify").filter_map(|s| crate::club::spotify_id(&s.key)).collect();
            let mut fp: Option<(u64, Vec<u32>)> = None;
            let secs = t.rec.duration_ms as f64 / 1000.0;
            for w in &wanted {
                let by_id = w.song.hash.as_deref().is_some_and(|h| files.contains(&h)) || w.spotify.as_ref().is_some_and(|s| spotify.contains(s));
                let by_fp = !by_id
                    && match (&w.fp, t.rec.fingerprint.as_deref()) {
                        (Some((wk, wfp)), Some(tf)) => {
                            // Different lengths can't be the same version: skip the comparison.
                            let wsecs = w.song.length.unwrap_or(wfp.len() as f64 * item);
                            if (wsecs - secs).abs() > crate::fingerprint::SAME_VERSION_LENGTH_SECS + 1.0 {
                                false
                            } else {
                                let tk = key(tf);
                                let known = self.fingerprint_matches.lock().get(&(tk, *wk)).copied();
                                known.unwrap_or_else(|| {
                                    let tfp = &fp.get_or_insert_with(|| (tk, crate::fingerprint::decode(tf).unwrap_or_default())).1;
                                    let same = crate::fingerprint::same_version(tfp, wfp);
                                    self.fingerprint_matches.lock().insert((tk, *wk), same);
                                    same
                                })
                            }
                        }
                        _ => false,
                    };
                if by_id || by_fp {
                    if let Some(e) = out.get_mut(w.style).and_then(|v| v.get_mut(w.index)) {
                        e.push(t.rec.id.clone());
                    }
                }
            }
        }
        out
    }

    /// The club (and version) new sets get: the default club's latest.
    pub(crate) fn default_club_ref(&self) -> Option<ClubRef> {
        let id = self.settings.read().default_club.clone();
        if id.is_empty() {
            return None;
        }
        Some(ClubRef { version: self.clubs().latest(&id)?, id })
    }

    /// The open set's club and version, else the default club's latest.
    pub(crate) fn active_club_ref(&self) -> Option<ClubRef> {
        let sid = self.perform.lock().setlist_id.clone();
        match sid.and_then(|s| self.store.setlist(&s).ok()) {
            Some(s) => s.club,
            None => self.default_club_ref(),
        }
    }

    pub(crate) fn active_club(&self) -> Option<Arc<Club>> {
        self.club(&self.active_club_ref()?)
    }

    /// Your styles and club mappings; seeded (and saved) on first use from the
    /// styles your library already uses, named after the active club profile.
    pub fn style_library(&self) -> StyleLibrary {
        if let Some(lib) = self.store.style_library() {
            return lib;
        }
        // No default styles: only those your songs already carry.
        let styles: Vec<LibraryStyle> = self
            .store
            .style_usage()
            .into_keys()
            .map(|key| LibraryStyle { name: crate::settings::prettify_key(&key), key, ..Default::default() })
            .collect();
        let lib = StyleLibrary { styles, ..Default::default() };
        if let Err(e) = self.store.save_style_library(lib.clone()) {
            log::warn!("saving styles.json: {e:#}");
        }
        lib
    }

    /// Your own name for a style key (used in messages; the UI resolves club names).
    pub fn style_name(&self, key: &str) -> String {
        self.style_library()
            .styles
            .iter()
            .find(|s| s.key == key)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| crate::settings::prettify_key(key))
    }

    // ------------------------------------------------------------ audio

    pub fn apply_outputs(&self) {
        let s = self.settings.read().audio.clone();
        let cfg = OutputConfig {
            master_device: s.master_device,
            master_channels: s.master_channels,
            cue_device: s.cue_device,
            cue_channels: s.cue_channels,
            buffer_ms: s.buffer_ms,
        };
        // Announcements go to the room and the headphones, like a deck.
        let master_taps = self.decks.iter().map(|d| d.shared.master_tap.clone()).chain([self.announcer.room.clone()]).collect();
        let cue_taps = self.decks.iter().map(|d| d.shared.cue_tap.clone()).chain([self.announcer.phones.clone()]).collect();
        let (rate, status) = self.outputs.apply(cfg, master_taps, cue_taps);
        self.rate.store(rate, Ordering::SeqCst);
        // Without a room output, let the headphone output pace playback.
        self.clock_cue.store(status.master.is_none() && status.cue.is_some(), Ordering::SeqCst);
        for d in &self.decks {
            d.reconfigure();
        }
        emit(&self.handle, "output-status", &status);
    }

    pub fn output_status(&self) -> OutputStatus {
        self.outputs.status.lock().clone()
    }

    pub(crate) fn fade_secs(&self) -> f64 {
        let s = self.settings.read();
        match s.fade_out_ms {
            Some(ms) => ms as f64 / 1000.0,
            None => {
                drop(s);
                self.active_club().and_then(|c| c.set.fade_out).filter(|f| *f > 0.0).unwrap_or(DEFAULT_FADE_SECS)
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
        if kind == "spotify" {
            self.analyse_spotify_once(&track.rec.id);
        }
        // Stem defaults: version / entry settings, e.g. {"vocals": false}.
        let stems_on: [bool; 4] = std::array::from_fn(|i| {
            r.stems
                .as_ref()
                .and_then(|m| m.get(crate::stems::STEMS[i]))
                .and_then(|v| v.as_bool())
                .unwrap_or(true)
        });
        let mut has_stems = false;
        let open: crate::audio::deck::SourceOpener = if kind == "spotify" {
            let dp = self.spotify.deck_player(deck)?;
            let dur = track.rec.duration_ms;
            let start = start_ms as f64 / 1000.0;
            Box::new(move || Ok(Box::new(SpotifySource::open(dp, &key, dur, start)?) as Box<dyn Source>))
        } else {
            let stem_files = self.store.file_info(&key).and_then(|f| self.stems.files(&f.hash));
            let path = PathBuf::from(key);
            match stem_files {
                Some(files) => {
                    has_stems = true;
                    let gains = self.decks[deck].shared.stem_gains.clone();
                    Box::new(move || {
                        let main = Box::new(LocalSource::open(&path)?) as Box<dyn Source>;
                        Ok(Box::new(crate::audio::source::StemSource::open(main, &files.paths, gains)?) as Box<dyn Source>)
                    })
                }
                None => Box::new(move || Ok(Box::new(LocalSource::open(&path)?) as Box<dyn Source>)),
            }
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
            cue_secs: None,
            has_stems,
            stems_on,
        })
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
            cue_secs: None,
            has_stems: false,
            stems_on: [true; 4],
        };
        if self.audible(deck) {
            return;
        }
        self.decks[deck].load(req);
    }

    pub fn deck_snapshots(&self) -> [DeckSnapshot; 2] {
        [self.decks[0].snapshot(), self.decks[1].snapshot()]
    }

    fn on_deck_event(&self, ev: DeckEvent) {
        match ev {
            DeckEvent::Ended { deck, .. } => self.on_deck_ended(deck),
            DeckEvent::Error { deck, message, .. } => {
                emit(&self.handle, "toast", format!("Deck {}: {message}", crate::perform::deck_name(deck)));
                if deck == self.live_deck() {
                    self.perform.lock().gap_started = Some(now_ms());
                }
                self.emit_perform();
            }
            DeckEvent::Loaded { .. } => {}
        }
    }

    // ------------------------------------------------------------ background

    /// Removes set-only songs nothing uses any more (keeping whatever is on a deck).
    pub fn prune(&self) {
        let keep: Vec<Id> = self.decks.iter().filter_map(|d| d.track_id()).collect();
        match self.store.prune_unused(&keep) {
            Ok(0) => {}
            Ok(n) => {
                log::info!("removed {n} unused set-only track(s)");
                emit(&self.handle, "library-changed", Vec::<Id>::new());
            }
            Err(e) => log::warn!("pruning set-only tracks: {e:#}"),
        }
    }

    /// Spotify songs aren't analysed on import (that would stream every
    /// song); a song without a waveform is analysed when first loaded.
    fn analyse_spotify_once(&self, id: &Id) {
        let current = self.store.track(id).is_ok_and(|t| t.rec.analysis_version >= crate::analysis::VERSION);
        if (current && self.store.waveform(id).is_some()) || !self.spotify_analysed.lock().insert(id.clone()) {
            return;
        }
        self.queue_analysis(id.clone(), true);
    }

    pub fn tts_status(&self) -> crate::tts::TtsStatus {
        self.tts.status(&self.settings.read().announce.tts_config())
    }

    pub fn emit_tts_status(&self) {
        emit(&self.handle, "tts-status", self.tts_status());
    }

    /// Logs audio problems since the last call: underruns (frames played as
    /// silence), frames dropped by a headphone copy, and late or slow output
    /// callbacks (heard as crackles although no deck ran out of audio).
    fn log_audio_problems(&self) {
        for (i, d) in self.decks.iter().enumerate() {
            for (name, tap) in [("room", &d.shared.master_tap), ("headphones", &d.shared.cue_tap)] {
                let deck = if i == 0 { "A" } else { "B" };
                let n = tap.underruns.swap(0, Ordering::Relaxed);
                if n > 0 {
                    log::warn!("deck {deck}: {n} frames of underrun on the {name} output");
                }
                let n = tap.dropped.swap(0, Ordering::Relaxed);
                if n > 0 {
                    log::warn!("deck {deck}: {n} frames dropped on the {name} output (its buffer was full)");
                }
            }
        }
        for (name, t) in ["room", "headphones"].iter().zip(&self.levels.timing) {
            let (late, slow) = (t.late.swap(0, Ordering::Relaxed), t.slow.swap(0, Ordering::Relaxed));
            let (gap, busy) = (t.max_gap_us.swap(0, Ordering::Relaxed), t.max_busy_us.swap(0, Ordering::Relaxed));
            if late > 0 || slow > 0 {
                log::warn!(
                    "{name} output: {late} late and {slow} slow callbacks (period {:.1} ms, longest gap {:.1} ms, longest callback {:.1} ms)",
                    t.period_us.load(Ordering::Relaxed) as f64 / 1000.0,
                    gap as f64 / 1000.0,
                    busy as f64 / 1000.0
                );
            }
        }
    }

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
                let mut tick: u64 = 0;
                loop {
                    std::thread::sleep(Duration::from_millis(50));
                    let Some(app) = weak.upgrade() else { return };
                    tick += 1;
                    app.apply_pending_seeks();
                    app.register_audible();
                    if tick % 5 == 0 {
                        app.auto_load();
                    }
                    if tick % 100 == 0 {
                        app.log_audio_problems();
                        app.tts.unload_if_idle();
                    }
                    let snaps = app.deck_snapshots();
                    let key = serde_json::to_string(&snaps).unwrap_or_default();
                    let lv = LevelsSnapshot {
                        master: app.levels.master_peak.get(),
                        cue: app.levels.cue_peak.get(),
                        master_volume: app.levels.master.get(),
                        cue_volume: app.levels.cue.get(),
                        decks: [app.decks[0].peak(), app.decks[1].peak()],
                    };
                    app.levels.master_peak.set(app.levels.master_peak.get() * 0.6);
                    app.levels.cue_peak.set(app.levels.cue_peak.get() * 0.6);
                    for d in &app.decks {
                        d.decay_peak(0.6);
                    }
                    let audible = lv.master > 0.001 || lv.cue > 0.001 || lv.decks.iter().any(|&p| p > 0.001);
                    if last.as_deref() != Some(&key) || audible {
                        emit(&app.handle, "deck-state", &snaps);
                        emit(&app.handle, "levels", &lv);
                        last = Some(key);
                    }
                    // Auto mode: pause, announcement, pause, next song.
                    app.auto_tick();
                    let armed = app.perform.lock().armed_until.map(|t| t > Instant::now()).unwrap_or(false);
                    if armed != last_armed {
                        last_armed = armed;
                        app.emit_perform();
                    }
                    if let Some(midi) = app.midi.get() {
                        if tick % 2 == 0 {
                            midi.set_leds(&app.led_states());
                        }
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
                .spawn(move || {
                    // Background work: never compete with the audio threads.
                    crate::audio::priority::lower_current_thread();
                    loop {
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
                    }
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
        let a = crate::analysis::analyze(&mono, sr, self.store.known_bpm(id));
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

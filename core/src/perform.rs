//! Performance logic: two symmetric decks, the crossfader, "Play Next", the
//! pause between songs, and how played songs become part of the set.
//!
//! Model:
//! - The crossfader decides which deck(s) the room hears. Play Next and Go Live
//!   move it to the new deck themselves (after the previous song has faded),
//!   so by default songs never overlap; a DJ can still crossfade by hand.
//! - The deck the room hears is *on air*; the other one is *free* and is used
//!   for cueing in headphones. The up-next song is auto-loaded onto the free deck.
//! - A song becomes part of the set (and the play history) only once it is
//!   audible in the room: playing on a deck with a non-zero crossfader gain.

use crate::app::{emit, App};
use crate::model::*;
use anyhow::{anyhow, Result};
use serde::Serialize;
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub enum PendingNext {
    /// The up-next entry.
    Next,
    /// A specific entry.
    Entry(Id),
    /// Whatever is loaded on this deck ("Go Live").
    Deck(usize),
}

/// A synthesized announcement: its samples (the chime first) and rate, and
/// how long its chime is (with the silence after it).
pub struct Prepared {
    pub samples: Arc<Vec<f32>>,
    pub rate: u32,
    pub chime_frames: usize,
}

impl Prepared {
    fn ms(&self, frames: usize) -> i64 {
        (frames as f64 * 1000.0 / self.rate.max(1) as f64) as i64
    }
    pub fn chime_ms(&self) -> i64 {
        self.ms(self.chime_frames)
    }
    pub fn speech_ms(&self) -> i64 {
        self.ms(self.samples.len() - self.chime_frames)
    }
}

/// A prepared announcement, or why there is none.
pub type Clip = std::result::Result<Prepared, String>;

/// Speech, roughly (characters per second at normal speed): to estimate an
/// announcement's length until it's synthesized.
const SPEECH_CHARS_PER_SEC: f64 = 14.0;

/// Crossfader position where the room hears both decks fully.
pub const CENTER: f64 = 0.5;

/// Pause between songs when neither you nor the club set one (seconds).
pub const DEFAULT_PAUSE_SECS: f64 = 8.0;

/// How long auto-load waits before trying again a song it couldn't load
/// (e.g. Spotify isn't connected).
const AUTO_LOAD_RETRY: Duration = Duration::from_secs(30);

/// How long the controller's lights flash when an action is refused.
pub const ERROR_FLASH: Duration = Duration::from_millis(1200);

/// Auto mode's progress through the pause before the next song:
/// song ends → pause → announcement → pause → next song.
#[derive(Default, Clone)]
pub enum AutoPhase {
    #[default]
    Idle,
    /// Synthesizing the announcement for `entry` (started when the pause
    /// begins); how long its chime and speech will be, roughly (ms).
    Preparing { entry: Id, clip: Arc<Mutex<Option<Clip>>>, estimate: (i64, i64) },
    /// Chiming and speaking from `started` until `until` (epoch ms); the
    /// first `chime_ms` are the chime.
    Speaking { entry: Id, started: i64, chime_ms: i64, until: i64 },
    /// The announcement finished (or failed) at the given time.
    Announced { entry: Id, at: i64 },
}

impl AutoPhase {
    fn entry(&self) -> Option<&Id> {
        match self {
            AutoPhase::Idle => None,
            AutoPhase::Preparing { entry, .. } | AutoPhase::Speaking { entry, .. } | AutoPhase::Announced { entry, .. } => Some(entry),
        }
    }
}

#[derive(Default)]
pub struct Perform {
    pub setlist_id: Option<Id>,
    /// Epoch ms when the on-air deck stopped (the pause between songs).
    pub gap_started: Option<i64>,
    /// What to start once the current fade-out completes.
    pub pending: Option<PendingNext>,
    /// "Play Next" was pressed once while a song was playing; press again to confirm.
    pub armed_until: Option<Instant>,
    pub auto_advance_override: Option<bool>,
    pub message: Option<String>,
    /// 0 = only deck A in the room, 1 = only deck B, 0.5 (the start) = both.
    /// Only the DJ moves it.
    pub crossfader: f64,
    /// Controller LEDs flash until then: an action was refused (see `check_crossfader`).
    pub error_flash_until: Option<Instant>,
    /// The deck the room heard last (the "current" song's deck), and whether
    /// it has heard either yet.
    pub live_deck: usize,
    pub heard_any: bool,
    /// Load generation of each deck's track once it has been counted as played.
    pub registered: [Option<u64>; 2],
    pub auto: AutoPhase,
    /// Last announcement pattern used (not repeated next time).
    pub last_pattern: Option<usize>,
    /// The set entry auto-load last failed to load, when, and why (it waits
    /// `AUTO_LOAD_RETRY` before trying it again).
    pub auto_load_failed: Option<(Id, Instant, String)>,
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
    /// Auto mode: `waiting` (pause), `announcing`, `held` (next entry waits for
    /// the DJ), or none.
    pub auto_phase: Option<String>,
    pub pending_next: bool,
    pub armed: bool,
    pub message: Option<String>,
    pub started_at: Option<i64>,
    pub crossfader: f64,
    pub live_deck: usize,
    /// Where songs load (see `App::free_deck`).
    pub free_deck: usize,
    /// The chime the next entry would have (whether or not it's on).
    pub next_chime: Option<Id>,
    /// Auto mode's steps to the next entry, in order (none unless it's on
    /// its way there).
    pub auto_steps: Vec<AutoStep>,
    /// What will go wrong at the next entry, as far as can be told now.
    pub next_problems: Vec<Problem>,
    /// The steps' times are from the end of the song playing (ms after it).
    pub steps_after_song: bool,
}

/// Something that will go wrong at the next entry: `stops` auto mode (it
/// can't start the song, or waits there), or just a warning.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub text: String,
    pub stops: bool,
}

/// A step of the pause before the next entry in auto mode: `pause`, `chime`,
/// `announcement` or `pauseAfter`, from and to when (epoch ms); a chime's or
/// announcement's length is `estimated` until it's ready (and so are the
/// times of the steps after it).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoStep {
    pub kind: String,
    pub from: i64,
    pub to: i64,
    pub estimated: bool,
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
    /// `song` or `pause` (a pause / announcement without a song).
    pub kind: EntryKind,
    pub track_id: Option<Id>,
    /// For cover art (`recording_artwork`).
    pub recording_id: Option<Id>,
    pub title: String,
    pub artist: String,
    /// What dancers call it (a custom label, else the styles it shows: the
    /// club's in a club's set), a short form, and its style's colour.
    pub dance: String,
    pub dance_short: String,
    pub color: String,
    /// Playing length (seconds).
    pub length_secs: Option<f64>,
    pub tags: Vec<ProjTag>,
    pub status: String,
    pub bpm: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectorPayload {
    pub title: String,
    pub mode: String,
    pub settings: crate::settings::ProjectorSettings,
    pub current: Option<ProjEntry>,
    pub next: Option<ProjEntry>,
    pub history: Vec<ProjEntry>,
    pub entries: Vec<ProjEntry>,
    pub current_index: Option<usize>,
    pub in_gap: bool,
    pub playing: bool,
    pub message: Option<String>,
    /// Deck whose position drives the projector's progress bar.
    pub live_deck: usize,
}

/// The deck the crossfader favors (none = centered; it snaps to the center).
fn crossfader_side(x: f64) -> Option<usize> {
    if x < CENTER {
        Some(0)
    } else if x > CENTER {
        Some(1)
    } else {
        None
    }
}

/// Room gain of `deck` for crossfader position `x` (0 = A, 1 = B). Both decks
/// are at full level in the middle ("dipless" curve).
pub fn crossfader_gain(deck: usize, x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    let g = if deck == 0 { 2.0 * (1.0 - x) } else { 2.0 * x };
    g.min(1.0)
}

impl App {
    // ------------------------------------------------------------ routing

    /// Applies the crossfader and headphone (PFL) state to the decks' taps.
    pub fn update_routing(&self) {
        let x = self.perform.lock().crossfader;
        for (i, d) in self.decks.iter().enumerate() {
            let g = crossfader_gain(i, x);
            d.shared.master_tap.gain.set(g as f32);
            d.shared.on_air.store(g > 0.001, std::sync::atomic::Ordering::Relaxed);
            d.shared.cue_tap.gain.set(if d.pfl() { 1.0 } else { 0.0 });
        }
    }

    /// Moved by the DJ (screen or controller). Within the snap distance of
    /// the middle it snaps to the exact center. Moving it off center toward
    /// a deck puts that deck on air like Go Live does for headphones.
    pub fn set_crossfader(&self, x: f64) {
        let snap = self.settings.read().crossfader_snap;
        let x = if (x - CENTER).abs() <= snap { CENTER } else { x.clamp(0.0, 1.0) };
        let before = std::mem::replace(&mut self.perform.lock().crossfader, x);
        self.update_routing();
        let (was, now) = (crossfader_side(before), crossfader_side(x));
        if let Some(deck) = now.filter(|_| now != was) {
            self.went_live(deck);
        }
    }

    pub fn set_crossfader_and_emit(&self, x: f64) {
        self.set_crossfader(x);
        self.emit_perform();
    }

    /// Automation (Play Next, Go Live, auto mode) is about to start `deck`.
    /// Orpheus never moves the crossfader: centered or toward `deck`, fine.
    /// Toward the other deck is the DJ's choice, so the start is refused,
    /// loudly: an error, and the controller's lights flash.
    pub fn check_crossfader(&self, deck: usize) -> Result<()> {
        let x = self.perform.lock().crossfader;
        if crossfader_side(x) != Some(1 - deck) {
            return Ok(());
        }
        self.perform.lock().error_flash_until = Some(Instant::now() + ERROR_FLASH);
        emit(&self.handle, "crossfader-blocked", deck);
        Err(anyhow!(
            "The crossfader is over to deck {}, so the room wouldn't hear deck {} fully. Center it for Play Next, Go Live and auto mode, or move it to deck {} yourself.",
            deck_name(1 - deck),
            deck_name(deck),
            deck_name(deck)
        ))
    }

    /// Whether the controller's lights are flashing for a refused action.
    pub fn error_flashing(&self) -> bool {
        self.perform.lock().error_flash_until.is_some_and(|t| t > Instant::now())
    }

    /// `deck` started for the room: optionally, headphones move to the other deck.
    fn went_live(&self, deck: usize) {
        if self.settings.read().headphones_follow_free_deck {
            self.decks[deck].set_pfl(false);
            self.decks[1 - deck].set_pfl(true);
            self.update_routing();
        }
    }

    /// Whether the room can hear this deck right now.
    pub fn audible(&self, deck: usize) -> bool {
        let d = &self.decks[deck];
        d.is_on_air() && matches!(d.status().as_str(), "playing" | "loading")
    }

    pub fn live_deck(&self) -> usize {
        self.perform.lock().live_deck
    }

    /// The deck songs load onto: the one the room heard less recently. Until
    /// the room has heard either, deck A if it's free, else deck B.
    pub fn free_deck(&self) -> usize {
        let (heard_any, live) = {
            let p = self.perform.lock();
            (p.heard_any, p.live_deck)
        };
        if heard_any {
            return 1 - live;
        }
        // The first song goes on A, unless A holds something else: a song
        // loaded or playing there that isn't a set's earlier auto-load (a set
        // entry, stopped, no longer next: auto-load replaces it).
        let a = &self.decks[0];
        let stale = !matches!(a.status().as_str(), "playing" | "loading") && a.entry_id().is_some_and(|e| self.next_entry().map_or(true, |n| n.id != e));
        if stale || matches!(a.status().as_str(), "empty" | "ended" | "error") { 0 } else { 1 }
    }

    /// The on-air deck is protected from scrubbing / cue jumps while playing.
    pub fn protected(&self, deck: usize) -> bool {
        self.settings.read().protect_on_air && self.audible(deck)
    }

    // ------------------------------------------------------------ setlist

    pub fn auto_advance(&self) -> bool {
        if let Some(v) = self.perform.lock().auto_advance_override {
            return v;
        }
        let s = self.settings.read().auto_advance;
        s.unwrap_or_else(|| self.active_club().and_then(|c| c.set.auto_advance).unwrap_or(false))
    }

    pub fn gap_seconds(&self) -> f64 {
        let s = self.settings.read().gap_seconds;
        s.unwrap_or_else(|| self.active_club().and_then(|c| c.set.pause.target).unwrap_or(DEFAULT_PAUSE_SECS))
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

    pub(crate) fn entries(&self) -> Vec<SetlistEntry> {
        let id = self.perform.lock().setlist_id.clone();
        id.map(|id| self.store.entries(&id)).unwrap_or_default()
    }

    /// The entry of the song the room is hearing (or heard last).
    pub fn current_entry_id(&self) -> Option<Id> {
        // Before the room has heard anything, a song cued on deck A isn't current.
        if !self.perform.lock().heard_any {
            return None;
        }
        let d = &self.decks[self.live_deck()];
        if d.status() == "empty" {
            return None;
        }
        d.entry_id()
    }

    /// The entry that would play next.
    pub fn next_entry(&self) -> Option<SetlistEntry> {
        let entries = self.entries();
        let current = self.current_entry_id();
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

    /// Creates a live set if no setlist is open.
    fn ensure_setlist(&self) -> Result<Id> {
        if let Some(id) = self.perform.lock().setlist_id.clone() {
            return Ok(id);
        }
        let date = chrono_like_date();
        let id = self.store.setlist_create(&format!("Live set — {date}"), "live", self.default_club_ref(), None)?;
        emit(&self.handle, "setlists-changed", ());
        self.open_setlist(Some(id.clone()))?;
        Ok(id)
    }

    // ------------------------------------------------------------ Play Next / Go Live

    /// "Play Next": start the up-next song on the free deck and put it on air.
    /// If a song is still audible it is faded out first.
    pub fn play_next(&self) -> Result<String> {
        self.request(PendingNext::Next)
    }

    pub fn play_entry(&self, entry_id: Id) -> Result<String> {
        self.request(PendingNext::Entry(entry_id))
    }

    /// Puts whatever is loaded on `deck` on air.
    pub fn go_live(&self, deck: usize) -> Result<String> {
        if !self.decks[deck].loaded() {
            return Err(anyhow!("Load a song onto deck {} first", deck_name(deck)));
        }
        self.request(PendingNext::Deck(deck))
    }

    fn request(&self, target: PendingNext) -> Result<String> {
        let target_deck = match &target {
            PendingNext::Deck(d) => *d,
            _ => self.free_deck(),
        };
        // Refuse before fading anything out.
        if let Some(deck) = self.deck_for(&target) {
            self.check_crossfader(deck)?;
        }
        // Something else audible? Fade it out first; never overlap by default.
        let busy: Vec<usize> = (0..2).filter(|&d| d != target_deck && self.audible(d)).collect();
        // In auto mode the next song still comes after its pause and
        // announcement: only fade out, and auto mode carries on from there.
        let after_pause = matches!(target, PendingNext::Next) && self.auto_advance();
        if !busy.is_empty() {
            let pending = (!after_pause).then(|| target.clone());
            if busy.iter().any(|&d| self.decks[d].is_fading()) {
                self.perform.lock().pending = pending;
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
                p.pending = pending;
            }
            let secs = self.settings.read().manual_next_fade_ms as f64 / 1000.0;
            for d in busy {
                self.decks[d].fade_out(secs);
            }
            self.emit_perform();
            return Ok("fading".into());
        }
        self.start(target, target_deck)?;
        Ok("started".into())
    }

    fn start(&self, target: PendingNext, deck: usize) -> Result<()> {
        match target {
            PendingNext::Deck(d) => {
                let dk = &self.decks[d];
                self.went_live(d);
                if dk.status() == "ended" {
                    dk.seek(dk.cue_secs());
                }
                dk.play();
            }
            PendingNext::Next | PendingNext::Entry(_) => {
                let e = match &target {
                    PendingNext::Entry(id) => self.store.entry(id)?.0,
                    _ => self.next_entry().ok_or_else(|| anyhow!("Nothing left in the setlist"))?,
                };
                self.start_entry(&e, deck)?;
                // A pause entry starts the pause before the following entry.
                if e.kind == EntryKind::Pause {
                    self.perform.lock().pending = None;
                    return Ok(());
                }
            }
        }
        {
            let mut p = self.perform.lock();
            p.pending = None;
            p.armed_until = None;
            p.gap_started = None;
            p.auto = AutoPhase::Idle;
        }
        self.announcer.stop();
        self.emit_perform();
        Ok(())
    }

    /// The deck `target` would start (none for a pause entry, or nothing to start).
    fn deck_for(&self, target: &PendingNext) -> Option<usize> {
        let entry = match target {
            PendingNext::Deck(d) => return Some(*d),
            PendingNext::Entry(id) => self.store.entry(id).ok()?.0,
            PendingNext::Next => self.next_entry()?,
        };
        (entry.kind != EntryKind::Pause).then(|| self.deck_for_entry(&entry, self.free_deck()))
    }

    /// Already loaded on the other deck (e.g. the DJ loaded it by hand)? Use that deck.
    fn deck_for_entry(&self, entry: &SetlistEntry, deck: usize) -> usize {
        if self.decks[1 - deck].entry_id().as_deref() == Some(&entry.id) && !self.audible(1 - deck) {
            1 - deck
        } else {
            deck
        }
    }

    /// Starts a setlist entry on `deck`, reusing it if it's already loaded there
    /// (so cue points / tempo set while cueing are kept).
    fn start_entry(&self, entry: &SetlistEntry, deck: usize) -> Result<()> {
        if entry.kind == EntryKind::Pause {
            // Play Next on a pause entry: say its announcement; it's done.
            if let Some(text) = self.announcement_for(entry) {
                self.speak(text, self.chime_for(entry));
            }
            return self.complete_pause(entry);
        }
        let version_id = entry.version_id.clone().ok_or_else(|| {
            let style = match (&entry.label, entry.styles.as_deref()) {
                (Some(l), _) => l.clone(),
                (None, Some(st)) if !st.is_empty() => st.iter().map(|s| self.style_name(s)).collect::<Vec<_>>().join(" / "),
                _ => "this".into(),
            };
            anyhow!("Choose a song for the {style} slot first")
        })?;
        let deck = self.deck_for_entry(entry, deck);
        let d = &self.decks[deck];
        let same = d.loaded()
            && d.status() != "loading"
            && (d.entry_id().as_deref() == Some(&entry.id)
                || (d.entry_id().is_none() && d.version_id().as_deref() == Some(&version_id)));
        self.went_live(deck);
        if same {
            d.set_entry_id(Some(entry.id.clone()));
            // Always start from the cue point (the version's start unless the DJ
            // set another with CUE), never from wherever cueing left the playhead.
            if (d.position_secs() - d.cue_secs()).abs() > 0.05 || d.status() != "paused" {
                d.seek(d.cue_secs());
            }
            d.play();
        } else {
            let (version, track_id) = self.store.version(&version_id)?;
            let track = self.store.track(&track_id)?;
            let req = self.load_request(deck, &track, &version, Some(entry), true)?;
            self.untake(&[&format!("deck{deck}.tempo")]);
            d.load(req);
        }
        Ok(())
    }

    /// Marks a deck's entry as played once it stops being the current song.
    fn finish_entry_on(&self, deck: usize) {
        if let Some(id) = self.decks[deck].entry_id() {
            if let Ok((e, sid)) = self.store.entry(&id) {
                if e.status == "playing" {
                    let _ = self.store.entry_update(&id, &EntryPatch { status: Some("played".into()), ..Default::default() });
                    self.emit_setlist(&sid);
                }
            }
        }
    }

    /// Counts songs as played once the room can hear them: marks the setlist
    /// entry as playing (inserting the song into the set if it wasn't in it)
    /// and logs it to the history. Called by the poller.
    pub fn register_audible(&self) {
        for deck in 0..2 {
            if !self.audible(deck) || self.decks[deck].status() != "playing" {
                continue;
            }
            let gen = self.decks[deck].load_generation();
            if self.perform.lock().registered[deck] == Some(gen) {
                continue;
            }
            let Some(version_id) = self.decks[deck].version_id() else {
                self.perform.lock().registered[deck] = Some(gen); // test tone etc.
                continue;
            };
            if let Err(e) = self.register(deck, &version_id) {
                log::warn!("registering played song: {e:#}");
            }
            self.perform.lock().registered[deck] = Some(gen);
        }
    }

    fn register(&self, deck: usize, version_id: &str) -> Result<()> {
        let other = 1 - deck;
        // The previous song is over unless the room still hears it (manual crossfade).
        if !self.audible(other) {
            self.finish_entry_on(other);
        }
        let setlist_id = self.ensure_setlist()?;
        let d = &self.decks[deck];
        let entry_id = match d.entry_id().filter(|id| self.store.entry(id).map(|(_, s)| s == setlist_id).unwrap_or(false)) {
            Some(id) => id,
            None => {
                // Played without being in the set: insert it after the current song.
                let entries = self.store.entries(&setlist_id);
                let cur = self.decks[other].entry_id();
                let at = cur
                    .and_then(|c| entries.iter().position(|e| e.id == c))
                    .or_else(|| entries.iter().rposition(|e| e.status != "pending"))
                    .map(|i| i + 1)
                    .unwrap_or(0);
                let ids = self.store.entries_insert(
                    &setlist_id,
                    Some(at),
                    &[NewEntry { version_id: Some(version_id.to_string()), ..Default::default() }],
                )?;
                d.set_entry_id(Some(ids[0].clone()));
                ids[0].clone()
            }
        };
        self.store.entry_set_playing(&entry_id)?;
        let (_, track_id) = self.store.version(version_id)?;
        {
            let mut p = self.perform.lock();
            p.live_deck = deck;
            p.heard_any = true;
            p.gap_started = None;
            p.auto = AutoPhase::Idle;
        }
        // Music is on: an announcement in progress gives way.
        self.announcer.stop();
        self.emit_setlist(&setlist_id);
        emit(&self.handle, "library-changed", vec![track_id]);
        self.emit_perform();
        Ok(())
    }

    /// Loads the up-next song onto the free deck (paused at its start) so it
    /// can be cued in headphones. Never replaces something the DJ loaded by hand.
    pub fn auto_load(&self) {
        if !self.settings.read().auto_load_next {
            return;
        }
        let Some(next) = self.next_entry() else { return };
        let Some(version_id) = next.version_id.clone() else { return };
        // Already loaded on either deck?
        if self.decks.iter().any(|d| d.entry_id().as_deref() == Some(&next.id) && d.loaded()) {
            return;
        }
        let free = self.free_deck();
        let d = &self.decks[free];
        let status = d.status();
        let replaceable = match status.as_str() {
            "empty" | "error" | "ended" => true,
            "loading" | "playing" => false,
            // Paused: only replace a stale auto-load (a set entry that isn't next).
            _ => d.entry_id().map(|e| e != next.id).unwrap_or(false) && !self.audible(free),
        };
        if !replaceable || self.perform.lock().pending.is_some() {
            return;
        }
        if self.perform.lock().auto_load_failed.as_ref().is_some_and(|(id, at, _)| *id == next.id && at.elapsed() < AUTO_LOAD_RETRY) {
            return;
        }
        let Ok((version, track_id)) = self.store.version(&version_id) else { return };
        let Ok(track) = self.store.track(&track_id) else { return };
        match self.load_request(free, &track, &version, Some(&next), false) {
            Ok(req) => {
                self.perform.lock().auto_load_failed = None;
                self.untake(&[&format!("deck{free}.tempo")]);
                d.load(req);
            }
            Err(e) => {
                log::warn!("auto-load: {e:#}");
                self.perform.lock().auto_load_failed = Some((next.id.clone(), Instant::now(), format!("{e:#}")));
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

    /// Fades out whatever the room hears.
    pub fn fade_out_air(&self) {
        let secs = self.fade_secs();
        for d in 0..2 {
            if self.audible(d) {
                self.decks[d].fade_out(secs);
            }
        }
    }

    pub(crate) fn on_deck_ended(&self, deck: usize) {
        self.finish_entry_on(deck);
        let was_live = deck == self.live_deck() && !self.audible(1 - deck);
        if !was_live {
            self.emit_perform();
            return;
        }
        let pending = self.perform.lock().pending.take();
        if let Some(t) = pending {
            if let Err(e) = self.request(t) {
                emit(&self.handle, "toast", format!("{e:#}"));
                self.perform.lock().gap_started = Some(now_ms());
            }
        } else {
            self.perform.lock().gap_started = Some(now_ms());
        }
        self.emit_perform();
    }

    // ------------------------------------------------------------ state for the UI

    // ------------------------------------------------------------ auto mode

    /// What `entry`'s announcement and chime come from: the entry, its set,
    /// its version and its song (see `chime.rs`).
    fn with_layers<R>(&self, entry: &SetlistEntry, f: impl FnOnce(&crate::chime::Layers) -> R) -> R {
        let set = self.store.entry(&entry.id).ok().and_then(|(_, sid)| self.store.setlist(&sid).ok());
        let vt = entry.version_id.as_ref().and_then(|v| self.store.version(v).ok()).and_then(|(v, tid)| Some((v, self.store.track(&tid).ok()?.rec)));
        f(&crate::chime::Layers { entry: Some(entry), set: set.as_ref(), version: vt.as_ref().map(|x| &x.0), song: vt.as_ref().map(|x| &x.1) })
    }

    /// Whether auto mode will say something before `entry`.
    fn has_announcement(&self, entry: &SetlistEntry) -> bool {
        let a = self.settings.read().announce.clone();
        self.with_layers(entry, |l| l.speaks(&a))
    }

    /// The chime chosen for `entry`, whether or not it's on.
    fn chime_version_for(&self, entry: &SetlistEntry) -> Option<Id> {
        let a = self.settings.read().announce.clone();
        self.with_layers(entry, |l| l.chime_sound(&a))
    }

    /// The chime to play for the next entry now (manual button, even with chimes off).
    pub fn next_chime(&self) -> Result<Id> {
        let next = self.next_entry().ok_or_else(|| anyhow!("Nothing is coming up"))?;
        self.chime_version_for(&next).ok_or_else(|| anyhow!("No chime is chosen for the next song"))
    }

    /// The chime before `entry`'s announcement, if one plays.
    fn chime_for(&self, entry: &SetlistEntry) -> crate::chime::Chime {
        let a = self.settings.read().announce.clone();
        self.with_layers(entry, |l| l.chime(&a))
    }

    /// What is said before `entry`: the announcement chosen for it (by the
    /// entry, its version or its song; empty = nothing), else, for songs with
    /// announcements on, a random pattern (never the same twice in a row).
    /// `{dance}` becomes the dance's name.
    fn announcement_for(&self, entry: &SetlistEntry) -> Option<String> {
        if !self.has_announcement(entry) {
            return None;
        }
        let name = self.dance_name(entry);
        let fill = |t: &str| -> Option<String> {
            if t.contains("{dance}") {
                Some(t.replace("{dance}", name.as_deref()?))
            } else {
                Some(t.to_string())
            }
        };
        if let Some(t) = self.with_layers(entry, |l| l.announcement().map(String::from)) {
            return fill(t.trim());
        }
        let patterns: Vec<String> = self.settings.read().announce.patterns.iter().filter(|p| !p.trim().is_empty()).cloned().collect();
        let n = patterns.len();
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos() as usize).unwrap_or(0);
        let mut i = seed % n;
        let last = self.perform.lock().last_pattern;
        if n > 1 && last == Some(i) {
            i = (i + 1 + seed / n % (n - 1)) % n;
        }
        let text = fill(&patterns[i])?;
        self.perform.lock().last_pattern = Some(i);
        Some(text)
    }

    /// Pauses (seconds) before `entry`'s announcement and between it and the
    /// entry's start; the entry's own values override the defaults.
    fn entry_pauses(&self, entry: &SetlistEntry) -> (f64, f64) {
        let a = self.settings.read().announce.clone();
        let speaks = self.has_announcement(entry);
        let with_announcements = speaks || a.enabled;
        let before = entry.pause_before.unwrap_or(if with_announcements { a.pause_before_secs } else { self.gap_seconds() });
        let after = if with_announcements { entry.pause_after.unwrap_or(a.pause_after_secs) } else { 0.0 };
        (before, after)
    }

    /// What dancers call an entry: its label, else its version's display name,
    /// else the styles it shows (the club's names in a club's set).
    fn dance_name(&self, entry: &SetlistEntry) -> Option<String> {
        let set = self.perform.lock().setlist_id.clone().and_then(|id| self.store.setlist(&id).ok());
        let ctx = self.view_context(set.and_then(|s| s.club));
        let found = entry.version_id.as_ref().and_then(|v| self.store.version(v).ok()).and_then(|(v, tid)| Some((self.store.track(&tid).ok()?, v)));
        let shown = match &found {
            Some((t, v)) => crate::views::shown_for(&ctx, t, v, Some(entry)),
            None => crate::views::shown_styles(&ctx, None, None, Some(entry)),
        };
        let custom = crate::views::custom_label(found.as_ref().map(|(_, v)| v), Some(entry));
        let name = crate::views::label_of(&shown, custom.as_deref(), " or ");
        (!name.is_empty()).then_some(name)
    }

    /// What the chime version plays: its audio file, from its start to its
    /// end (or through its fade), at its gain.
    fn chime_excerpt(&self, version_id: &str) -> Result<crate::chime::Excerpt> {
        let (v, tid) = self.store.version(version_id).map_err(|_| anyhow!("its version was removed"))?;
        let track = self.store.track(&tid)?;
        let path = crate::library::resolve_path(&self.store, &v.recording).ok_or_else(|| anyhow!("its audio isn't a file on this computer"))?;
        let st = Self::resolve_settings(&v, None);
        let secs = |ms: i64| ms as f64 / 1000.0;
        // Like a deck: it fades out from its fade point, else stops at its end.
        let fade = st.fade_out_ms.map(|_| self.fade_secs()).unwrap_or(0.0);
        let (keylock, engine) = {
            let s = self.settings.read();
            (s.keylock, s.stretch_engine.clone())
        };
        Ok(crate::chime::Excerpt {
            path,
            start_secs: st.cue_in_ms.map_or(0.0, secs),
            end_secs: st.fade_out_ms.map(|f| secs(f) + fade).or(st.cue_out_ms.map(secs)),
            fade_secs: fade,
            gain: 10f32.powf(st.gain_db.unwrap_or(0.0) as f32 / 20.0),
            rate: crate::views::playback(track.recording_of(&v.id), &v, None, None).rate,
            keylock,
            stretch: crate::audio::stretch::Engine::from_setting(&engine),
        })
    }

    /// A chime version in words: its song's title, and its name if that tells it apart.
    pub fn chime_label(&self, version_id: &str) -> String {
        match self.store.version(version_id).ok().and_then(|(v, tid)| Some((v, self.store.track(&tid).ok()?))) {
            Some((v, t)) if t.rec.versions.len() > 1 || v.name != "Original" => format!("{} — {}", t.rec.title, v.name),
            Some((_, t)) => t.rec.title,
            None => "A removed version".into(),
        }
    }

    /// The chime's samples at `rate` (none if it can't be played: then a message).
    fn chime_samples(handle: &crate::app::Events, excerpt: &Result<crate::chime::Excerpt>, rate: u32) -> Option<Vec<f32>> {
        match excerpt.as_ref().map_err(|e| anyhow!("{e}")).and_then(|x| crate::chime::render(x, rate)) {
            Ok(s) => Some(s),
            Err(e) => {
                emit(handle, "toast", format!("The chime can't play ({e:#})."));
                None
            }
        }
    }

    /// Synthesizes `text`, with `chime` before it (to run in a background thread).
    fn clip_job(&self, text: String, chime: crate::chime::Chime) -> impl FnOnce() -> Clip + Send + 'static {
        let (tts, cfg) = (self.tts.clone(), self.settings.read().announce.tts_config());
        let chime = chime.map(|c| self.chime_excerpt(&c));
        let prons = self.style_library().pronunciations;
        let handle = self.handle.clone();
        log::info!("announcement: {text}");
        move || {
            let (speech, rate) = tts.synthesize(&cfg, &text, &prons).map_err(|e| format!("{e:#}"))?;
            let Some(chime) = chime.and_then(|x| Self::chime_samples(&handle, &x, rate)) else { return Ok(Prepared { samples: speech, rate, chime_frames: 0 }) };
            let samples = crate::chime::before(chime, &speech, rate);
            Ok(Prepared { chime_frames: samples.len() - speech.len(), samples: Arc::new(samples), rate })
        }
    }

    /// Speaks `text` now, after `chime` (in a background thread).
    pub fn speak(&self, text: String, chime: crate::chime::Chime) {
        let volume = self.settings.read().announce.volume as f32;
        let announcer = self.announcer.clone();
        let handle = self.handle.clone();
        let job = self.clip_job(text, chime);
        std::thread::spawn(move || match job() {
            Ok(p) => {
                announcer.play(&p.samples, p.rate, volume);
            }
            Err(e) => emit(&handle, "toast", format!("Announcement: {e}")),
        });
    }

    /// Plays a chime alone (to try it).
    pub fn preview_chime(&self, version_id: &str) {
        let volume = self.settings.read().announce.volume as f32;
        if let Some(samples) = Self::chime_samples(&self.handle, &self.chime_excerpt(version_id), crate::chime::PREVIEW_RATE) {
            self.announcer.play(&samples, crate::chime::PREVIEW_RATE, volume);
        }
    }

    /// Announces the next entry now (manual button): its announcement, or a pattern.
    pub fn announce_next(&self) -> Result<()> {
        let next = self.next_entry().ok_or_else(|| anyhow!("Nothing is coming up"))?;
        let text = match self.announcement_for(&next) {
            Some(t) => t,
            None => {
                // The button works even with announcements off in auto mode.
                let patterns = self.settings.read().announce.patterns.clone();
                let name = self.dance_name(&next);
                match (patterns.iter().find(|p| !p.trim().is_empty()), name) {
                    (Some(p), Some(n)) => p.replace("{dance}", &n),
                    _ => return Err(anyhow!("Nothing to announce for the next entry")),
                }
            }
        };
        self.speak(text, self.chime_for(&next));
        Ok(())
    }

    /// A pause entry is reached: it is done (its pause and announcement
    /// already happened); the next entry's pause starts now.
    fn complete_pause(&self, entry: &SetlistEntry) -> Result<()> {
        self.store.entry_update(&entry.id, &EntryPatch { status: Some("played".into()), ..Default::default() })?;
        {
            let mut p = self.perform.lock();
            p.gap_started = Some(now_ms());
            p.auto = AutoPhase::Idle;
        }
        // (Bind first: the lock must not be held while emitting.)
        let sid = self.perform.lock().setlist_id.clone();
        if let Some(sid) = sid {
            self.emit_setlist(&sid);
        }
        self.emit_perform();
        Ok(())
    }

    /// Called by the poller: moves auto mode through the pause before the next
    /// entry: pause → announcement → pause → start (or, for a pause entry, done).
    pub fn auto_tick(&self) {
        let (gap, pending) = {
            let p = self.perform.lock();
            (p.gap_started, p.pending.is_some())
        };
        let Some(g) = gap else { return };
        if pending || !self.auto_advance() || (0..2).any(|d| self.audible(d)) {
            return;
        }
        let Some(next) = self.next_entry() else { return };
        let is_pause = next.kind == EntryKind::Pause;
        if (!is_pause && next.version_id.is_none()) || next.hold {
            return;
        }
        let (before, after) = self.entry_pauses(&next);
        let now = now_ms();
        let phase = self.perform.lock().auto.clone();
        // A different entry is next now (reordered): start over for it.
        let phase = if phase.entry().is_some_and(|e| *e != next.id) { AutoPhase::Idle } else { phase };
        match phase {
            AutoPhase::Idle => {
                // Prepare the announcement while the first pause runs.
                let clip = Arc::new(Mutex::new(None));
                let text = self.announcement_for(&next);
                let estimate = self.announcement_estimate(&next, text.as_deref());
                match text {
                    Some(text) => {
                        let (job, slot) = (self.clip_job(text, self.chime_for(&next)), clip.clone());
                        std::thread::spawn(move || *slot.lock() = Some(job()));
                    }
                    // Nothing to say (or no dance to name): just pause.
                    None => *clip.lock() = Some(Err(String::new())),
                }
                self.perform.lock().auto = AutoPhase::Preparing { entry: next.id.clone(), clip, estimate };
                self.emit_perform();
            }
            AutoPhase::Preparing { entry, clip, estimate } => {
                // Ready: the panel learns how long it really is (once).
                let ready = clip.lock().as_ref().map(|r| r.as_ref().map_or((0, 0), |p| (p.chime_ms(), p.speech_ms())));
                if let Some(len) = ready.filter(|l| *l != estimate) {
                    if let AutoPhase::Preparing { estimate: e, .. } = &mut self.perform.lock().auto {
                        *e = len;
                    }
                    self.emit_perform();
                }
                if now - g < (before * 1000.0) as i64 {
                    return;
                }
                let Some(result) = clip.lock().take() else { return };
                // Don't announce a song the crossfader wouldn't let the room hear.
                if let Some(deck) = self.deck_for(&PendingNext::Next) {
                    if let Err(e) = self.check_crossfader(deck) {
                        return self.stop_auto(e);
                    }
                }
                let phase = match result {
                    Ok(p) => {
                        let volume = self.settings.read().announce.volume as f32;
                        let len = self.announcer.play(&p.samples, p.rate, volume);
                        AutoPhase::Speaking { entry, started: now, chime_ms: p.chime_ms(), until: now + len.as_millis() as i64 }
                    }
                    Err(e) => {
                        if !e.is_empty() {
                            emit(&self.handle, "toast", format!("Announcement skipped: {e}"));
                        }
                        AutoPhase::Announced { entry, at: now }
                    }
                };
                self.perform.lock().auto = phase;
                self.emit_perform();
            }
            AutoPhase::Speaking { entry, until, .. } => {
                if now >= until && !self.announcer.speaking() {
                    self.perform.lock().auto = AutoPhase::Announced { entry, at: now };
                    self.emit_perform();
                }
            }
            AutoPhase::Announced { at, .. } => {
                if now - at < (after * 1000.0) as i64 {
                    return;
                }
                let r = if is_pause { self.complete_pause(&next) } else { self.play_next().map(|_| ()) };
                if let Err(e) = r {
                    self.stop_auto(e);
                }
            }
        }
    }

    /// Auto mode can't go on (the next song can't start): it stops, says why,
    /// and the pause ends (the DJ goes on with Play Next).
    fn stop_auto(&self, why: anyhow::Error) {
        emit(&self.handle, "toast", format!("Auto mode stopped: {why:#}"));
        {
            let mut p = self.perform.lock();
            p.gap_started = None;
            p.auto = AutoPhase::Idle;
        }
        self.emit_perform();
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
        let (crossfader, live_deck) = (p.crossfader, p.live_deck);
        let auto_phase_now = p.auto.clone();
        drop(p);
        let entries = self.entries();
        let started_at = entries.iter().filter_map(|e| e.played_at).min();
        let next = self.next_entry();
        let next_chime = next.as_ref().and_then(|e| self.chime_version_for(e));
        let next_problems = next.as_ref().map(|e| self.problems_at(e, crossfader)).unwrap_or_default();
        let next_ready = next.as_ref().map(|e| e.version_id.is_some() || e.kind == EntryKind::Pause).unwrap_or(false);
        let next_held = next.as_ref().is_some_and(|e| e.hold);
        // In the pause: its steps, when. While a song plays: those after it,
        // from its end (`steps_after_song`: the panel knows when that is).
        let playing = (0..2).any(|d| self.audible(d));
        let steps_after_song = gap.is_none() && playing;
        let steps = match (&next, gap) {
            (Some(e), Some(g)) if auto && next_ready && !next_held => self.auto_steps(e, g, &auto_phase_now),
            (Some(e), None) if auto && next_ready && !next_held && playing => self.auto_steps(e, 0, &AutoPhase::Idle),
            _ => vec![],
        };
        // When the next entry starts (estimated until its announcement is ready).
        let auto_at = steps.last().filter(|_| !steps_after_song).map(|s| s.to);
        let auto_phase = match (&auto_phase_now, gap) {
            _ if !auto || next.is_none() => None,
            _ if next_held => Some("held".to_string()),
            (AutoPhase::Speaking { .. }, Some(_)) => Some("announcing".to_string()),
            (_, Some(_)) if next_ready => Some("waiting".to_string()),
            _ => None,
        };
        PerformState {
            setlist_id,
            mode,
            current_entry_id: self.current_entry_id(),
            next_entry_id: next.map(|e| e.id),
            gap_started_at: gap,
            gap_seconds: gap_secs,
            auto_advance: auto,
            auto_advance_at: if auto && next_ready && !next_held { auto_at } else { None },
            auto_phase,
            pending_next: pending,
            armed,
            message,
            started_at,
            crossfader,
            live_deck,
            free_deck: self.free_deck(),
            next_chime,
            auto_steps: steps,
            steps_after_song,
            next_problems,
        }
    }

    /// What will go wrong when `entry` comes up, as far as can be told now
    /// (`crossfader`: where it is): what stops auto mode first, then warnings.
    /// It changes nothing (unlike starting it, which these checks mirror).
    fn problems_at(&self, entry: &SetlistEntry, crossfader: f64) -> Vec<Problem> {
        let mut out = vec![];
        let stop = |text: String| Problem { text, stops: true };
        if entry.kind == EntryKind::Song {
            match &entry.version_id {
                None => out.push(stop("It's a slot with no song chosen: auto mode waits there (choose one, or skip it).".into())),
                Some(vid) => {
                    let found = self.store.version(vid).ok().and_then(|(v, tid)| Some((v, self.store.track(&tid).ok()?)));
                    match found {
                        None => out.push(stop("Its song is no longer in the library.".into())),
                        Some((v, t)) => match t.recording_of(&v.id) {
                            None => out.push(stop(format!("The audio of \"{}\" isn't in the library.", t.rec.title))),
                            Some(r) if r.rec.removed => out.push(stop(format!("The audio of \"{}\" was removed from the library.", t.rec.title))),
                            Some(r) => match self.choose_source(r, &t.rec.title) {
                                Err(e) => out.push(stop(format!("{e:#}"))),
                                Ok((kind, _)) if kind == "spotify" && !self.spotify.is_connected() => {
                                    out.push(stop(format!("\"{}\" plays from Spotify, and Spotify isn't connected.", t.rec.title)))
                                }
                                Ok(_) => {}
                            },
                        },
                    }
                    if let Some((_, _, why)) = self.perform.lock().auto_load_failed.as_ref().filter(|(id, _, _)| *id == entry.id) {
                        if out.is_empty() {
                            out.push(stop(format!("It couldn't be loaded onto a deck: {why}")));
                        }
                    }
                    let deck = self.deck_for_entry(entry, self.free_deck());
                    if crossfader_side(crossfader) == Some(1 - deck) {
                        out.push(stop(format!("The crossfader is over to deck {}, and it starts on deck {}: center the crossfader.", deck_name(1 - deck), deck_name(deck))));
                    }
                }
            }
        }
        let a = self.settings.read().announce.clone();
        if self.has_announcement(entry) {
            if !self.tts.installed(&a.tts_config()) {
                out.push(Problem { text: "Nothing will be said before it: the announcer's voice isn't downloaded (Settings → Auto Mode).".into(), stops: false });
            } else if self.with_layers(entry, |l| l.chime_enabled(&a)) {
                match self.chime_for(entry) {
                    None => out.push(Problem { text: "Its chime is on, but no version is chosen to play.".into(), stops: false }),
                    Some(v) => {
                        if let Err(e) = self.chime_excerpt(&v) {
                            out.push(Problem { text: format!("Its chime can't play ({e:#})."), stops: false });
                        }
                    }
                }
            }
        }
        out
    }

    /// Roughly how long `entry`'s chime and announcement (`text`, or a
    /// pattern) will take (ms), until they're ready.
    fn announcement_estimate(&self, entry: &SetlistEntry, text: Option<&str>) -> (i64, i64) {
        // Nothing is said without the voice (and the chime comes only with words).
        if !self.has_announcement(entry) || !self.tts.installed(&self.settings.read().announce.tts_config()) {
            return (0, 0);
        }
        let (speed, pattern) = {
            let a = &self.settings.read().announce;
            (a.speed, a.patterns.iter().find(|p| !p.trim().is_empty()).cloned().unwrap_or_default())
        };
        let chars = text.map_or(pattern.len(), str::len) as f64;
        let speech = chars / (SPEECH_CHARS_PER_SEC * if speed > 0.0 { speed } else { 1.0 }) * 1000.0;
        (self.chime_estimate_ms(entry), speech as i64)
    }

    /// How long `entry`'s chime takes (ms; none if it has none or can't play).
    fn chime_estimate_ms(&self, entry: &SetlistEntry) -> i64 {
        let Some(vid) = self.chime_for(entry) else { return 0 };
        let Ok(x) = self.chime_excerpt(&vid) else { return 0 };
        let file_secs = self.store.version(&vid).ok().and_then(|(v, _)| self.store.recording(&v.recording).ok()).map_or(0.0, |r| r.rec.duration_ms as f64 / 1000.0);
        let end = x.end_secs.unwrap_or_else(|| file_secs.min(x.start_secs + crate::chime::MAX_OPEN_SECS * x.rate));
        (((end - x.start_secs).max(0.0) / x.rate.max(0.01) + crate::chime::GAP_SECS as f64) * 1000.0) as i64
    }

    /// Auto mode's steps to `entry`, the pause having started at `g`: the
    /// pause, its chime and announcement, the pause after, each with when it
    /// starts and ends (estimated until the announcement is ready).
    fn auto_steps(&self, entry: &SetlistEntry, g: i64, phase: &AutoPhase) -> Vec<AutoStep> {
        let (before, after) = self.entry_pauses(entry);
        let (before, after) = ((before * 1000.0) as i64, (after * 1000.0) as i64);
        let now = now_ms();
        let pause_end = g + before;
        let mut steps = vec![AutoStep { kind: "pause".into(), from: g, to: pause_end, estimated: false }];
        // When the chime and speech start, how long they are, and whether that's known.
        let (start, chime, speech, estimated, done) = match phase {
            AutoPhase::Speaking { started, chime_ms, until, entry: e } if *e == entry.id => (*started, *chime_ms, until - started - chime_ms, false, None),
            AutoPhase::Announced { at, entry: e } if *e == entry.id => (*at, 0, 0, false, Some(*at)),
            AutoPhase::Preparing { clip, estimate, entry: e } if *e == entry.id => {
                // It starts once the pause is over and it's ready.
                let start = pause_end.max(now);
                match clip.lock().as_ref() {
                    Some(Ok(p)) => (start, p.chime_ms(), p.speech_ms(), false, None),
                    Some(Err(_)) => (start, 0, 0, false, None),
                    None => (start, estimate.0, estimate.1, true, None),
                }
            }
            _ => {
                let (c, s) = self.announcement_estimate(entry, None);
                (pause_end, c, s, true, None)
            }
        };
        if chime > 0 {
            steps.push(AutoStep { kind: "chime".into(), from: start, to: start + chime, estimated });
        }
        if speech > 0 {
            steps.push(AutoStep { kind: "announcement".into(), from: start + chime, to: start + chime + speech, estimated });
        }
        let end = done.unwrap_or(start + chime + speech);
        if steps.len() == 1 {
            // Nothing said: one pause, to the end of both.
            steps[0].to = steps[0].to.max(end) + after;
        } else if after > 0 {
            // Its length is known; only when it starts may not be.
            steps.push(AutoStep { kind: "pauseAfter".into(), from: end, to: end + after, estimated: false });
        }
        steps
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

    fn proj_entry(&self, e: &SetlistEntry, ctx: &crate::views::ViewContext, lookup: &HashMap<Id, (Version, Track)>) -> ProjEntry {
        let club = ctx.club.as_deref();
        let vt = e.version_id.as_ref().and_then(|id| lookup.get(id));
        let shown = match (vt, e.kind) {
            (_, EntryKind::Pause) => vec![],
            (Some((v, t)), _) => crate::views::shown_for(ctx, t, v, Some(e)),
            (None, _) => crate::views::shown_styles(ctx, None, None, Some(e)),
        };
        // Entry label, else the version's display name (e.g. a line dance's name).
        let custom = crate::views::custom_label(vt.map(|(v, _)| v), Some(e)).or_else(|| (e.kind == EntryKind::Pause).then(|| "Pause".to_string()));
        let shorts: Vec<crate::views::Shown> = shown.iter().map(|s| crate::views::Shown { name: s.short.clone(), ..s.clone() }).collect();
        let settings = vt.map(|(v, _)| Self::resolve_settings(v, Some(e)));
        let target = settings.as_ref().and_then(|s| s.target_bpm);
        let rec = vt.and_then(|(v, t)| t.recording_of(&v.id)).map(|r| &r.rec);
        // From the cue-in point to the fade-out / cue-out point (or the end).
        let length_secs = rec.zip(settings.as_ref()).map(|(r, s)| {
            let end = s.fade_out_ms.or(s.cue_out_ms).unwrap_or(r.duration_ms);
            (end - s.cue_in_ms.unwrap_or(0)).max(0) as f64 / 1000.0
        });
        ProjEntry {
            id: e.id.clone(),
            kind: e.kind,
            track_id: vt.map(|(_, t)| t.rec.id.clone()),
            recording_id: rec.map(|r| r.id.clone()),
            title: vt.map(|(_, t)| t.rec.title.clone()).unwrap_or_default(),
            artist: vt.map(|(_, t)| t.rec.artist.clone()).unwrap_or_default(),
            dance: crate::views::label_of(&shown, custom.as_deref(), " / "),
            dance_short: crate::views::label_of(&shorts, custom.as_deref(), " / "),
            color: shown.first().map_or_else(|| crate::views::NEUTRAL.to_string(), |s| s.color.clone()),
            length_secs,
            tags: e
                .tags
                .iter()
                .filter_map(|tid| club.and_then(|c| c.tags.get(tid)).filter(|t| t.announce).map(|t| (tid, t)))
                .map(|(id, t)| ProjTag { id: id.clone(), name: t.name.clone(), color: t.color.clone().unwrap_or_default() })
                .collect(),
            status: e.status.clone(),
            bpm: target.or_else(|| rec.and_then(|r| r.bpm)),
        }
    }

    pub fn projector_payload(&self) -> ProjectorPayload {
        let settings = self.settings.read().projector.clone();
        let club = self.active_club();
        let (setlist_id, gap, message, live_deck) = {
            let p = self.perform.lock();
            (p.setlist_id.clone(), p.gap_started, p.message.clone(), p.live_deck)
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
        let ctx = self.view_context(None);
        let proj: Vec<ProjEntry> = entries.iter().map(|e| self.proj_entry(e, &ctx, &lookup)).collect();
        // Only songs the room has heard count as "current".
        let current_id = self.current_entry_id().filter(|id| {
            entries.iter().any(|e| &e.id == id && (e.status == "playing" || e.status == "played"))
        });
        let current_index = current_id.as_ref().and_then(|c| entries.iter().position(|e| &e.id == c));
        let playing = self.audible(live_deck) && self.decks[live_deck].status() == "playing";
        let current = current_index.map(|i| proj[i].clone());
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
        history.retain(|h| !h.title.is_empty() || !h.dance.is_empty());
        let mode = match settings.display_mode.as_str() {
            "live" | "preset" => settings.display_mode.clone(),
            _ => setlist.as_ref().map(|s| s.mode.clone()).unwrap_or_else(|| "live".into()),
        };
        let title = if settings.title.trim().is_empty() {
            setlist.as_ref().map(|s| s.name.clone()).unwrap_or_else(|| club.as_ref().map(|c| c.name.clone()).unwrap_or_default())
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
            live_deck,
        }
    }
}

pub fn deck_name(d: usize) -> &'static str {
    if d == 0 { "A" } else { "B" }
}

/// "Oct 13, 2026" without pulling in a date crate.
fn chrono_like_date() -> String {
    const M: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let days = now_ms().div_euclid(86_400_000);
    let (y, m, d) = crate::store::civil_from_days(days);
    format!("{} {d}, {y}", M[(m - 1) as usize])
}

#[cfg(test)]
mod tests {
    use super::crossfader_gain;

    #[test]
    fn crossfader_curve() {
        assert_eq!(crossfader_gain(0, 0.0), 1.0);
        assert_eq!(crossfader_gain(1, 0.0), 0.0);
        assert_eq!(crossfader_gain(0, 0.5), 1.0);
        assert_eq!(crossfader_gain(1, 0.5), 1.0);
        assert_eq!(crossfader_gain(0, 1.0), 0.0);
        assert!((crossfader_gain(1, 0.25) - 0.5).abs() < 1e-9);
    }
}

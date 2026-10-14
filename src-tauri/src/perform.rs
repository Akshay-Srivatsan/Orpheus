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
use crate::settings::ProfileCore;
use anyhow::{anyhow, Result};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
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
    /// 0 = only deck A in the room, 1 = only deck B.
    pub crossfader: f64,
    /// The deck the room heard last (the "current" song's deck).
    pub live_deck: usize,
    /// Load generation of each deck's track once it has been counted as played.
    pub registered: [Option<u64>; 2],
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
    pub crossfader: f64,
    pub live_deck: usize,
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
    /// For cover art (`track_artwork`).
    pub track_id: Option<Id>,
    pub title: String,
    pub artist: String,
    /// Your style keys to show (club styles are resolved by the page).
    pub styles: Vec<String>,
    /// Custom text replacing the style names.
    pub label: Option<String>,
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
    /// The club profile (for its style names), and your styles and mappings.
    pub profile: serde_json::Value,
    pub style_library: StyleLibrary,
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

    pub fn set_crossfader(&self, x: f64) {
        self.perform.lock().crossfader = x.clamp(0.0, 1.0);
        self.update_routing();
    }

    /// Puts `deck` fully on air (and, optionally, headphones on the other deck).
    fn take_air(&self, deck: usize) {
        self.set_crossfader(if deck == 0 { 0.0 } else { 1.0 });
        self.untake(&["cue.crossfader"]);
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

    pub fn free_deck(&self) -> usize {
        1 - self.live_deck()
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

    pub(crate) fn entries(&self) -> Vec<SetlistEntry> {
        let id = self.perform.lock().setlist_id.clone();
        id.map(|id| self.store.entries(&id)).unwrap_or_default()
    }

    /// The entry of the song the room is hearing (or heard last).
    pub fn current_entry_id(&self) -> Option<Id> {
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
        let s = self.settings.read().clone();
        let date = chrono_like_date();
        let id = self.store.setlist_create(&format!("Live set — {date}"), "live", &s.active_profile_id, None)?;
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
        // Something else audible? Fade it out first; never overlap by default.
        let busy: Vec<usize> = (0..2).filter(|&d| d != target_deck && self.audible(d)).collect();
        if !busy.is_empty() {
            if busy.iter().any(|&d| self.decks[d].is_fading()) {
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
                self.take_air(d);
                if dk.status() == "ended" {
                    dk.seek(dk.cue_secs());
                }
                dk.play();
            }
            PendingNext::Next => {
                let e = self.next_entry().ok_or_else(|| anyhow!("Nothing left in the setlist"))?;
                self.start_entry(&e, deck)?;
            }
            PendingNext::Entry(id) => {
                let (e, _) = self.store.entry(&id)?;
                self.start_entry(&e, deck)?;
            }
        }
        {
            let mut p = self.perform.lock();
            p.pending = None;
            p.armed_until = None;
            p.gap_started = None;
        }
        self.emit_perform();
        Ok(())
    }

    /// Starts a setlist entry on `deck`, reusing it if it's already loaded there
    /// (so cue points / tempo set while cueing are kept).
    fn start_entry(&self, entry: &SetlistEntry, deck: usize) -> Result<()> {
        let version_id = entry.version_id.clone().ok_or_else(|| {
            let style = match (&entry.label, entry.styles.as_deref()) {
                (Some(l), _) => l.clone(),
                (None, Some(st)) if !st.is_empty() => st.iter().map(|s| self.style_name(s)).collect::<Vec<_>>().join(" / "),
                _ => "this".into(),
            };
            anyhow!("Choose a song for the {style} slot first")
        })?;
        // Already loaded on the other deck (e.g. the DJ loaded it by hand)? Use that deck.
        let deck = if self.decks[1 - deck].entry_id().as_deref() == Some(&entry.id) && !self.audible(1 - deck) {
            1 - deck
        } else {
            deck
        };
        let d = &self.decks[deck];
        let same = d.loaded()
            && d.status() != "loading"
            && (d.entry_id().as_deref() == Some(&entry.id)
                || (d.entry_id().is_none() && d.version_id().as_deref() == Some(&version_id)));
        self.take_air(deck);
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
            p.gap_started = None;
        }
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
        let Ok((version, track_id)) = self.store.version(&version_id) else { return };
        let Ok(track) = self.store.track(&track_id) else { return };
        match self.load_request(free, &track, &version, Some(&next), false) {
            Ok(req) => {
                self.untake(&[&format!("deck{free}.tempo")]);
                d.load(req);
            }
            Err(e) => log::warn!("auto-load: {e:#}"),
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
        drop(p);
        let entries = self.entries();
        let started_at = entries.iter().filter_map(|e| e.played_at).min();
        let next = self.next_entry();
        let next_ready = next.as_ref().map(|e| e.version_id.is_some()).unwrap_or(false);
        PerformState {
            setlist_id,
            mode,
            current_entry_id: self.current_entry_id(),
            next_entry_id: next.map(|e| e.id),
            gap_started_at: gap,
            gap_seconds: gap_secs,
            auto_advance: auto,
            auto_advance_at: if auto && next_ready { gap.map(|g| g + (gap_secs * 1000.0) as i64) } else { None },
            pending_next: pending,
            armed,
            message,
            started_at,
            crossfader,
            live_deck,
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

    /// Club style names / colours are filled in by the projector page from
    /// `ProjectorPayload::{profile, style_library}` (see `src/lib/styles.ts`).
    fn proj_entry(&self, e: &SetlistEntry, profile: &ProfileCore, lookup: &HashMap<Id, (Version, Track)>) -> ProjEntry {
        let vt = e.version_id.as_ref().and_then(|id| lookup.get(id));
        let styles = e.styles.clone().or_else(|| vt.map(|(v, _)| v.styles.clone())).unwrap_or_default();
        let target = vt.and_then(|(v, _)| Self::resolve_settings(v, Some(e)).target_bpm);
        ProjEntry {
            id: e.id.clone(),
            track_id: vt.map(|(_, t)| t.rec.id.clone()),
            title: vt.map(|(_, t)| t.rec.title.clone()).unwrap_or_default(),
            artist: vt.map(|(_, t)| t.rec.artist.clone()).unwrap_or_default(),
            styles,
            label: e.label.clone(),
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
        let proj: Vec<ProjEntry> = entries.iter().map(|e| self.proj_entry(e, &profile, &lookup)).collect();
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
        history.retain(|h| !h.title.is_empty() || !h.styles.is_empty() || h.label.is_some());
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
            live_deck,
            profile: self.active_profile_json(),
            style_library: self.style_library(),
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

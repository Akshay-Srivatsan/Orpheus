//! Deck controls (cue, hot cues, scrubbing, jumps, edit points) and the
//! mapping from controller actions to them.

use crate::app::{emit, App};
use crate::midi::Value;
use crate::model::*;
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

/// Jog wheel resolution (ticks per revolution, DJ2GO2) and scrub speeds.
const JOG_TICKS_PER_REV: f64 = 236.0;
/// Seconds per revolution while touching the platter (like vinyl at 33⅓).
const JOG_FINE_SECS_PER_REV: f64 = 1.8;
/// Seconds per revolution when turning the outer edge (fast search).
const JOG_COARSE_SECS_PER_REV: f64 = 12.0;
/// Length of the snippet played while scrubbing a paused deck.
const SCRUB_SNIPPET: Duration = Duration::from_millis(110);

#[derive(Default)]
pub struct DeckControlState {
    /// Seek target not yet sent to the deck (jog ticks are coalesced).
    pub pending_seek: Option<f64>,
    /// Play a short snippet after the pending seek (scrubbing while paused).
    pub snippet: bool,
    /// The snippet ends at this time.
    pub snippet_until: Option<Instant>,
    /// CUE is held for a preview from the cue point.
    pub cue_preview: bool,
    pub jog_touch: bool,
}

#[derive(Default)]
pub struct Takeover {
    pub engaged: std::collections::HashSet<String>,
    pub last_hw: HashMap<String, f64>,
}

impl App {
    fn require_loaded(&self, deck: usize) -> Result<()> {
        if self.decks[deck].loaded() {
            Ok(())
        } else {
            Err(anyhow!("Deck {} is empty", crate::perform::deck_name(deck)))
        }
    }

    /// Loads a version onto a deck. Refuses if the room is hearing that deck.
    pub fn deck_load(&self, deck: usize, version_id: &str, entry_id: Option<&str>, play: bool) -> Result<()> {
        if self.audible(deck) {
            return Err(anyhow!(
                "Deck {} is on air. Load onto the other deck, or fade this one out first.",
                crate::perform::deck_name(deck)
            ));
        }
        let (version, track_id) = self.store.version(version_id)?;
        let track = self.store.track(&track_id)?;
        let entry = entry_id.and_then(|id| self.store.entry(id).ok()).map(|(e, _)| e);
        let req = self.load_request(deck, &track, &version, entry.as_ref(), play)?;
        self.untake(&[&format!("deck{deck}.tempo")]);
        self.decks[deck].load(req);
        Ok(())
    }

    pub fn deck_play_pause(&self, deck: usize) {
        let d = &self.decks[deck];
        match d.status().as_str() {
            "playing" => d.pause(),
            "ended" => {
                d.seek(d.cue_secs());
                d.play();
            }
            _ => {
                d.play();
            }
        }
        self.emit_perform();
    }

    /// CDJ-style CUE. Playing: back to the cue point and stop. Paused away from
    /// the cue point: set it here. Paused at the cue point: preview while held.
    pub fn deck_cue(&self, deck: usize, down: bool) {
        let d = &self.decks[deck];
        if !d.loaded() {
            return;
        }
        let mut cs = self.deck_ctl.lock();
        let st = &mut cs[deck];
        if !down {
            if st.cue_preview {
                st.cue_preview = false;
                d.pause();
                d.seek(d.cue_secs());
            }
            return;
        }
        if d.status() == "playing" {
            if self.protected(deck) {
                drop(cs);
                emit(&self.handle, "toast", "CUE is disabled on the on-air deck while it plays (Settings → General).");
                return;
            }
            d.pause();
            d.seek(d.cue_secs());
        } else if (d.position_secs() - d.cue_secs()).abs() > 0.05 {
            d.set_cue(d.position_secs());
        } else if !d.is_on_air() {
            // Preview from the cue point while held (headphones only).
            st.cue_preview = true;
            d.play();
        }
    }

    fn deck_track(&self, deck: usize) -> Option<Track> {
        self.decks[deck].track_id().and_then(|id| self.store.track(&id).ok())
    }

    /// Hot cue pad: jump to it if set, otherwise set it at the current position.
    pub fn deck_hotcue(&self, deck: usize, slot: u8) -> Result<()> {
        self.require_loaded(deck)?;
        let t = self.deck_track(deck).ok_or_else(|| anyhow!("no track"))?;
        match t.rec.hot_cues.iter().find(|c| c.slot == slot) {
            Some(c) => {
                if self.protected(deck) {
                    return Err(anyhow!("Hot cue jumps are disabled on the on-air deck while it plays."));
                }
                self.decks[deck].seek(c.ms as f64 / 1000.0);
            }
            None => {
                let ms = (self.decks[deck].position_secs() * 1000.0).round() as i64;
                self.store.set_hot_cue(&t.rec.id, slot, Some(ms))?;
                emit(&self.handle, "library-changed", vec![t.rec.id.clone()]);
            }
        }
        Ok(())
    }

    pub fn deck_hotcue_clear(&self, deck: usize, slot: u8) -> Result<()> {
        let t = self.deck_track(deck).ok_or_else(|| anyhow!("Deck is empty"))?;
        self.store.set_hot_cue(&t.rec.id, slot, None)?;
        emit(&self.handle, "library-changed", vec![t.rec.id.clone()]);
        Ok(())
    }

    /// Moves the playhead by `secs` (coalesced; plays a snippet when paused).
    pub fn deck_nudge_position(&self, deck: usize, secs: f64) -> Result<()> {
        let d = &self.decks[deck];
        self.require_loaded(deck)?;
        if self.protected(deck) {
            return Err(anyhow!("Scrubbing is disabled on the on-air deck while it plays."));
        }
        let mut cs = self.deck_ctl.lock();
        let st = &mut cs[deck];
        let base = st.pending_seek.unwrap_or_else(|| d.position_secs());
        let snap = d.snapshot();
        let end = snap.end_ms.unwrap_or(snap.duration_ms) as f64 / 1000.0;
        st.pending_seek = Some((base + secs).clamp(0.0, (end - 0.05).max(0.0)));
        // Snippets only in headphones: never while the room can hear the deck.
        st.snippet = d.status() != "playing" && !d.is_on_air();
        Ok(())
    }

    /// Scrub to an absolute position (waveform drag).
    pub fn deck_scrub_to(&self, deck: usize, secs: f64) -> Result<()> {
        let cur = self.decks[deck].position_secs();
        let pending = self.deck_ctl.lock()[deck].pending_seek;
        self.deck_nudge_position(deck, secs - pending.unwrap_or(cur))
    }

    pub fn deck_jog(&self, deck: usize, ticks: i32) -> Result<()> {
        let touching = self.deck_ctl.lock()[deck].jog_touch;
        let per_rev = if touching { JOG_FINE_SECS_PER_REV } else { JOG_COARSE_SECS_PER_REV };
        self.deck_nudge_position(deck, ticks as f64 * per_rev / JOG_TICKS_PER_REV)
    }

    /// Applies coalesced seeks and ends scrub snippets (called by the poller).
    pub fn apply_pending_seeks(&self) {
        let now = Instant::now();
        let mut cs = self.deck_ctl.lock();
        for (i, st) in cs.iter_mut().enumerate() {
            let d = &self.decks[i];
            if let Some(secs) = st.pending_seek.take() {
                d.seek(secs);
                if st.snippet && d.status() == "paused" {
                    d.set_audible(true);
                    st.snippet_until = Some(now + SCRUB_SNIPPET);
                }
                st.snippet = false;
            }
            if let Some(t) = st.snippet_until {
                if now >= t {
                    st.snippet_until = None;
                    if d.status() == "paused" {
                        d.set_audible(false);
                    }
                }
            }
        }
    }

    /// Sets edit points of the loaded version from the current position:
    /// `start`, `end`, `fade`, or `clear` (removes all three).
    pub fn deck_set_point(&self, deck: usize, kind: &str) -> Result<()> {
        let d = &self.decks[deck];
        let vid = d.version_id().ok_or_else(|| anyhow!("Deck is empty"))?;
        let pos = (d.position_secs() * 1000.0).round() as i64;
        let mut p = VersionPatch::default();
        match kind {
            "start" => {
                p.cue_in_ms = Some(Some(pos));
                d.set_cue(pos as f64 / 1000.0);
            }
            "end" => p.cue_out_ms = Some(Some(pos)),
            "fade" => {
                p.fade_out_ms = Some(Some(pos));
                d.set_fade_at(Some(pos as f64 / 1000.0));
            }
            "clear" => {
                p.cue_in_ms = Some(None);
                p.cue_out_ms = Some(None);
                p.fade_out_ms = Some(None);
                d.set_fade_at(None);
            }
            other => return Err(anyhow!("unknown point {other}")),
        }
        let v = self.store.version_update(&vid, &p)?;
        let (_, tid) = self.store.version(&v.id)?;
        emit(&self.handle, "library-changed", vec![tid]);
        let label = match kind {
            "start" => "Start",
            "end" => "End",
            "fade" => "Fade-out point",
            _ => "Start, end and fade",
        };
        let what = if kind == "clear" { "cleared" } else { "set" };
        emit(&self.handle, "toast-info", format!("{label} {what} for “{}”", v.name));
        Ok(())
    }

    /// Resets the tempo to the version's (or setlist entry's) target.
    pub fn deck_tempo_reset(&self, deck: usize) {
        let d = &self.decks[deck];
        if let (Some(tid), Some(vid)) = (d.track_id(), d.version_id()) {
            if let (Ok(t), Ok((v, _))) = (self.store.track(&tid), self.store.version(&vid)) {
                let e = d.entry_id().and_then(|id| self.store.entry(&id).ok()).map(|(e, _)| e);
                let target = Self::resolve_settings(&v, e.as_ref()).target_bpm;
                let tempo = match (target, t.rec.bpm) {
                    (Some(x), Some(b)) if b > 0.0 => x / crate::app::fold_bpm(b, x),
                    _ => 1.0,
                };
                d.set_tempo(tempo);
                self.untake(&[&format!("deck{deck}.tempo")]);
            }
        }
    }

    // ------------------------------------------------------------ stems

    /// Queues stem separation for tracks with a local file.
    pub fn prepare_stems(&self, track_ids: &[Id]) -> Result<usize> {
        let mut queued = 0;
        for id in track_ids {
            let t = self.store.track(id)?;
            let Some(path) = crate::library::resolve_path(&self.store, &t.rec.id) else {
                if track_ids.len() == 1 {
                    return Err(anyhow!("Stems need a local file; “{}” is only on Spotify.", t.rec.title));
                }
                continue;
            };
            let Some(hash) = self.store.file_info(&path.to_string_lossy()).map(|f| f.hash) else { continue };
            self.stems.request(&t.rec.id, &hash, path)?;
            queued += 1;
        }
        Ok(queued)
    }

    /// Stem pad: toggles a stem, or starts preparing stems if the song has none.
    pub fn deck_stem_toggle(&self, deck: usize, index: usize) -> Result<()> {
        let d = &self.decks[deck];
        self.require_loaded(deck)?;
        if d.has_stems() {
            d.set_stem(index, !d.stem_on(index));
            return Ok(());
        }
        let tid = d.track_id().ok_or_else(|| anyhow!("Deck is empty"))?;
        if matches!(self.stems.job(&tid), Some(crate::stems::JobStatus::Queued | crate::stems::JobStatus::Running { .. })) {
            return Err(anyhow!("Stems for this song are still being prepared."));
        }
        if !self.stems.model_ready() {
            emit(&self.handle, "open-settings", "stems");
            return Err(anyhow!("Stems need the separation model (about 210 MB). Download it in Settings → Stems."));
        }
        self.prepare_stems(&[tid.clone()])?;
        let title = self.store.track(&tid).map(|t| t.rec.title).unwrap_or_default();
        emit(&self.handle, "toast-info", format!("Preparing stems for “{title}” (about a minute)…"));
        Ok(())
    }

    /// Called when a stem job or the model changes state.
    pub fn on_stems_changed(&self, track: Option<Id>) {
        emit(&self.handle, "stems-status", self.stems.status());
        let Some(tid) = track else { return };
        if self.stems.job(&tid).is_some() {
            return; // still running (or failed)
        }
        // Newly ready: switch decks with this song onto its stems, unless the room hears them.
        for deck in 0..2 {
            let d = &self.decks[deck];
            if d.track_id().as_deref() == Some(&tid) && !d.has_stems() && !self.audible(deck) {
                if let Err(e) = self.reload_deck(deck) {
                    log::warn!("reloading deck with stems: {e:#}");
                }
            }
        }
        emit(&self.handle, "library-changed", vec![tid]);
    }

    /// Reloads a deck's song in place (position, play state, tempo and cue kept).
    fn reload_deck(&self, deck: usize) -> Result<()> {
        let d = &self.decks[deck];
        let vid = d.version_id().ok_or_else(|| anyhow!("Deck is empty"))?;
        let (version, track_id) = self.store.version(&vid)?;
        let track = self.store.track(&track_id)?;
        let entry = d.entry_id().and_then(|id| self.store.entry(&id).ok()).map(|(e, _)| e);
        let (pos, tempo, cue, playing) = (d.position_secs(), d.tempo(), d.cue_secs(), d.status() == "playing");
        let mut req = self.load_request(deck, &track, &version, entry.as_ref(), playing)?;
        req.tempo = tempo;
        req.cue_secs = Some(cue);
        d.load(req);
        d.seek(pos);
        Ok(())
    }

    pub fn deck_set_pfl(&self, deck: usize, on: bool) {
        self.decks[deck].set_pfl(on);
        self.update_routing();
    }

    // ------------------------------------------------------------ MIDI

    pub fn untake(&self, actions: &[&str]) {
        let mut t = self.takeover.lock();
        for a in actions {
            t.engaged.remove(*a);
        }
    }

    /// Soft takeover: a hardware fader only takes control once it reaches (or
    /// crosses) the current software value, so a preset tempo or an automatic
    /// crossfader move isn't undone by wherever the fader happens to be.
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

        if let Some(rest) = action.strip_prefix("deck") {
            let Some((n, cmd)) = rest.split_once('.') else { return };
            let Ok(deck) = n.parse::<usize>() else { return };
            if deck > 1 {
                return;
            }
            let d = &self.decks[deck];
            match (cmd, value) {
                ("playPause", Value::Press) => self.deck_play_pause(deck),
                ("cue", Value::Press) => self.deck_cue(deck, true),
                ("cue", Value::Release) => self.deck_cue(deck, false),
                // SYNC: Go Live; on the deck the room is hearing, fade out (again: cancel).
                ("goLive", Value::Press) => {
                    if self.audible(deck) && d.status() == "playing" {
                        if d.is_fading() {
                            d.cancel_fade();
                            self.perform.lock().pending = None;
                        } else {
                            d.fade_out(self.fade_secs());
                        }
                    } else {
                        result = self.go_live(deck).map(|_| ());
                    }
                }
                ("pfl", Value::Press) => self.deck_set_pfl(deck, !d.pfl()),
                ("load", Value::Press) => {
                    emit(&self.handle, "midi-ui", serde_json::json!({ "action": "deck.load", "deck": deck }));
                }
                ("tempo", Value::Abs(v)) => {
                    if self.takeover(action, v, tempo_to_hw(d.tempo())) {
                        d.set_tempo(hw_to_tempo(v));
                    }
                }
                ("tempoNudge", Value::Rel(n)) => {
                    d.set_tempo(d.tempo() + n as f64 * 0.001);
                    self.untake(&[&format!("deck{deck}.tempo")]);
                }
                ("tempoReset", Value::Press) => self.deck_tempo_reset(deck),
                ("trim", Value::Abs(v)) => {
                    if self.takeover(action, v, trim_to_hw(d.shared.trim_db.get())) {
                        d.set_trim_db(trim_db(v));
                    }
                }
                ("jogTouch", Value::Press) => self.deck_ctl.lock()[deck].jog_touch = true,
                ("jogTouch", Value::Release) => self.deck_ctl.lock()[deck].jog_touch = false,
                ("jog", Value::Rel(n)) => {
                    // Silently ignored when protected: jog wheels get bumped.
                    if !self.protected(deck) {
                        result = self.deck_jog(deck, n);
                    }
                }
                ("setStart", Value::Press) => result = self.deck_set_point(deck, "start"),
                ("setEnd", Value::Press) => result = self.deck_set_point(deck, "end"),
                ("setFade", Value::Press) => result = self.deck_set_point(deck, "fade"),
                ("clearPoints", Value::Press) => result = self.deck_set_point(deck, "clear"),
                (c, Value::Press) if c.starts_with("stem") => {
                    if let Ok(n) = c["stem".len()..].parse::<usize>() {
                        if (1..=4).contains(&n) {
                            result = self.deck_stem_toggle(deck, n - 1);
                        }
                    }
                }
                (c, Value::Press) if c.starts_with("hotcueClear") => {
                    if let Ok(n) = c["hotcueClear".len()..].parse::<u8>() {
                        result = self.deck_hotcue_clear(deck, n);
                    }
                }
                (c, Value::Press) if c.starts_with("hotcue") => {
                    if let Ok(n) = c["hotcue".len()..].parse::<u8>() {
                        result = self.deck_hotcue(deck, n);
                    }
                }
                (c, Value::Press) if c.starts_with("jump") => {
                    if let Ok(s) = c["jump".len()..].parse::<f64>() {
                        result = self.deck_nudge_position(deck, s);
                    }
                }
                _ => {}
            }
        } else {
            match (action, value) {
                ("perform.playNext", Value::Press) => result = self.play_next().map(|_| ()),
                ("perform.fadeOut", Value::Press) => self.fade_out_air(),
                ("perform.skipNext", Value::Press) => result = self.skip_next(),
                ("perform.toggleAutoAdvance", Value::Press) => {
                    let v = !self.auto_advance();
                    self.perform.lock().auto_advance_override = Some(v);
                    self.emit_perform();
                }
                ("master.volume", Value::Abs(v)) => self.levels.master.set((v * v) as f32),
                ("cue.volume", Value::Abs(v)) => self.levels.cue.set((v * v) as f32),
                ("cue.crossfader", Value::Abs(v)) => {
                    let cur = self.perform.lock().crossfader;
                    if self.takeover(action, v, cur) {
                        self.set_crossfader(v);
                        self.emit_perform();
                    }
                }
                (a, v) if a.starts_with("browse.") => {
                    let payload = match v {
                        Value::Rel(n) => serde_json::json!({ "action": a, "delta": n }),
                        _ => serde_json::json!({ "action": a }),
                    };
                    emit(&self.handle, "midi-ui", payload);
                }
                _ => {}
            }
        }
        if let Err(e) = result {
            emit(&self.handle, "toast", format!("{e:#}"));
        }
    }

    /// Controller LED states (see `midi::led_states`).
    pub fn led_states(&self) -> HashMap<String, bool> {
        let blink = (now_ms() / 400) % 2 == 0;
        let next = self.next_entry();
        let next_ready = next.as_ref().map(|e| e.version_id.is_some()).unwrap_or(false);
        let mut st = HashMap::new();
        let anything_audible = (0..2).any(|d| self.audible(d) && self.decks[d].status() == "playing");
        st.insert("perform.nextReady".to_string(), next_ready && !anything_audible && blink);
        st.insert("perform.autoAdvance".to_string(), self.auto_advance());
        for (i, d) in self.decks.iter().enumerate() {
            let snap = d.snapshot();
            let loaded = d.loaded();
            let holds_next = next.as_ref().map(|n| snap.entry_id.as_deref() == Some(&n.id)).unwrap_or(false);
            st.insert(format!("deck{i}.playing"), snap.status == "playing");
            st.insert(format!("deck{i}.onAir"), snap.on_air && loaded);
            // SYNC (= Go Live): solid when on air, blinking when it holds the next song.
            st.insert(format!("deck{i}.sync"), (snap.on_air && loaded) || (holds_next && !snap.on_air && blink));
            st.insert(format!("deck{i}.pfl"), snap.pfl);
            st.insert(
                format!("deck{i}.cueReady"),
                loaded && snap.status != "playing" && (snap.position_ms - snap.cue_ms).abs() < 60,
            );
            let cues = snap
                .track_id
                .as_ref()
                .and_then(|id| self.store.track(id).ok())
                .map(|t| t.rec.hot_cues)
                .unwrap_or_default();
            for n in 1..=4u8 {
                st.insert(format!("deck{i}.hotcue{n}"), cues.iter().any(|c| c.slot == n));
            }
            // Stem pads: lit while the stem is on; blinking while stems are prepared.
            let preparing = snap
                .track_id
                .as_ref()
                .map(|t| matches!(self.stems.job(t), Some(crate::stems::JobStatus::Queued | crate::stems::JobStatus::Running { .. })))
                .unwrap_or(false);
            for n in 0..4 {
                let on = match snap.stems {
                    Some(s) => s[n],
                    None => preparing && blink,
                };
                st.insert(format!("deck{i}.stem{}", n + 1), on);
            }
        }
        let _ = Ordering::Relaxed;
        st
    }
}

//! Persistence as plain files.
//!
//! The *library directory* is meant to be version-controlled or synced
//! (git, Dropbox, Syncthing...). Everything in it is pretty-printed JSON with
//! stable field order, one object per file, so diffs and merges stay small:
//!
//! ```text
//! <library>/
//!   tracks/<id>.json      a recording: metadata, analysis, sources, versions
//!   setlists/<id>.json    a setlist and its entries
//!   crates/<id>.json      a hand-picked collection of versions
//!   profiles/<id>.json    custom / modified club profiles
//!   styles.json           your dance styles and how they map onto clubs' styles
//! ```
//!
//! Machine-specific state lives in the app data directory instead: the file
//! index (paths differ per machine; files are matched to tracks by content
//! hash), waveform caches, settings and the Spotify login.
//!
//! All writes go to a temporary file which is then renamed over the target, so
//! a crash never leaves a half-written file.

use crate::model::*;
use anyhow::{anyhow, Context, Result};
use parking_lot::RwLock;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct NewTrack {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub duration_ms: i64,
    pub bpm: Option<f64>,
    pub bpm_source: Option<String>,
    pub artwork_url: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Analysis {
    pub duration_ms: i64,
    pub bpm: Option<f64>,
    /// Detected beats per bar (3 or 4), if clear.
    pub meter: Option<u8>,
    pub loudness_db: Option<f64>,
    pub audio_start_ms: Option<i64>,
    pub audio_end_ms: Option<i64>,
    pub waveform: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub hash: String,
    pub size: i64,
    pub mtime: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
}

/// Machine-local map of file paths to content hashes.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct FileIndex {
    version: u32,
    files: BTreeMap<String, FileEntry>,
}

#[derive(Default)]
struct Inner {
    tracks: HashMap<Id, TrackRecord>,
    version_owner: HashMap<Id, Id>,
    source_owner: HashMap<SourceRef, Id>,
    alias: HashMap<Id, Id>,
    setlists: HashMap<Id, Setlist>,
    entry_owner: HashMap<Id, Id>,
    crates: HashMap<Id, Crate>,
    profiles: BTreeMap<String, serde_json::Value>,
    index: FileIndex,
    by_hash: HashMap<String, Vec<String>>,
    load_errors: Vec<String>,
    styles: Option<StyleLibrary>,
}

pub struct Store {
    lib: RwLock<PathBuf>,
    local: PathBuf,
    inner: RwLock<Inner>,
}

/// Removes duplicates, keeping the first occurrence of each.
fn dedup(v: &[String]) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for s in v {
        if !s.is_empty() && !out.contains(s) {
            out.push(s.clone());
        }
    }
    out
}

/// Replaces (or with `None` removes) one style key in a list.
fn rekey(v: &[String], from: &str, to: Option<&str>) -> Vec<String> {
    let mapped: Vec<String> = v.iter().filter_map(|s| if s == from { to.map(String::from) } else { Some(s.clone()) }).collect();
    dedup(&mapped)
}

/// Writes `bytes` to `path` atomically (temp file + rename).
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().ok_or_else(|| anyhow!("bad path {}", path.display()))?;
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(
        ".{}.tmp-{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("file"),
        std::process::id()
    ));
    {
        let mut f = fs::File::create(&tmp).with_context(|| format!("writing {}", tmp.display()))?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut s = serde_json::to_string_pretty(value)?;
    s.push('\n');
    write_atomic(path, s.as_bytes())
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let s = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&s).with_context(|| format!("parsing {}", path.display()))
}

fn json_files(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
                .filter(|p| !p.file_name().and_then(|n| n.to_str()).unwrap_or("").starts_with('.'))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

impl Inner {
    fn reindex_track(&mut self, t: &TrackRecord) {
        for v in &t.versions {
            self.version_owner.insert(v.id.clone(), t.id.clone());
        }
        for s in &t.sources {
            self.source_owner.insert(s.clone(), t.id.clone());
        }
        for a in &t.aliases {
            self.alias.insert(a.clone(), t.id.clone());
        }
    }

    fn reindex_setlist(&mut self, s: &Setlist) {
        for e in &s.entries {
            self.entry_owner.insert(e.id.clone(), s.id.clone());
        }
    }

    fn rebuild_hash_index(&mut self) {
        self.by_hash.clear();
        for (p, f) in &self.index.files {
            self.by_hash.entry(f.hash.clone()).or_default().push(p.clone());
        }
    }

    fn canonical(&self, track_id: &str) -> Id {
        self.alias.get(track_id).cloned().unwrap_or_else(|| track_id.to_string())
    }

    /// Track id → (number of setlists it was played in, last played), derived
    /// from the setlists so deleting a setlist erases its plays.
    fn play_stats(&self) -> HashMap<Id, (i64, i64)> {
        let mut stats: HashMap<Id, (i64, i64)> = HashMap::new();
        for s in self.setlists.values() {
            let mut seen: HashMap<&Id, i64> = HashMap::new();
            for e in &s.entries {
                if !(e.status == "played" || e.status == "playing") {
                    continue;
                }
                let Some(tid) = e.version_id.as_ref().and_then(|v| self.version_owner.get(v)) else { continue };
                let at = seen.entry(tid).or_insert(0);
                *at = (*at).max(e.played_at.unwrap_or(0));
            }
            for (tid, last) in seen {
                let st = stats.entry(tid.clone()).or_insert((0, 0));
                st.0 += 1;
                st.1 = st.1.max(last);
            }
        }
        stats
    }

    fn to_track(&self, rec: &TrackRecord, stats: &HashMap<Id, (i64, i64)>) -> Track {
        let mut files = vec![];
        for s in rec.sources.iter().filter(|s| s.kind == "file") {
            if let Some(paths) = self.by_hash.get(&s.key) {
                files.extend(paths.iter().cloned());
            }
        }
        files.sort();
        let has_spotify = rec.sources.iter().any(|s| s.kind == "spotify");
        let (count, last) = stats.get(&rec.id).copied().unwrap_or((0, 0));
        Track {
            missing: files.is_empty() && !has_spotify,
            files,
            play_count: count,
            last_played_at: (last > 0).then_some(last),
            rec: rec.clone(),
        }
    }
}

impl Store {
    pub fn open(library_dir: &Path, local_dir: &Path) -> Result<Self> {
        let store = Self {
            lib: RwLock::new(library_dir.to_path_buf()),
            local: local_dir.to_path_buf(),
            inner: RwLock::new(Inner::default()),
        };
        store.reload(library_dir)?;
        Ok(store)
    }

    pub fn library_dir(&self) -> PathBuf {
        self.lib.read().clone()
    }

    pub fn load_errors(&self) -> Vec<String> {
        self.inner.read().load_errors.clone()
    }

    /// (Re)loads everything from disk, e.g. after `git pull` or switching libraries.
    pub fn reload(&self, library_dir: &Path) -> Result<()> {
        for d in ["tracks", "setlists", "crates", "profiles"] {
            fs::create_dir_all(library_dir.join(d))
                .with_context(|| format!("creating {}", library_dir.join(d).display()))?;
        }
        fs::create_dir_all(self.local.join("waveforms"))?;
        // Make the library pleasant to keep in git.
        let gi = library_dir.join(".gitignore");
        if !gi.exists() {
            let _ = fs::write(&gi, ".*.tmp-*\n");
        }
        let mut inner = Inner::default();
        let mut errors = vec![];
        for p in json_files(&library_dir.join("tracks")) {
            match read_json::<TrackRecord>(&p) {
                Ok(mut t) if !t.id.is_empty() => {
                    t.versions.iter_mut().for_each(Version::normalize);
                    inner.reindex_track(&t);
                    inner.tracks.insert(t.id.clone(), t);
                }
                Ok(_) => errors.push(format!("{}: missing id", p.display())),
                Err(e) => errors.push(format!("{e:#}")),
            }
        }
        for p in json_files(&library_dir.join("setlists")) {
            match read_json::<Setlist>(&p) {
                Ok(mut s) if !s.id.is_empty() => {
                    s.entries.iter_mut().for_each(SetlistEntry::normalize);
                    inner.reindex_setlist(&s);
                    inner.setlists.insert(s.id.clone(), s);
                }
                Ok(_) => errors.push(format!("{}: missing id", p.display())),
                Err(e) => errors.push(format!("{e:#}")),
            }
        }
        for p in json_files(&library_dir.join("crates")) {
            match read_json::<Crate>(&p) {
                Ok(c) if !c.id.is_empty() => {
                    inner.crates.insert(c.id.clone(), c);
                }
                Ok(_) => errors.push(format!("{}: missing id", p.display())),
                Err(e) => errors.push(format!("{e:#}")),
            }
        }
        for p in json_files(&library_dir.join("profiles")) {
            match read_json::<serde_json::Value>(&p) {
                Ok(v) => match v["id"].as_str() {
                    Some(id) => {
                        inner.profiles.insert(id.to_string(), v);
                    }
                    None => errors.push(format!("{}: missing id", p.display())),
                },
                Err(e) => errors.push(format!("{e:#}")),
            }
        }
        let styles_path = library_dir.join("styles.json");
        if styles_path.exists() {
            match read_json::<StyleLibrary>(&styles_path) {
                Ok(s) => inner.styles = Some(s),
                Err(e) => errors.push(format!("{e:#}")),
            }
        }
        let index_path = self.local.join("file-index.json");
        if index_path.exists() {
            match read_json::<FileIndex>(&index_path) {
                Ok(ix) => inner.index = ix,
                Err(e) => errors.push(format!("{e:#} (the file index will be rebuilt by the next scan)")),
            }
        }
        inner.rebuild_hash_index();
        for e in &errors {
            log::warn!("library: {e}");
        }
        inner.load_errors = errors;
        *self.inner.write() = inner;
        *self.lib.write() = library_dir.to_path_buf();
        Ok(())
    }

    fn lib_path(&self, kind: &str, id: &str) -> PathBuf {
        self.lib.read().join(kind).join(format!("{id}.json"))
    }

    fn save_track(&self, t: &TrackRecord) -> Result<()> {
        write_json(&self.lib_path("tracks", &t.id), t)
    }

    fn save_setlist(&self, s: &Setlist) -> Result<()> {
        write_json(&self.lib_path("setlists", &s.id), s)
    }

    fn save_crate(&self, c: &Crate) -> Result<()> {
        write_json(&self.lib_path("crates", &c.id), c)
    }

    pub fn save_index(&self) -> Result<()> {
        let ix = self.inner.read().index.clone();
        write_json(&self.local.join("file-index.json"), &ix)
    }

    /// Mutates a track and writes its file.
    fn with_track<R>(&self, id: &str, f: impl FnOnce(&mut TrackRecord) -> Result<R>) -> Result<R> {
        let mut inner = self.inner.write();
        let id = inner.canonical(id);
        let t = inner.tracks.get_mut(&id).ok_or_else(|| anyhow!("track {id} not found"))?;
        let r = f(t)?;
        let t = t.clone();
        inner.reindex_track(&t);
        drop(inner);
        self.save_track(&t)?;
        Ok(r)
    }

    fn with_setlist<R>(&self, id: &str, f: impl FnOnce(&mut Setlist) -> Result<R>) -> Result<R> {
        let mut inner = self.inner.write();
        let s = inner.setlists.get_mut(id).ok_or_else(|| anyhow!("setlist {id} not found"))?;
        let r = f(s)?;
        s.updated_at = now_ms();
        let s = s.clone();
        inner.reindex_setlist(&s);
        drop(inner);
        self.save_setlist(&s)?;
        Ok(r)
    }

    fn with_crate<R>(&self, id: &str, f: impl FnOnce(&mut Crate) -> R) -> Result<R> {
        let mut inner = self.inner.write();
        let c = inner.crates.get_mut(id).ok_or_else(|| anyhow!("crate {id} not found"))?;
        let r = f(c);
        let c = c.clone();
        drop(inner);
        self.save_crate(&c)?;
        Ok(r)
    }

    // ---------------------------------------------------------------- tracks

    pub fn all_tracks(&self) -> Vec<Track> {
        let inner = self.inner.read();
        let stats = inner.play_stats();
        let mut v: Vec<Track> = inner.tracks.values().map(|t| inner.to_track(t, &stats)).collect();
        v.sort_by(|a, b| (&a.rec.artist, &a.rec.title).cmp(&(&b.rec.artist, &b.rec.title)));
        v
    }

    pub fn track(&self, id: &str) -> Result<Track> {
        let inner = self.inner.read();
        let id = inner.canonical(id);
        let stats = inner.play_stats();
        inner.tracks.get(&id).map(|t| inner.to_track(t, &stats)).ok_or_else(|| anyhow!("track {id} not found"))
    }

    /// Creates a track with one source and a default version.
    pub fn insert_track(&self, t: &NewTrack, source: SourceRef) -> Result<Id> {
        if let Some(existing) = self.track_for_source(&source) {
            return Ok(existing);
        }
        let rec = TrackRecord {
            id: new_id(),
            title: t.title.clone(),
            artist: t.artist.clone(),
            album: t.album.clone(),
            genre: t.genre.clone(),
            duration_ms: t.duration_ms,
            bpm: t.bpm,
            bpm_source: t.bpm_source.clone(),
            tag_bpm: if t.bpm_source.as_deref() == Some("tag") { t.bpm } else { None },
            artwork_url: t.artwork_url.clone(),
            added_at: now_ms(),
            in_library: true,
            sources: vec![source],
            versions: vec![Version {
                id: new_id(),
                name: "Original".into(),
                display_name: None,
                styles: vec![],
                legacy_style: None,
                settings: Overrides::default(),
                comment: String::new(),
                tags: vec![],
            }],
            ..Default::default()
        };
        self.save_track(&rec)?;
        let mut inner = self.inner.write();
        inner.reindex_track(&rec);
        let id = rec.id.clone();
        inner.tracks.insert(id.clone(), rec);
        Ok(id)
    }

    pub fn track_for_source(&self, s: &SourceRef) -> Option<Id> {
        self.inner.read().source_owner.get(s).cloned()
    }

    pub fn add_source(&self, track_id: &str, s: SourceRef) -> Result<()> {
        if let Some(owner) = self.track_for_source(&s) {
            if owner != track_id {
                return Err(anyhow!("that source already belongs to another track (merge the tracks instead)"));
            }
            return Ok(());
        }
        self.with_track(track_id, |t| {
            t.sources.push(s);
            Ok(())
        })
    }

    pub fn remove_source(&self, track_id: &str, s: &SourceRef) -> Result<()> {
        self.with_track(track_id, |t| {
            if t.sources.len() <= 1 {
                return Err(anyhow!("a track needs at least one source"));
            }
            t.sources.retain(|x| x != s);
            Ok(())
        })?;
        self.inner.write().source_owner.remove(s);
        Ok(())
    }

    /// Moves the sources and versions of `others` into `keep` and deletes them.
    /// Setlists and crates keep working because version ids don't change.
    pub fn merge_tracks(&self, keep: &str, others: &[Id]) -> Result<()> {
        let mut moved = vec![];
        {
            let inner = self.inner.read();
            for o in others.iter().filter(|o| o.as_str() != keep) {
                if let Some(t) = inner.tracks.get(o) {
                    moved.push(t.clone());
                }
            }
        }
        self.with_track(keep, |k| {
            for o in &moved {
                for s in &o.sources {
                    if !k.sources.contains(s) {
                        k.sources.push(s.clone());
                    }
                }
                k.versions.extend(o.versions.iter().cloned());
                k.aliases.push(o.id.clone());
                k.aliases.extend(o.aliases.iter().cloned());
                if k.notes.is_empty() {
                    k.notes = o.notes.clone();
                }
            }
            Ok(())
        })?;
        let lib = self.library_dir();
        let mut inner = self.inner.write();
        for o in &moved {
            inner.tracks.remove(&o.id);
            let _ = fs::remove_file(lib.join("tracks").join(format!("{}.json", o.id)));
            let _ = fs::remove_file(self.local.join("waveforms").join(format!("{}.bin", o.id)));
        }
        Ok(())
    }

    pub fn update_track(&self, id: &str, p: &TrackPatch) -> Result<()> {
        self.with_track(id, |t| {
            if let Some(v) = &p.title { t.title = v.clone(); }
            if let Some(v) = &p.artist { t.artist = v.clone(); }
            if let Some(v) = &p.album { t.album = v.clone(); }
            if let Some(v) = &p.genre { t.genre = v.clone(); }
            if let Some(v) = &p.bpm {
                t.bpm = *v;
                t.bpm_source = v.map(|_| p.bpm_source.clone().unwrap_or_else(|| "user".into()));
            }
            if let Some(v) = &p.beats_per_bar {
                t.beats_per_bar = v.or(t.detected_meter);
                t.meter_source = match v {
                    Some(_) => Some("user".into()),
                    None => t.detected_meter.map(|_| "detected".into()),
                };
            }
            if let Some(v) = &p.rating { t.rating = *v; }
            if let Some(v) = &p.gain_db { t.gain_db = *v; }
            if let Some(v) = &p.notes { t.notes = v.clone(); }
            if let Some(v) = &p.preferred_source { t.preferred_source = v.clone(); }
            Ok(())
        })
    }

    /// A BPM the DJ entered (or from tags), which analysis should use for the meter.
    pub fn known_bpm(&self, id: &str) -> Option<f64> {
        self.track(id).ok().filter(|t| t.rec.bpm_source.as_deref() != Some("detected")).and_then(|t| t.rec.bpm)
    }

    pub fn set_analysis(&self, id: &str, a: &Analysis) -> Result<()> {
        let id = self.inner.read().canonical(id);
        if !a.waveform.is_empty() {
            write_atomic(&self.local.join("waveforms").join(format!("{id}.bin")), &a.waveform)?;
        }
        self.with_track(&id, |t| {
            t.analyzed = true;
            t.analysis_version = crate::analysis::VERSION;
            t.loudness_db = a.loudness_db.map(|l| (l * 10.0).round() / 10.0);
            t.audio_start_ms = a.audio_start_ms;
            t.audio_end_ms = a.audio_end_ms;
            if a.duration_ms > 0 {
                t.duration_ms = a.duration_ms;
            }
            t.detected_bpm = a.bpm;
            t.detected_meter = a.meter;
            if t.meter_source.as_deref() != Some("user") {
                t.beats_per_bar = a.meter;
                t.meter_source = a.meter.map(|_| "detected".into());
            }
            // Never overwrite a BPM the DJ entered (or one from tags).
            if t.bpm.is_none() || t.bpm_source.as_deref() == Some("detected") {
                t.bpm = a.bpm;
                t.bpm_source = a.bpm.map(|_| "detected".into());
            }
            Ok(())
        })
    }

    /// Sets (`Some`) or clears (`None`) hot cue `slot` on a track.
    pub fn set_hot_cue(&self, id: &str, slot: u8, ms: Option<i64>) -> Result<()> {
        self.with_track(id, |t| {
            t.hot_cues.retain(|c| c.slot != slot);
            if let Some(ms) = ms {
                t.hot_cues.push(HotCue { slot, ms: ms.max(0) });
                t.hot_cues.sort_by_key(|c| c.slot);
            }
            Ok(())
        })
    }

    pub fn set_in_library(&self, id: &str, value: bool) -> Result<()> {
        self.with_track(id, |t| {
            t.in_library = value;
            Ok(())
        })
    }

    /// Deletes set-only tracks that no setlist or crate references any more.
    /// `keep` protects tracks that are loaded on a deck.
    pub fn prune_unused(&self, keep: &[Id]) -> Result<usize> {
        let doomed: Vec<Id> = {
            let inner = self.inner.read();
            let mut used: std::collections::HashSet<&str> = std::collections::HashSet::new();
            for s in inner.setlists.values() {
                for e in &s.entries {
                    if let Some(v) = &e.version_id {
                        used.insert(v);
                    }
                }
            }
            for c in inner.crates.values() {
                used.extend(c.version_ids.iter().map(|v| v.as_str()));
            }
            inner
                .tracks
                .values()
                .filter(|t| !t.in_library && !keep.contains(&t.id))
                .filter(|t| !t.versions.iter().any(|v| used.contains(v.id.as_str())))
                .map(|t| t.id.clone())
                .collect()
        };
        self.delete_tracks(&doomed)?;
        Ok(doomed.len())
    }

    /// Reverts the BPM to the analysed value (`detected`) or the file's tag
    /// (`tag`, passing the tag value if the record doesn't have it yet).
    /// Returns `true` if the track must be re-analysed first.
    pub fn reset_bpm(&self, id: &str, from: &str, tag_bpm: Option<f64>) -> Result<bool> {
        self.with_track(id, |t| match from {
            "tag" => {
                if tag_bpm.is_some() {
                    t.tag_bpm = tag_bpm;
                }
                let b = t.tag_bpm.ok_or_else(|| anyhow!("This song has no BPM in its tags."))?;
                t.bpm = Some(b);
                t.bpm_source = Some("tag".into());
                Ok(false)
            }
            "detected" => match t.detected_bpm {
                Some(b) => {
                    t.bpm = Some(b);
                    t.bpm_source = Some("detected".into());
                    Ok(false)
                }
                None => {
                    // Not analysed since this field existed: clear and re-analyse.
                    t.bpm = None;
                    t.bpm_source = None;
                    Ok(true)
                }
            },
            other => Err(anyhow!("unknown BPM source {other}")),
        })
    }

    pub fn waveform(&self, id: &str) -> Option<Vec<u8>> {
        let id = self.inner.read().canonical(id);
        fs::read(self.local.join("waveforms").join(format!("{id}.bin"))).ok()
    }

    /// Tracks with a file on this machine but no analysis (or no cached waveform).
    pub fn needs_analysis(&self) -> Vec<Id> {
        let inner = self.inner.read();
        inner
            .tracks
            .values()
            .filter(|t| {
                let has_file = t.sources.iter().any(|s| s.kind == "file" && inner.by_hash.contains_key(&s.key));
                has_file
                    && (!t.analyzed
                        || t.analysis_version < crate::analysis::VERSION
                        || !self.local.join("waveforms").join(format!("{}.bin", t.id)).exists())
            })
            .map(|t| t.id.clone())
            .collect()
    }

    pub fn delete_tracks(&self, ids: &[Id]) -> Result<()> {
        let lib = self.library_dir();
        let mut changed_setlists = vec![];
        let mut changed_crates = vec![];
        {
            let mut inner = self.inner.write();
            for id in ids {
                let Some(t) = inner.tracks.remove(id) else { continue };
                let vids: Vec<Id> = t.versions.iter().map(|v| v.id.clone()).collect();
                for s in &t.sources {
                    inner.source_owner.remove(s);
                }
                for v in &vids {
                    inner.version_owner.remove(v);
                }
                for s in inner.setlists.values_mut() {
                    let mut hit = false;
                    for e in s.entries.iter_mut() {
                        if e.version_id.as_ref().map(|v| vids.contains(v)).unwrap_or(false) {
                            e.version_id = None;
                            hit = true;
                        }
                    }
                    if hit {
                        changed_setlists.push(s.clone());
                    }
                }
                for c in inner.crates.values_mut() {
                    let before = c.version_ids.len();
                    c.version_ids.retain(|v| !vids.contains(v));
                    if c.version_ids.len() != before {
                        changed_crates.push(c.clone());
                    }
                }
                let _ = fs::remove_file(lib.join("tracks").join(format!("{id}.json")));
                let _ = fs::remove_file(self.local.join("waveforms").join(format!("{id}.bin")));
            }
        }
        for s in changed_setlists {
            self.save_setlist(&s)?;
        }
        for c in changed_crates {
            self.save_crate(&c)?;
        }
        Ok(())
    }

    /// Songs played in setlists (newest first). Deleting a setlist removes its plays.
    pub fn history(&self, limit: usize) -> Vec<HistoryItem> {
        let inner = self.inner.read();
        let mut out: Vec<HistoryItem> = vec![];
        for s in inner.setlists.values() {
            for e in &s.entries {
                let (Some(at), Some(vid)) = (e.played_at, e.version_id.as_ref()) else { continue };
                if !(e.status == "played" || e.status == "playing") {
                    continue;
                }
                let Some(tid) = inner.version_owner.get(vid) else { continue };
                let styles = e.styles.clone().unwrap_or_else(|| {
                    inner.tracks.get(tid).and_then(|t| t.versions.iter().find(|v| &v.id == vid)).map(|v| v.styles.clone()).unwrap_or_default()
                });
                out.push(HistoryItem {
                    played_at: at,
                    track_id: tid.clone(),
                    version_id: Some(vid.clone()),
                    styles,
                    setlist_id: s.id.clone(),
                    entry_id: e.id.clone(),
                });
            }
        }
        out.sort_by(|a, b| b.played_at.cmp(&a.played_at));
        out.truncate(limit);
        out
    }

    // ---------------------------------------------------------------- versions

    /// A version and the id of the track that owns it.
    pub fn version(&self, id: &str) -> Result<(Version, Id)> {
        let inner = self.inner.read();
        let tid = inner.version_owner.get(id).ok_or_else(|| anyhow!("version {id} not found"))?;
        let t = inner.tracks.get(tid).ok_or_else(|| anyhow!("track {tid} not found"))?;
        let v = t.versions.iter().find(|v| v.id == id).ok_or_else(|| anyhow!("version {id} not found"))?;
        Ok((v.clone(), tid.clone()))
    }

    pub fn version_create(&self, track_id: &str, mut v: Version) -> Result<Version> {
        v.id = new_id();
        let out = v.clone();
        self.with_track(track_id, |t| {
            t.versions.push(v);
            Ok(())
        })?;
        Ok(out)
    }

    pub fn version_update(&self, id: &str, p: &VersionPatch) -> Result<Version> {
        let (_, tid) = self.version(id)?;
        self.with_track(&tid, |t| {
            let v = t.versions.iter_mut().find(|v| v.id == id).ok_or_else(|| anyhow!("version not found"))?;
            if let Some(x) = &p.name { v.name = x.clone(); }
            if let Some(x) = &p.display_name { v.display_name = x.clone().filter(|s| !s.trim().is_empty()); }
            if let Some(x) = &p.styles { v.styles = dedup(x); }
            if let Some(x) = &p.target_bpm { v.settings.target_bpm = *x; }
            if let Some(x) = &p.cue_in_ms { v.settings.cue_in_ms = *x; }
            if let Some(x) = &p.cue_out_ms { v.settings.cue_out_ms = *x; }
            if let Some(x) = &p.fade_out_ms { v.settings.fade_out_ms = *x; }
            if let Some(x) = &p.gain_db { v.settings.gain_db = *x; }
            if let Some(x) = &p.stems { v.settings.stems = x.clone(); }
            if let Some(x) = &p.comment { v.comment = x.clone(); }
            if let Some(x) = &p.tags { v.tags = x.clone(); }
            Ok(v.clone())
        })
    }

    pub fn version_delete(&self, id: &str) -> Result<()> {
        let (_, tid) = self.version(id)?;
        let used: usize = self
            .inner
            .read()
            .setlists
            .values()
            .map(|s| s.entries.iter().filter(|e| e.version_id.as_deref() == Some(id)).count())
            .sum();
        if used > 0 {
            return Err(anyhow!("this version is used in {used} setlist entr{}", if used == 1 { "y" } else { "ies" }));
        }
        self.with_track(&tid, |t| {
            if t.versions.len() <= 1 {
                return Err(anyhow!("a track needs at least one version"));
            }
            t.versions.retain(|v| v.id != id);
            Ok(())
        })?;
        let mut inner = self.inner.write();
        inner.version_owner.remove(id);
        let mut changed = vec![];
        for c in inner.crates.values_mut() {
            if c.version_ids.iter().any(|v| v == id) {
                c.version_ids.retain(|v| v != id);
                changed.push(c.clone());
            }
        }
        drop(inner);
        for c in changed {
            self.save_crate(&c)?;
        }
        Ok(())
    }

    // ---------------------------------------------------------------- file index

    pub fn file_info(&self, path: &str) -> Option<FileEntry> {
        self.inner.read().index.files.get(path).cloned()
    }

    pub fn upsert_file(&self, path: &str, e: FileEntry) {
        let mut inner = self.inner.write();
        let mut e = e;
        if e.root.is_none() {
            e.root = inner.index.files.get(path).and_then(|old| old.root.clone());
        }
        inner.index.files.insert(path.to_string(), e);
        inner.rebuild_hash_index();
    }

    pub fn all_file_paths(&self) -> Vec<String> {
        self.inner.read().index.files.keys().cloned().collect()
    }

    pub fn remove_file(&self, path: &str) {
        let mut inner = self.inner.write();
        inner.index.files.remove(path);
        inner.rebuild_hash_index();
    }

    pub fn track_file_paths(&self, track_id: &str) -> Vec<String> {
        self.track(track_id).map(|t| t.files).unwrap_or_default()
    }

    // ---------------------------------------------------------------- crates

    pub fn crates(&self) -> Vec<Crate> {
        let mut v: Vec<Crate> = self.inner.read().crates.values().cloned().collect();
        v.sort_by(|a, b| (a.position, &a.name).cmp(&(b.position, &b.name)));
        v
    }

    /// Creates a crate; with `smart` rules it's a smart crate.
    pub fn crate_create(&self, name: &str, smart: Option<serde_json::Value>) -> Result<Id> {
        let pos = self.inner.read().crates.values().map(|c| c.position).max().unwrap_or(0) + 1;
        let c = Crate { id: new_id(), name: name.into(), position: pos, version_ids: vec![], smart };
        self.save_crate(&c)?;
        let id = c.id.clone();
        self.inner.write().crates.insert(id.clone(), c);
        Ok(id)
    }

    pub fn crate_rename(&self, id: &str, name: &str) -> Result<()> {
        self.with_crate(id, |c| c.name = name.into())
    }

    pub fn crate_delete(&self, id: &str) -> Result<()> {
        self.inner.write().crates.remove(id);
        let _ = fs::remove_file(self.lib_path("crates", id));
        Ok(())
    }

    /// Replaces a smart crate's rules (or turns a smart crate into a plain one with `None`).
    pub fn crate_set_smart(&self, id: &str, smart: Option<serde_json::Value>) -> Result<()> {
        self.with_crate(id, |c| c.smart = smart)
    }

    pub fn crate_add(&self, id: &str, version_ids: &[Id]) -> Result<()> {
        self.with_crate(id, |c| {
            for v in version_ids {
                if !c.version_ids.contains(v) {
                    c.version_ids.push(v.clone());
                }
            }
        })
    }

    /// Adds a dance style to a version (a version can have several).
    pub fn assign_style(&self, version_id: &str, style: &str) -> Result<()> {
        let (v, _) = self.version(version_id)?;
        if !v.styles.iter().any(|s| s == style) {
            let mut styles = v.styles.clone();
            styles.push(style.to_string());
            self.version_update(version_id, &VersionPatch { styles: Some(styles), ..Default::default() })?;
        }
        Ok(())
    }

    /// Removes one style (or all with `None`) from a version. The version is
    /// kept, so setlists that use it keep working.
    pub fn unassign_style(&self, version_id: &str, style: Option<&str>) -> Result<()> {
        let (v, _) = self.version(version_id)?;
        let styles = match style {
            Some(st) => v.styles.iter().filter(|s| *s != st).cloned().collect(),
            None => vec![],
        };
        self.version_update(version_id, &VersionPatch { styles: Some(styles), ..Default::default() })?;
        Ok(())
    }

    /// Adds (`on`) or removes one of your tags on a version.
    pub fn set_version_tag(&self, version_id: &str, tag: &str, on: bool) -> Result<()> {
        let (v, _) = self.version(version_id)?;
        let has = v.tags.iter().any(|t| t == tag);
        if has != on {
            let tags = if on { [v.tags.clone(), vec![tag.to_string()]].concat() } else { v.tags.iter().filter(|t| *t != tag).cloned().collect() };
            self.version_update(version_id, &VersionPatch { tags: Some(tags), ..Default::default() })?;
        }
        Ok(())
    }

    pub fn crate_remove(&self, id: &str, version_ids: &[Id]) -> Result<()> {
        self.with_crate(id, |c| c.version_ids.retain(|v| !version_ids.contains(v)))
    }

    // ---------------------------------------------------------------- setlists

    pub fn setlists(&self) -> Vec<SetlistSummary> {
        let mut v: Vec<SetlistSummary> = self.inner.read().setlists.values().map(SetlistSummary::from).collect();
        v.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        v
    }

    pub fn setlist(&self, id: &str) -> Result<Setlist> {
        self.inner.read().setlists.get(id).cloned().ok_or_else(|| anyhow!("setlist {id} not found"))
    }

    pub fn setlist_create(&self, name: &str, mode: &str, profile_id: &str, date: Option<String>) -> Result<Id> {
        let now = now_ms();
        let s = Setlist {
            id: new_id(),
            name: name.into(),
            event_date: date,
            profile_id: profile_id.into(),
            mode: mode.into(),
            created_at: now,
            updated_at: now,
            ..Default::default()
        };
        self.save_setlist(&s)?;
        let id = s.id.clone();
        self.inner.write().setlists.insert(id.clone(), s);
        Ok(id)
    }

    pub fn setlist_update(&self, id: &str, p: &SetlistPatch) -> Result<()> {
        self.with_setlist(id, |s| {
            if let Some(v) = &p.name { s.name = v.clone(); }
            if let Some(v) = &p.event_date { s.event_date = v.clone(); }
            if let Some(v) = &p.profile_id { s.profile_id = v.clone(); }
            if let Some(v) = &p.notes { s.notes = v.clone(); }
            if let Some(v) = &p.mode { s.mode = v.clone(); }
            Ok(())
        })
    }

    pub fn setlist_delete(&self, id: &str) -> Result<()> {
        self.inner.write().setlists.remove(id);
        let _ = fs::remove_file(self.lib_path("setlists", id));
        Ok(())
    }

    pub fn setlist_duplicate(&self, id: &str, name: &str) -> Result<Id> {
        let mut s = self.setlist(id)?;
        let now = now_ms();
        s.id = new_id();
        s.name = name.into();
        s.created_at = now;
        s.updated_at = now;
        for e in s.entries.iter_mut() {
            e.id = new_id();
            e.status = "pending".into();
            e.played_at = None;
        }
        self.save_setlist(&s)?;
        let nid = s.id.clone();
        let mut inner = self.inner.write();
        inner.reindex_setlist(&s);
        inner.setlists.insert(nid.clone(), s);
        Ok(nid)
    }

    pub fn entries(&self, setlist_id: &str) -> Vec<SetlistEntry> {
        self.inner.read().setlists.get(setlist_id).map(|s| s.entries.clone()).unwrap_or_default()
    }

    /// An entry and the id of its setlist.
    pub fn entry(&self, id: &str) -> Result<(SetlistEntry, Id)> {
        let inner = self.inner.read();
        let sid = inner.entry_owner.get(id).ok_or_else(|| anyhow!("setlist entry {id} not found"))?;
        let e = inner
            .setlists
            .get(sid)
            .and_then(|s| s.entries.iter().find(|e| e.id == id))
            .ok_or_else(|| anyhow!("setlist entry {id} not found"))?;
        Ok((e.clone(), sid.clone()))
    }

    /// Inserts entries before index `position`; `None` appends.
    pub fn entries_insert(&self, setlist_id: &str, position: Option<usize>, new: &[NewEntry]) -> Result<Vec<Id>> {
        self.with_setlist(setlist_id, |s| {
            let at = position.unwrap_or(s.entries.len()).min(s.entries.len());
            let items: Vec<SetlistEntry> = new
                .iter()
                .map(|n| SetlistEntry {
                    id: new_id(),
                    version_id: n.version_id.clone(),
                    styles: n.styles.as_ref().map(|s| dedup(s)),
                    label: n.label.clone().filter(|l| !l.trim().is_empty()),
                    overrides: n.overrides.clone(),
                    note: n.note.clone(),
                    tags: n.tags.clone(),
                    ..Default::default()
                })
                .collect();
            let ids = items.iter().map(|e| e.id.clone()).collect();
            s.entries.splice(at..at, items);
            Ok(ids)
        })
    }

    pub fn entry_update(&self, id: &str, p: &EntryPatch) -> Result<SetlistEntry> {
        let (_, sid) = self.entry(id)?;
        self.with_setlist(&sid, |s| {
            let e = s.entries.iter_mut().find(|e| e.id == id).ok_or_else(|| anyhow!("entry not found"))?;
            if let Some(v) = &p.version_id { e.version_id = v.clone(); }
            if let Some(v) = &p.styles { e.styles = v.as_ref().map(|s| dedup(s)); }
            if let Some(v) = &p.label { e.label = v.clone().filter(|l| !l.trim().is_empty()); }
            if let Some(v) = &p.overrides { e.overrides = v.clone(); }
            if let Some(v) = &p.note { e.note = v.clone(); }
            if let Some(v) = &p.tags { e.tags = v.clone(); }
            if let Some(v) = &p.status {
                e.status = v.clone();
                if v == "pending" {
                    e.played_at = None;
                }
            }
            Ok(e.clone())
        })
    }

    pub fn entry_set_playing(&self, id: &str) -> Result<()> {
        let (_, sid) = self.entry(id)?;
        self.with_setlist(&sid, |s| {
            if let Some(e) = s.entries.iter_mut().find(|e| e.id == id) {
                e.status = "playing".into();
                e.played_at = Some(now_ms());
            }
            Ok(())
        })
    }

    pub fn entries_delete(&self, setlist_id: &str, ids: &[Id]) -> Result<()> {
        self.with_setlist(setlist_id, |s| {
            s.entries.retain(|e| !ids.contains(&e.id));
            Ok(())
        })?;
        let mut inner = self.inner.write();
        for id in ids {
            inner.entry_owner.remove(id);
        }
        Ok(())
    }

    pub fn entries_reorder(&self, setlist_id: &str, ordered: &[Id]) -> Result<()> {
        self.with_setlist(setlist_id, |s| {
            let mut out: Vec<SetlistEntry> = vec![];
            for id in ordered {
                if let Some(pos) = s.entries.iter().position(|e| &e.id == id) {
                    out.push(s.entries.remove(pos));
                }
            }
            // Entries the caller didn't mention keep their relative order at the end.
            out.append(&mut s.entries);
            s.entries = out;
            Ok(())
        })
    }

    pub fn reset_setlist_status(&self, setlist_id: &str) -> Result<()> {
        self.with_setlist(setlist_id, |s| {
            for e in s.entries.iter_mut() {
                e.status = "pending".into();
                e.played_at = None;
            }
            Ok(())
        })
    }

    // ---------------------------------------------------------------- your styles

    /// Your styles and club mappings (`None` until first saved / seeded).
    pub fn style_library(&self) -> Option<StyleLibrary> {
        self.inner.read().styles.clone()
    }

    pub fn save_style_library(&self, lib: StyleLibrary) -> Result<()> {
        let mut seen = std::collections::HashSet::new();
        for s in &lib.styles {
            if s.key.trim().is_empty() || !seen.insert(s.key.clone()) {
                return Err(anyhow!("every style needs a unique, non-empty key (\"{}\")", s.key));
            }
        }
        let mut seen = std::collections::HashSet::new();
        for t in &lib.tags {
            if t.key.trim().is_empty() || !seen.insert(t.key.clone()) {
                return Err(anyhow!("every tag needs a unique, non-empty key (\"{}\")", t.key));
            }
        }
        write_json(&self.library_dir().join("styles.json"), &lib)?;
        self.inner.write().styles = Some(lib);
        Ok(())
    }

    /// Style keys used by versions, with how many versions use each.
    pub fn style_usage(&self) -> BTreeMap<String, usize> {
        self.version_key_usage(|v| &v.styles)
    }

    /// Your tag keys used by versions, with how many versions use each.
    pub fn tag_usage(&self) -> BTreeMap<String, usize> {
        self.version_key_usage(|v| &v.tags)
    }

    fn version_key_usage(&self, keys: impl Fn(&Version) -> &Vec<String>) -> BTreeMap<String, usize> {
        let mut m = BTreeMap::new();
        for t in self.inner.read().tracks.values() {
            for v in &t.versions {
                for s in keys(v) {
                    *m.entry(s.clone()).or_insert(0) += 1;
                }
            }
        }
        m
    }

    /// Replaces one of your tag keys on every version and smart-crate
    /// condition (`None` removes it).
    pub fn rekey_tag(&self, from: &str, to: Option<&str>) -> Result<()> {
        let (tracks, crates): (Vec<TrackRecord>, Vec<Crate>) = {
            let mut inner = self.inner.write();
            let mut ts = vec![];
            for t in inner.tracks.values_mut() {
                let mut hit = false;
                for v in t.versions.iter_mut().filter(|v| v.tags.iter().any(|s| s == from)) {
                    v.tags = rekey(&v.tags, from, to);
                    hit = true;
                }
                if hit {
                    ts.push(t.clone());
                }
            }
            let mut cs = vec![];
            for c in inner.crates.values_mut() {
                let Some(conds) = c.smart.as_mut().and_then(|v| v.get_mut("conditions")).and_then(|v| v.as_array_mut()) else {
                    continue;
                };
                let before = conds.clone();
                conds.retain_mut(|cond| {
                    if cond["field"].as_str() != Some("tag") || cond["value"].as_str() != Some(from) {
                        return true;
                    }
                    match to {
                        Some(t) => {
                            cond["value"] = serde_json::Value::String(t.to_string());
                            true
                        }
                        None => false,
                    }
                });
                if *conds != before {
                    cs.push(c.clone());
                }
            }
            (ts, cs)
        };
        for t in &tracks {
            self.save_track(t)?;
        }
        for c in &crates {
            self.save_crate(c)?;
        }
        Ok(())
    }

    /// Replaces a style key on every version, setlist entry and smart-crate
    /// condition (`None` removes it).
    pub fn rekey_style(&self, from: &str, to: Option<&str>) -> Result<()> {
        let (tracks, setlists, crates): (Vec<TrackRecord>, Vec<Setlist>, Vec<Crate>) = {
            let mut inner = self.inner.write();
            let mut ts = vec![];
            for t in inner.tracks.values_mut() {
                let mut hit = false;
                for v in t.versions.iter_mut().filter(|v| v.styles.iter().any(|s| s == from)) {
                    v.styles = rekey(&v.styles, from, to);
                    hit = true;
                }
                if hit {
                    ts.push(t.clone());
                }
            }
            let mut ss = vec![];
            for s in inner.setlists.values_mut() {
                let mut hit = false;
                for e in s.entries.iter_mut() {
                    if let Some(st) = e.styles.as_mut().filter(|st| st.iter().any(|s| s == from)) {
                        *st = rekey(st, from, to);
                        hit = true;
                    }
                }
                if hit {
                    ss.push(s.clone());
                }
            }
            // Smart crates that test for the style.
            let mut cs = vec![];
            for c in inner.crates.values_mut() {
                let Some(conds) = c.smart.as_mut().and_then(|v| v.get_mut("conditions")).and_then(|v| v.as_array_mut()) else {
                    continue;
                };
                let before = conds.len();
                let mut hit = false;
                conds.retain_mut(|cond| {
                    let refers = matches!(cond["field"].as_str(), Some("style" | "bpm")) && cond["value"].as_str() == Some(from);
                    if !refers {
                        return true;
                    }
                    hit = true;
                    match to {
                        Some(t) => {
                            cond["value"] = serde_json::Value::String(t.to_string());
                            true
                        }
                        None => false,
                    }
                });
                if hit || conds.len() != before {
                    cs.push(c.clone());
                }
            }
            (ts, ss, cs)
        };
        for t in &tracks {
            self.save_track(t)?;
        }
        for s in &setlists {
            self.save_setlist(s)?;
        }
        for c in &crates {
            self.save_crate(c)?;
        }
        Ok(())
    }

    // ---------------------------------------------------------------- profiles

    pub fn profiles(&self) -> Vec<serde_json::Value> {
        self.inner.read().profiles.values().cloned().collect()
    }

    pub fn profile_save(&self, value: serde_json::Value) -> Result<()> {
        let id = value["id"].as_str().filter(|s| !s.is_empty()).ok_or_else(|| anyhow!("profile needs an id"))?;
        if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err(anyhow!("profile ids may only contain letters, digits, - and _"));
        }
        write_json(&self.lib_path("profiles", id), &value)?;
        self.inner.write().profiles.insert(id.to_string(), value);
        Ok(())
    }

    pub fn profile_delete(&self, id: &str) -> Result<()> {
        self.inner.write().profiles.remove(id);
        let _ = fs::remove_file(self.lib_path("profiles", id));
        Ok(())
    }
}

/// Days since 1970-01-01 → (year, month, day). (Howard Hinnant's algorithm.)
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dirs() -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("orpheus-test-{}", new_id()));
        (base.join("lib"), base.join("local"))
    }

    #[test]
    fn bpm_can_be_reset_after_editing() {
        let (lib, local) = temp_dirs();
        let store = Store::open(&lib, &local).unwrap();
        let nt = NewTrack { title: "W".into(), bpm: Some(120.0), bpm_source: Some("tag".into()), ..Default::default() };
        let id = store.insert_track(&nt, SourceRef { kind: "file".into(), key: "h".into() }).unwrap();
        store.set_analysis(&id, &Analysis { bpm: Some(93.1), ..Default::default() }).unwrap();
        // Tag BPM wins over analysis, but the analysed value is remembered.
        assert_eq!(store.track(&id).unwrap().rec.bpm, Some(120.0));
        store.update_track(&id, &TrackPatch { bpm: Some(Some(139.6)), ..Default::default() }).unwrap();
        assert_eq!(store.track(&id).unwrap().rec.bpm_source.as_deref(), Some("user"));

        assert!(!store.reset_bpm(&id, "detected", None).unwrap());
        let t = store.track(&id).unwrap().rec;
        assert_eq!((t.bpm, t.bpm_source.as_deref()), (Some(93.1), Some("detected")));
        assert!(!store.reset_bpm(&id, "tag", None).unwrap());
        assert_eq!(store.track(&id).unwrap().rec.bpm, Some(120.0));

        // Survives a reload from disk.
        let store2 = Store::open(&lib, &local).unwrap();
        assert_eq!(store2.track(&id).unwrap().rec.detected_bpm, Some(93.1));
        let _ = fs::remove_dir_all(lib.parent().unwrap());
    }

    #[test]
    fn set_only_tracks_are_pruned_when_unused() {
        let (lib, local) = temp_dirs();
        let store = Store::open(&lib, &local).unwrap();
        let src = SourceRef { kind: "spotify".into(), key: "spotify:track:abc".into() };
        let id = store.insert_track(&NewTrack { title: "S".into(), ..Default::default() }, src).unwrap();
        store.set_in_library(&id, false).unwrap();
        let vid = store.track(&id).unwrap().rec.versions[0].id.clone();
        let sid = store.setlist_create("Set", "live", "dancebreak", None).unwrap();
        let eids = store.entries_insert(&sid, None, &[NewEntry { version_id: Some(vid), ..Default::default() }]).unwrap();
        assert_eq!(store.prune_unused(&[]).unwrap(), 0, "still used by the set");
        // Reloading keeps the flag (it's only written when false).
        let store = Store::open(&lib, &local).unwrap();
        assert!(!store.track(&id).unwrap().rec.in_library);
        store.entries_delete(&sid, &eids).unwrap();
        assert_eq!(store.prune_unused(&[id.clone()]).unwrap(), 0, "protected while on a deck");
        assert_eq!(store.prune_unused(&[]).unwrap(), 1);
        assert!(store.track(&id).is_err());
        let _ = fs::remove_dir_all(lib.parent().unwrap());
    }

    #[test]
    fn styles_are_labels_and_crates_mix_styles() {
        let (lib, local) = temp_dirs();
        let store = Store::open(&lib, &local).unwrap();
        let src = |k: &str| SourceRef { kind: "file".into(), key: k.into() };
        let a = store.insert_track(&NewTrack { title: "A".into(), ..Default::default() }, src("a")).unwrap();
        let b = store.insert_track(&NewTrack { title: "B".into(), ..Default::default() }, src("b")).unwrap();
        let va = store.track(&a).unwrap().rec.versions[0].id.clone();
        let vb = store.track(&b).unwrap().rec.versions[0].id.clone();

        // A version can carry several styles; adding is idempotent.
        store.assign_style(&va, "waltz").unwrap();
        store.assign_style(&va, "rotary-waltz").unwrap();
        store.assign_style(&va, "waltz").unwrap();
        assert_eq!(store.version(&va).unwrap().0.styles, vec!["waltz".to_string(), "rotary-waltz".to_string()]);
        let va2 = va.clone();

        // Older files with a single "style" are read into "styles".
        let path = lib.join("tracks").join(format!("{b}.json"));
        let text = fs::read_to_string(&path).unwrap().replace("\"name\": \"Original\"", "\"name\": \"Original\", \"style\": \"polka\"");
        fs::write(&path, text).unwrap();
        let reloaded = Store::open(&lib, &local).unwrap();
        assert_eq!(reloaded.version(&vb).unwrap().0.styles, vec!["polka".to_string()]);

        // Renaming a style key updates every version.
        store.rekey_style("waltz", Some("rotary-waltz")).unwrap();
        assert_eq!(store.version(&va).unwrap().0.styles, vec!["rotary-waltz".to_string()]);

        // One "Waltzes" crate can hold both styles.
        let waltzes = store.crate_create("Waltzes", None).unwrap();
        store.crate_add(&waltzes, &[va.clone(), va2.clone(), vb.clone()]).unwrap();
        assert_eq!(store.crates()[0].version_ids.len(), 2);

        // Smart crates keep their rules (opaque to the backend) on disk.
        let rules = serde_json::json!({ "match": "all", "conditions": [{ "field": "style", "op": "is", "value": "rotary-waltz" }] });
        let smart = store.crate_create("Rotary (smart)", Some(rules.clone())).unwrap();
        fs::write(lib.join("crates").join("old.json"), r#"{"id":"old","name":"Old","position":9,"versionIds":["x"]}"#).unwrap();
        let store2 = Store::open(&lib, &local).unwrap();
        assert!(store2.load_errors().is_empty(), "{:?}", store2.load_errors());
        let crates = store2.crates();
        assert_eq!(crates.iter().find(|c| c.id == smart).unwrap().smart.as_ref(), Some(&rules));
        assert_eq!(crates.iter().find(|c| c.id == "old").unwrap().version_ids, vec!["x".to_string()]);
        store2.crate_set_smart(&smart, None).unwrap();
        assert!(store2.crates().iter().find(|c| c.id == smart).unwrap().smart.is_none());

        store2.unassign_style(&va, Some("rotary-waltz")).unwrap();
        assert!(store2.version(&va).unwrap().0.styles.is_empty());
        let _ = fs::remove_dir_all(lib.parent().unwrap());
    }

    #[test]
    fn plays_and_history_come_from_setlists() {
        let (lib, local) = temp_dirs();
        let store = Store::open(&lib, &local).unwrap();
        let tid = store.insert_track(&NewTrack { title: "S".into(), ..Default::default() }, SourceRef { kind: "file".into(), key: "h".into() }).unwrap();
        let vid = store.track(&tid).unwrap().rec.versions[0].id.clone();
        let mut sets = vec![];
        for _ in 0..2 {
            let sid = store.setlist_create("Set", "live", "none", None).unwrap();
            // Played twice in the same set counts once for that set.
            let ids = store.entries_insert(&sid, None, &[NewEntry { version_id: Some(vid.clone()), ..Default::default() }, NewEntry { version_id: Some(vid.clone()), ..Default::default() }]).unwrap();
            for id in &ids {
                store.entry_set_playing(id).unwrap();
            }
            sets.push(sid);
        }
        assert_eq!(store.track(&tid).unwrap().play_count, 2);
        assert_eq!(store.history(10).len(), 4);
        store.setlist_delete(&sets[0]).unwrap();
        let reloaded = Store::open(&lib, &local).unwrap();
        assert_eq!(reloaded.track(&tid).unwrap().play_count, 1);
        assert_eq!(reloaded.history(10).len(), 2);
        assert!(reloaded.track(&tid).unwrap().last_played_at.is_some());
        let _ = fs::remove_dir_all(lib.parent().unwrap());
    }

    #[test]
    fn dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20374), (2025, 10, 13));
    }

    #[test]
    fn roundtrip_through_files() {
        let (lib, local) = temp_dirs();
        let store = Store::open(&lib, &local).unwrap();
        let tid = store
            .insert_track(&NewTrack { title: "Song".into(), ..Default::default() }, SourceRef { kind: "file".into(), key: "h1".into() })
            .unwrap();
        assert!(store.track(&tid).unwrap().missing);
        store.upsert_file("/music/a.flac", FileEntry { hash: "h1".into(), size: 1, mtime: 2, root: None });
        assert_eq!(store.track(&tid).unwrap().files, vec!["/music/a.flac".to_string()]);
        store.save_index().unwrap();

        let v = store
            .version_create(
                &tid,
                Version {
                    id: String::new(),
                    name: "Rotary @150".into(),
                    display_name: None,
                    styles: vec!["rotary-waltz".into()],
                    legacy_style: None,
                    settings: Overrides { target_bpm: Some(150.0), ..Default::default() },
                    comment: String::new(),
                    tags: vec![],
                },
            )
            .unwrap();
        let sid = store.setlist_create("Social", "preset", "dancebreak", None).unwrap();
        let mk = |n: &str| NewEntry { note: n.into(), ..Default::default() };
        store.entries_insert(&sid, None, &[mk("a"), mk("b"), mk("c")]).unwrap();
        let ids = store.entries_insert(&sid, Some(1), &[NewEntry { version_id: Some(v.id.clone()), ..Default::default() }]).unwrap();
        let notes: Vec<_> = store.entries(&sid).into_iter().map(|e| e.note).collect();
        assert_eq!(notes, ["a", "", "b", "c"]);
        store.entry_update(&ids[0], &EntryPatch { overrides: Some(Overrides { target_bpm: Some(148.0), ..Default::default() }), ..Default::default() }).unwrap();
        let played = store.entries_insert(&sid, None, &[NewEntry { version_id: Some(v.id.clone()), ..Default::default() }]).unwrap();
        store.entry_set_playing(&played[0]).unwrap();

        // Everything survives a reload from disk.
        let store2 = Store::open(&lib, &local).unwrap();
        assert!(store2.load_errors().is_empty(), "{:?}", store2.load_errors());
        let t = store2.track(&tid).unwrap();
        assert_eq!(t.rec.versions.len(), 2);
        assert_eq!(t.play_count, 1);
        assert_eq!(t.files.len(), 1);
        let (e, _) = store2.entry(&ids[0]).unwrap();
        assert_eq!(e.overrides.target_bpm, Some(148.0));
        assert_eq!(store2.version(&v.id).unwrap().1, tid);

        // Merging keeps version ids (and so setlist references) intact.
        let sp = store2
            .insert_track(&NewTrack { title: "Song".into(), ..Default::default() }, SourceRef { kind: "spotify".into(), key: "spotify:track:x".into() })
            .unwrap();
        store2.merge_tracks(&tid, &[sp.clone()]).unwrap();
        let t = store2.track(&tid).unwrap();
        assert_eq!(t.rec.sources.len(), 2);
        assert_eq!(t.rec.versions.len(), 3);
        assert_eq!(store2.track(&sp).unwrap().rec.id, tid, "old id resolves via alias");

        // A hand-edited, broken file is reported instead of failing the load.
        fs::write(lib.join("tracks").join("broken.json"), "{ not json").unwrap();
        let store3 = Store::open(&lib, &local).unwrap();
        assert_eq!(store3.load_errors().len(), 1);
        assert_eq!(store3.all_tracks().len(), 1);
        let _ = fs::remove_dir_all(lib.parent().unwrap());
    }
}

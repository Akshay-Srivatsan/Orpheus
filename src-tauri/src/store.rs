//! Persistence as plain files.
//!
//! The *library directory* is meant to be version-controlled or synced
//! (git, Dropbox, Syncthing...). Everything in it is pretty-printed JSON with
//! stable field order, one object per file, so diffs and merges stay small:
//!
//! ```text
//! <library>/
//!   tracks/<id>.json      a song: title etc. and its versions (each plays a recording)
//!   recordings/<id>.json  a recording: sources (the same audio), analysis, hot cues
//!   setlists/<id>.json    a setlist and its entries
//!   crates/<id>.json      a hand-picked collection of versions
//!   clubs/<club>/         club guidelines, one TOML file per version (see club.rs)
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
    pub fingerprint: Option<String>,
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
    /// Songs.
    tracks: HashMap<Id, TrackRecord>,
    recordings: HashMap<Id, RecordingRecord>,
    /// Version id → song id.
    version_owner: HashMap<Id, Id>,
    /// Source → recording id.
    source_owner: HashMap<SourceRef, Id>,
    /// Combined songs' old ids → song id.
    alias: HashMap<Id, Id>,
    /// Merged recordings' old ids → recording id.
    rec_alias: HashMap<Id, Id>,
    setlists: HashMap<Id, Setlist>,
    entry_owner: HashMap<Id, Id>,
    crates: HashMap<Id, Crate>,
    index: FileIndex,
    by_hash: HashMap<String, Vec<String>>,
    load_errors: Vec<String>,
    styles: Option<StyleLibrary>,
    excluded: Excluded,
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

/// A song's first version, playing `recording`.
fn original_version(recording: &str) -> Version {
    Version {
        id: new_id(),
        name: "Original".into(),
        recording: recording.to_string(),
        display_name: None,
        styles: vec![],
        legacy_style: None,
        settings: Overrides::default(),
        comment: String::new(),
        tags: vec![],
    }
}

impl Inner {
    fn reindex_track(&mut self, t: &TrackRecord) {
        for v in &t.versions {
            self.version_owner.insert(v.id.clone(), t.id.clone());
        }
        for a in &t.aliases {
            self.alias.insert(a.clone(), t.id.clone());
        }
    }

    fn reindex_recording(&mut self, r: &RecordingRecord) {
        for s in &r.sources {
            self.source_owner.insert(s.clone(), r.id.clone());
        }
        for a in &r.aliases {
            self.rec_alias.insert(a.clone(), r.id.clone());
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

    fn canonical_rec(&self, rec_id: &str) -> Id {
        self.rec_alias.get(rec_id).cloned().unwrap_or_else(|| rec_id.to_string())
    }

    /// Recording id → the songs whose versions play it.
    fn recording_users(&self) -> HashMap<Id, Vec<Id>> {
        let mut out: HashMap<Id, Vec<Id>> = HashMap::new();
        let mut songs: Vec<&TrackRecord> = self.tracks.values().collect();
        songs.sort_by(|a, b| a.id.cmp(&b.id));
        for t in songs {
            for v in &t.versions {
                let users = out.entry(self.canonical_rec(&v.recording)).or_default();
                if !users.contains(&t.id) {
                    users.push(t.id.clone());
                }
            }
        }
        out
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

    fn to_recording(&self, rec: &RecordingRecord, users: &HashMap<Id, Vec<Id>>) -> Recording {
        let mut files = vec![];
        let mut file_paths = BTreeMap::new();
        for s in rec.sources.iter().filter(|s| s.kind == "file") {
            if let Some(paths) = self.by_hash.get(&s.key) {
                files.extend(paths.iter().cloned());
                let mut p = paths.clone();
                p.sort();
                file_paths.insert(s.key.clone(), p);
            }
        }
        files.sort();
        let has_spotify = rec.sources.iter().any(|s| s.kind == "spotify");
        Recording {
            missing: files.is_empty() && !has_spotify,
            files,
            file_paths,
            songs: users.get(&rec.id).cloned().unwrap_or_default(),
            rec: rec.clone(),
        }
    }

    fn to_track(&self, rec: &TrackRecord, stats: &HashMap<Id, (i64, i64)>, users: &HashMap<Id, Vec<Id>>) -> Track {
        let mut recordings: Vec<Recording> = vec![];
        for v in &rec.versions {
            let id = self.canonical_rec(&v.recording);
            if recordings.iter().any(|r| r.rec.id == id) {
                continue;
            }
            if let Some(r) = self.recordings.get(&id) {
                recordings.push(self.to_recording(r, users));
            }
        }
        let (count, last) = stats.get(&rec.id).copied().unwrap_or((0, 0));
        let mut rec = rec.clone();
        // Versions name their recording by its current id.
        for v in rec.versions.iter_mut() {
            v.recording = self.canonical_rec(&v.recording);
        }
        Track {
            missing: recordings.iter().all(|r| r.missing),
            recordings,
            play_count: count,
            last_played_at: (last > 0).then_some(last),
            rec,
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
        for d in ["tracks", "recordings", "setlists", "crates", "clubs"] {
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
        for p in json_files(&library_dir.join("recordings")) {
            match read_json::<RecordingRecord>(&p) {
                Ok(r) if !r.id.is_empty() => {
                    inner.reindex_recording(&r);
                    inner.recordings.insert(r.id.clone(), r);
                }
                Ok(_) => errors.push(format!("{}: missing id", p.display())),
                Err(e) => errors.push(format!("{e:#}")),
            }
        }
        // Older songs kept their recording inside: move it out.
        let mut upgraded: Vec<(TrackRecord, Option<RecordingRecord>)> = vec![];
        for p in json_files(&library_dir.join("tracks")) {
            match read_json::<TrackRecord>(&p) {
                Ok(mut t) if !t.id.is_empty() => {
                    t.versions.iter_mut().for_each(Version::normalize);
                    let legacy = t.take_legacy_recording();
                    if t.versions.iter().any(|v| v.recording.is_empty()) {
                        let rid = legacy.as_ref().map(|r| r.id.clone()).unwrap_or_else(|| t.id.clone());
                        for v in t.versions.iter_mut().filter(|v| v.recording.is_empty()) {
                            v.recording = rid.clone();
                        }
                        let new = legacy.filter(|r| !inner.recordings.contains_key(&r.id));
                        if let Some(r) = &new {
                            inner.reindex_recording(r);
                            inner.recordings.insert(r.id.clone(), r.clone());
                        }
                        upgraded.push((t.clone(), new));
                    }
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
        let styles_path = library_dir.join("styles.json");
        if styles_path.exists() {
            match read_json::<StyleLibrary>(&styles_path) {
                Ok(mut s) => {
                    s.upgrade();
                    inner.styles = Some(s);
                }
                Err(e) => errors.push(format!("{e:#}")),
            }
        }
        let excluded_path = library_dir.join("excluded.json");
        if excluded_path.exists() {
            match read_json::<Excluded>(&excluded_path) {
                Ok(x) => inner.excluded = x,
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
        // Recording first: a crash in between leaves the old song file, which upgrades again.
        if !upgraded.is_empty() {
            log::info!("library: moved {} songs' recordings to recordings/", upgraded.len());
        }
        for (t, r) in &upgraded {
            if let Some(r) = r {
                self.save_recording(r)?;
            }
            self.save_track(t)?;
        }
        // Recordings nothing plays (e.g. "hidden" ones from before) leave the
        // library; not when a file failed to load (its songs may play them).
        let unused: Vec<Id> = if !self.load_errors().is_empty() {
            vec![]
        } else {
            let inner = self.inner.read();
            let users = inner.recording_users();
            inner.recordings.keys().filter(|id| !users.contains_key(*id)).cloned().collect()
        };
        if !unused.is_empty() {
            log::info!("library: {} recording(s) no song plays leave the library", unused.len());
            self.drop_unused_recordings(&unused)?;
        }
        Ok(())
    }

    fn save_excluded(&self) -> Result<()> {
        let x = self.inner.read().excluded.clone();
        write_json(&self.library_dir().join("excluded.json"), &x)
    }

    /// Whether a file (by content hash) was removed from the library.
    pub fn is_excluded(&self, hash: &str) -> bool {
        self.inner.read().excluded.files.iter().any(|f| f.hash == hash)
    }

    /// Lets scanning add a removed file again.
    pub fn include_file(&self, hash: &str) -> Result<()> {
        let changed = {
            let mut inner = self.inner.write();
            let before = inner.excluded.files.len();
            inner.excluded.files.retain(|f| f.hash != hash);
            inner.excluded.files.len() != before
        };
        if changed {
            self.save_excluded()?;
        }
        Ok(())
    }

    /// Recordings nothing plays any more leave the library: Spotify-only ones
    /// simply go (they can be found on Spotify again); files are excluded
    /// from scanning (they stay on disk; Library → Files adds them again).
    fn drop_unused_recordings(&self, ids: &[Id]) -> Result<()> {
        let users = self.inner.read().recording_users();
        let mut excluded_changed = false;
        for id in ids {
            let rid = self.inner.read().canonical_rec(id);
            if users.contains_key(&rid) {
                continue;
            }
            let Some(r) = self.inner.write().recordings.remove(&rid) else { continue };
            {
                let mut inner = self.inner.write();
                for s in &r.sources {
                    inner.source_owner.remove(s);
                    if s.kind == "file" && !inner.excluded.files.iter().any(|f| f.hash == s.key) {
                        inner.excluded.files.push(ExcludedFile { hash: s.key.clone(), title: r.title.clone(), artist: r.artist.clone(), removed_at: now_ms() });
                        excluded_changed = true;
                    }
                }
            }
            let _ = fs::remove_file(self.lib_path("recordings", &r.id));
            let _ = fs::remove_file(self.waveform_path(&r.id));
        }
        if excluded_changed {
            self.save_excluded()?;
        }
        Ok(())
    }

    /// The audio files in the library folders (and files added from elsewhere), with what they are.
    pub fn library_files(&self) -> Vec<LibraryFile> {
        let inner = self.inner.read();
        let users = inner.recording_users();
        let mut out: Vec<LibraryFile> = inner
            .index
            .files
            .iter()
            .map(|(path, f)| {
                let rec = inner.source_owner.get(&SourceRef { kind: "file".into(), key: f.hash.clone() }).cloned();
                LibraryFile {
                    path: path.clone(),
                    root: f.root.clone(),
                    hash: f.hash.clone(),
                    songs: rec.as_ref().and_then(|r| users.get(r).cloned()).unwrap_or_default(),
                    recording: rec,
                    removed: inner.excluded.files.iter().any(|x| x.hash == f.hash),
                }
            })
            .collect();
        out.sort_by(|a, b| a.path.cmp(&b.path));
        out
    }

    fn lib_path(&self, kind: &str, id: &str) -> PathBuf {
        self.lib.read().join(kind).join(format!("{id}.json"))
    }

    fn save_track(&self, t: &TrackRecord) -> Result<()> {
        write_json(&self.lib_path("tracks", &t.id), t)
    }

    fn save_recording(&self, r: &RecordingRecord) -> Result<()> {
        write_json(&self.lib_path("recordings", &r.id), r)
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

    fn waveform_path(&self, rec_id: &str) -> PathBuf {
        self.local.join("waveforms").join(format!("{rec_id}.bin"))
    }

    /// Mutates a song and writes its file.
    fn with_track<R>(&self, id: &str, f: impl FnOnce(&mut TrackRecord) -> Result<R>) -> Result<R> {
        let mut inner = self.inner.write();
        let id = inner.canonical(id);
        let t = inner.tracks.get_mut(&id).ok_or_else(|| anyhow!("song {id} not found"))?;
        let r = f(t)?;
        let t = t.clone();
        inner.reindex_track(&t);
        drop(inner);
        self.save_track(&t)?;
        Ok(r)
    }

    /// Mutates a recording and writes its file.
    fn with_recording<R>(&self, id: &str, f: impl FnOnce(&mut RecordingRecord) -> Result<R>) -> Result<R> {
        let mut inner = self.inner.write();
        let id = inner.canonical_rec(id);
        let r = inner.recordings.get_mut(&id).ok_or_else(|| anyhow!("recording {id} not found"))?;
        let out = f(r)?;
        let r = r.clone();
        inner.reindex_recording(&r);
        drop(inner);
        self.save_recording(&r)?;
        Ok(out)
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

    // ---------------------------------------------------------------- songs

    pub fn all_tracks(&self) -> Vec<Track> {
        let inner = self.inner.read();
        let stats = inner.play_stats();
        let users = inner.recording_users();
        let mut v: Vec<Track> = inner.tracks.values().map(|t| inner.to_track(t, &stats, &users)).collect();
        v.sort_by(|a, b| (&a.rec.artist, &a.rec.title).cmp(&(&b.rec.artist, &b.rec.title)));
        v
    }

    pub fn track(&self, id: &str) -> Result<Track> {
        let inner = self.inner.read();
        let id = inner.canonical(id);
        let stats = inner.play_stats();
        let users = inner.recording_users();
        inner.tracks.get(&id).map(|t| inner.to_track(t, &stats, &users)).ok_or_else(|| anyhow!("song {id} not found"))
    }

    /// A new song with one version playing the recording, named after it.
    pub fn song_from_recording(&self, rec_id: &str, in_library: bool) -> Result<Id> {
        let r = self.recording(rec_id)?.rec;
        let song = TrackRecord {
            id: new_id(),
            title: r.title.clone(),
            artist: r.artist.clone(),
            album: r.album.clone(),
            genre: r.genre.clone(),
            added_at: now_ms(),
            in_library,
            versions: vec![original_version(&r.id)],
            ..Default::default()
        };
        self.save_track(&song)?;
        let mut inner = self.inner.write();
        inner.reindex_track(&song);
        let id = song.id.clone();
        inner.tracks.insert(id.clone(), song);
        Ok(id)
    }

    /// A song for new audio: the source's recording (created if new) and,
    /// unless a song already plays it, a song for it.
    pub fn insert_track(&self, t: &NewTrack, source: SourceRef) -> Result<Id> {
        let (rid, _) = self.insert_recording(t, source)?;
        match self.song_for_recording(&rid) {
            Some(id) => Ok(id),
            None => self.song_from_recording(&rid, true),
        }
    }

    /// The songs playing a recording.
    pub fn songs_for_recording(&self, rec_id: &str) -> Vec<Id> {
        let inner = self.inner.read();
        let rid = inner.canonical_rec(rec_id);
        inner.recording_users().remove(&rid).unwrap_or_default()
    }

    /// A song playing the recording (one in the library first).
    pub fn song_for_recording(&self, rec_id: &str) -> Option<Id> {
        let inner = self.inner.read();
        let rid = inner.canonical_rec(rec_id);
        let mut users: Vec<&TrackRecord> = inner.tracks.values().filter(|t| t.versions.iter().any(|v| inner.canonical_rec(&v.recording) == rid)).collect();
        users.sort_by_key(|t| (!t.in_library, t.added_at));
        users.first().map(|t| t.id.clone())
    }

    pub fn update_track(&self, id: &str, p: &TrackPatch) -> Result<()> {
        self.with_track(id, |t| {
            if let Some(v) = &p.title { t.title = v.clone(); }
            if let Some(v) = &p.artist { t.artist = v.clone(); }
            if let Some(v) = &p.album { t.album = v.clone(); }
            if let Some(v) = &p.genre { t.genre = v.clone(); }
            if let Some(v) = &p.rating { t.rating = *v; }
            if let Some(v) = &p.notes { t.notes = v.clone(); }
            if let Some(v) = &p.primary {
                if let Some(id) = v {
                    if !t.versions.iter().any(|x| &x.id == id) {
                        return Err(anyhow!("that version isn't one of this song's"));
                    }
                }
                // The first version is the primary anyway: no need to name it.
                t.primary = v.clone().filter(|id| t.versions.first().map(|f| &f.id) != Some(id));
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

    /// Moves the versions of `others` into `keep` and deletes them. Setlists
    /// and crates keep working because version ids don't change. (The UI asks
    /// which title etc. to keep first; `separate_versions` undoes it.)
    pub fn combine_songs(&self, keep: &str, others: &[Id]) -> Result<()> {
        let keep = self.inner.read().canonical(keep);
        let moved: Vec<TrackRecord> = {
            let inner = self.inner.read();
            others.iter().map(|o| inner.canonical(o)).filter(|o| *o != keep).filter_map(|o| inner.tracks.get(&o).cloned()).collect()
        };
        self.with_track(&keep, |k| {
            for o in &moved {
                for v in &o.versions {
                    let mut v = v.clone();
                    // Two "Original"s: name it after the song it was.
                    if k.versions.iter().any(|x| x.name == v.name) {
                        v.name = if o.title.trim().is_empty() || k.versions.iter().any(|x| x.name == o.title) { format!("{} ({})", v.name, k.versions.len() + 1) } else { o.title.clone() };
                    }
                    k.versions.push(v);
                }
                k.aliases.push(o.id.clone());
                k.aliases.extend(o.aliases.iter().cloned());
                if k.notes.is_empty() {
                    k.notes = o.notes.clone();
                }
                k.in_library |= o.in_library;
            }
            Ok(())
        })?;
        {
            let mut inner = self.inner.write();
            for o in &moved {
                inner.tracks.remove(&o.id);
                let _ = fs::remove_file(self.lib_path("tracks", &o.id));
            }
        }
        self.merge_identical_versions(&keep)
    }

    /// Merges a song's versions that differ only in name (same recording,
    /// styles, tags, settings…), as combining can leave them: the first is
    /// kept, and setlists and crates point to it instead.
    fn merge_identical_versions(&self, song: &str) -> Result<()> {
        let same = |a: &Version, b: &Version| {
            let sorted = |v: &[String]| {
                let mut v = v.to_vec();
                v.sort();
                v
            };
            a.recording == b.recording
                && a.display_name == b.display_name
                && a.settings == b.settings
                && a.comment == b.comment
                && sorted(&a.styles) == sorted(&b.styles)
                && sorted(&a.tags) == sorted(&b.tags)
        };
        // Removed version id → the version it duplicates.
        let mut into: HashMap<Id, Id> = HashMap::new();
        self.with_track(song, |t| {
            let mut kept: Vec<Version> = vec![];
            for v in t.versions.drain(..) {
                match kept.iter().find(|k| same(k, &v)) {
                    Some(k) => {
                        into.insert(v.id.clone(), k.id.clone());
                    }
                    None => kept.push(v),
                }
            }
            t.versions = kept;
            if let Some(k) = t.primary.as_ref().and_then(|p| into.get(p)) {
                t.primary = Some(k.clone());
            }
            Ok(())
        })?;
        if into.is_empty() {
            return Ok(());
        }
        let (setlists, crates) = {
            let mut inner = self.inner.write();
            for v in into.keys() {
                inner.version_owner.remove(v);
            }
            let mut setlists = vec![];
            for s in inner.setlists.values_mut() {
                let mut hit = false;
                for e in s.entries.iter_mut() {
                    if let Some(k) = e.version_id.as_ref().and_then(|v| into.get(v)) {
                        e.version_id = Some(k.clone());
                        hit = true;
                    }
                }
                if hit {
                    setlists.push(s.clone());
                }
            }
            let mut crates = vec![];
            for c in inner.crates.values_mut() {
                if c.version_ids.iter().any(|v| into.contains_key(v)) {
                    let ids: Vec<Id> = c.version_ids.iter().map(|v| into.get(v).unwrap_or(v).clone()).collect();
                    c.version_ids = dedup(&ids);
                    crates.push(c.clone());
                }
            }
            (setlists, crates)
        };
        for s in &setlists {
            self.save_setlist(s)?;
        }
        for c in &crates {
            self.save_crate(c)?;
        }
        Ok(())
    }

    /// Moves versions (of one song) into a new song of their own, named after
    /// the first one's recording.
    pub fn separate_versions(&self, version_ids: &[Id]) -> Result<Id> {
        let first = version_ids.first().ok_or_else(|| anyhow!("no versions given"))?;
        let (v, tid) = self.version(first)?;
        let song = self.track(&tid)?;
        if version_ids.iter().any(|id| !song.rec.versions.iter().any(|v| &v.id == id)) {
            return Err(anyhow!("the versions must belong to one song"));
        }
        if song.rec.versions.iter().all(|v| version_ids.contains(&v.id)) {
            return Err(anyhow!("those are all of the song's versions"));
        }
        let r = self.recording(&v.recording)?.rec;
        let moved: Vec<Version> = song.rec.versions.iter().filter(|v| version_ids.contains(&v.id)).cloned().collect();
        self.with_track(&tid, |t| {
            t.versions.retain(|v| !version_ids.contains(&v.id));
            if t.primary.as_ref().is_some_and(|p| version_ids.contains(p)) {
                t.primary = None;
            }
            Ok(())
        })?;
        let primary = song.rec.primary.clone().filter(|p| version_ids.contains(p));
        let new = TrackRecord {
            primary,
            id: new_id(),
            title: r.title,
            artist: r.artist,
            album: r.album,
            genre: r.genre,
            added_at: now_ms(),
            in_library: song.rec.in_library,
            versions: moved,
            ..Default::default()
        };
        self.save_track(&new)?;
        let mut inner = self.inner.write();
        inner.reindex_track(&new);
        let id = new.id.clone();
        inner.tracks.insert(id.clone(), new);
        Ok(id)
    }

    /// Deletes set-only songs that no setlist or crate references any more.
    /// `keep` protects songs that are loaded on a deck.
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

    /// Deletes songs (their versions leave setlists and crates). Their
    /// recordings stay, hidden, unless they're Spotify-only and nothing else
    /// plays them (they can be added from Spotify again).
    pub fn delete_tracks(&self, ids: &[Id]) -> Result<()> {
        let mut changed_setlists = vec![];
        let mut changed_crates = vec![];
        let mut recs: Vec<Id> = vec![];
        {
            let mut inner = self.inner.write();
            for id in ids {
                let id = inner.canonical(id);
                let Some(t) = inner.tracks.remove(&id) else { continue };
                let vids: Vec<Id> = t.versions.iter().map(|v| v.id.clone()).collect();
                recs.extend(t.versions.iter().map(|v| inner.canonical_rec(&v.recording)));
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
                let _ = fs::remove_file(self.lib_path("tracks", &id));
            }
        }
        for s in changed_setlists {
            self.save_setlist(&s)?;
        }
        for c in changed_crates {
            self.save_crate(&c)?;
        }
        self.drop_unused_recordings(&recs)
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

    // ---------------------------------------------------------------- recordings

    /// Every recording (hidden ones too), with the songs that play it.
    pub fn all_recordings(&self) -> Vec<Recording> {
        let inner = self.inner.read();
        let users = inner.recording_users();
        let mut v: Vec<Recording> = inner.recordings.values().map(|r| inner.to_recording(r, &users)).collect();
        v.sort_by(|a, b| (&a.rec.artist, &a.rec.title, &a.rec.id).cmp(&(&b.rec.artist, &b.rec.title, &b.rec.id)));
        v
    }

    pub fn recording(&self, id: &str) -> Result<Recording> {
        let inner = self.inner.read();
        let id = inner.canonical_rec(id);
        let users = inner.recording_users();
        inner.recordings.get(&id).map(|r| inner.to_recording(r, &users)).ok_or_else(|| anyhow!("recording {id} not found"))
    }

    /// A version and the recording it plays.
    pub fn version_recording(&self, version_id: &str) -> Result<(Version, Recording)> {
        let (v, _) = self.version(version_id)?;
        let r = self.recording(&v.recording)?;
        Ok((v, r))
    }

    /// The source's recording, created if it's new (`true`). No song is made.
    pub fn insert_recording(&self, t: &NewTrack, source: SourceRef) -> Result<(Id, bool)> {
        if let Some(existing) = self.recording_for_source(&source) {
            return Ok((existing, false));
        }
        let rec = RecordingRecord {
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
            sources: vec![source],
            ..Default::default()
        };
        self.save_recording(&rec)?;
        let mut inner = self.inner.write();
        inner.reindex_recording(&rec);
        let id = rec.id.clone();
        inner.recordings.insert(id.clone(), rec);
        Ok((id, true))
    }

    pub fn recording_for_source(&self, s: &SourceRef) -> Option<Id> {
        self.inner.read().source_owner.get(s).cloned()
    }

    pub fn add_source(&self, rec_id: &str, s: SourceRef) -> Result<()> {
        if let Some(owner) = self.recording_for_source(&s) {
            if owner != self.inner.read().canonical_rec(rec_id) {
                return Err(anyhow!("that source already belongs to another recording (mark the two as the same recording instead)"));
            }
            return Ok(());
        }
        self.with_recording(rec_id, |r| {
            r.sources.push(s);
            Ok(())
        })
    }

    pub fn remove_source(&self, rec_id: &str, s: &SourceRef) -> Result<()> {
        self.with_recording(rec_id, |r| {
            if r.sources.len() <= 1 {
                return Err(anyhow!("a recording needs at least one source"));
            }
            r.sources.retain(|x| x != s);
            Ok(())
        })?;
        self.inner.write().source_owner.remove(s);
        Ok(())
    }

    pub fn update_recording(&self, id: &str, p: &RecordingPatch) -> Result<()> {
        self.with_recording(id, |t| {
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
            if let Some(v) = &p.gain_db { t.gain_db = *v; }
            if let Some(v) = &p.preferred_source { t.preferred_source = v.clone(); }
            Ok(())
        })
    }

    /// Marks recordings as the same audio (e.g. a file and its Spotify
    /// track): their sources join `keep` and versions playing them play
    /// `keep`. (The UI asks which BPM etc. to keep first; `split_source`
    /// undoes it.)
    pub fn merge_recordings(&self, keep: &str, others: &[Id]) -> Result<()> {
        let keep = self.inner.read().canonical_rec(keep);
        let moved: Vec<RecordingRecord> = {
            let inner = self.inner.read();
            others.iter().map(|o| inner.canonical_rec(o)).filter(|o| *o != keep).filter_map(|o| inner.recordings.get(&o).cloned()).collect()
        };
        self.with_recording(&keep, |k| {
            for o in &moved {
                for s in &o.sources {
                    if !k.sources.contains(s) {
                        k.sources.push(s.clone());
                    }
                }
                k.aliases.push(o.id.clone());
                k.aliases.extend(o.aliases.iter().cloned());
                if k.hot_cues.is_empty() {
                    k.hot_cues = o.hot_cues.clone();
                }
            }
            Ok(())
        })?;
        // Versions follow (their files name the recording directly).
        let ids: Vec<&Id> = moved.iter().map(|o| &o.id).collect();
        let songs: Vec<Id> = {
            let inner = self.inner.read();
            inner.tracks.values().filter(|t| t.versions.iter().any(|v| ids.contains(&&v.recording))).map(|t| t.id.clone()).collect()
        };
        for sid in &songs {
            self.with_track(sid, |t| {
                for v in t.versions.iter_mut().filter(|v| ids.contains(&&v.recording)) {
                    v.recording = keep.clone();
                }
                Ok(())
            })?;
        }
        {
            let mut inner = self.inner.write();
            for o in &moved {
                inner.recordings.remove(&o.id);
                let _ = fs::remove_file(self.lib_path("recordings", &o.id));
                let _ = fs::remove_file(self.waveform_path(&o.id));
            }
        }
        // A song's versions of the two files may now be the same version.
        for sid in &songs {
            self.merge_identical_versions(sid)?;
        }
        Ok(())
    }

    /// Takes one source out of a recording into a new recording of its own
    /// (analysed afresh); `versions` switch to it. Undoes `merge_recordings`,
    /// including merges made before recordings were separate.
    pub fn split_source(&self, rec_id: &str, source: &SourceRef, versions: &[Id], new_song: bool) -> Result<Id> {
        if versions.is_empty() && !new_song {
            return Err(anyhow!("something must play the split-off audio: a version, or a song of its own"));
        }
        let old = self.recording(rec_id)?.rec;
        if !old.sources.contains(source) {
            return Err(anyhow!("that source isn't part of this recording"));
        }
        if old.sources.len() <= 1 {
            return Err(anyhow!("a recording needs at least one source"));
        }
        self.remove_source(&old.id, source)?;
        let new = RecordingRecord {
            id: new_id(),
            title: old.title.clone(),
            artist: old.artist.clone(),
            album: old.album.clone(),
            genre: old.genre.clone(),
            duration_ms: old.duration_ms,
            artwork_url: old.artwork_url.clone(),
            added_at: now_ms(),
            sources: vec![source.clone()],
            ..Default::default()
        };
        self.save_recording(&new)?;
        {
            let mut inner = self.inner.write();
            inner.reindex_recording(&new);
            inner.recordings.insert(new.id.clone(), new.clone());
        }
        for vid in versions {
            self.version_update(vid, &VersionPatch { recording: Some(new.id.clone()), ..Default::default() })?;
        }
        if new_song {
            self.song_from_recording(&new.id, true)?;
        }
        Ok(new.id)
    }

    /// Removes recordings from the library with the versions playing them
    /// (songs left without versions go too; set entries lose their song).
    /// Files stay on disk, excluded from scanning.
    pub fn remove_recordings(&self, ids: &[Id]) -> Result<()> {
        let rids: Vec<Id> = ids.iter().map(|id| self.inner.read().canonical_rec(id)).collect();
        let (whole, partial): (Vec<Id>, Vec<(Id, Vec<Id>)>) = {
            let inner = self.inner.read();
            let mut whole = vec![];
            let mut partial = vec![];
            for t in inner.tracks.values() {
                let hit: Vec<Id> = t.versions.iter().filter(|v| rids.contains(&inner.canonical_rec(&v.recording))).map(|v| v.id.clone()).collect();
                if hit.is_empty() {
                    continue;
                }
                if hit.len() == t.versions.len() {
                    whole.push(t.id.clone());
                } else {
                    partial.push((t.id.clone(), hit));
                }
            }
            (whole, partial)
        };
        for (song, vids) in &partial {
            self.with_track(song, |t| {
                t.versions.retain(|v| !vids.contains(&v.id));
                if t.primary.as_ref().is_some_and(|p| vids.contains(p)) {
                    t.primary = None;
                }
                Ok(())
            })?;
            self.forget_versions(vids)?;
        }
        self.delete_tracks(&whole)?;
        self.drop_unused_recordings(&rids)
    }

    /// Set entries and crates stop referring to removed versions.
    fn forget_versions(&self, vids: &[Id]) -> Result<()> {
        let (setlists, crates) = {
            let mut inner = self.inner.write();
            for v in vids {
                inner.version_owner.remove(v);
            }
            let mut setlists = vec![];
            for s in inner.setlists.values_mut() {
                let mut hit = false;
                for e in s.entries.iter_mut() {
                    if e.version_id.as_ref().is_some_and(|v| vids.contains(v)) {
                        e.version_id = None;
                        hit = true;
                    }
                }
                if hit {
                    setlists.push(s.clone());
                }
            }
            let mut crates = vec![];
            for c in inner.crates.values_mut() {
                let before = c.version_ids.len();
                c.version_ids.retain(|v| !vids.contains(v));
                if c.version_ids.len() != before {
                    crates.push(c.clone());
                }
            }
            (setlists, crates)
        };
        for s in &setlists {
            self.save_setlist(s)?;
        }
        for c in &crates {
            self.save_crate(c)?;
        }
        Ok(())
    }

    /// A BPM the DJ entered (or from tags), which analysis should use for the meter.
    pub fn known_bpm(&self, rec_id: &str) -> Option<f64> {
        self.recording(rec_id).ok().filter(|r| r.rec.bpm_source.as_deref() != Some("detected")).and_then(|r| r.rec.bpm)
    }

    pub fn set_analysis(&self, rec_id: &str, a: &Analysis) -> Result<()> {
        let id = self.inner.read().canonical_rec(rec_id);
        if !a.waveform.is_empty() {
            write_atomic(&self.waveform_path(&id), &a.waveform)?;
        }
        self.with_recording(&id, |t| {
            t.analyzed = true;
            t.analysis_version = crate::analysis::VERSION;
            t.loudness_db = a.loudness_db.map(|l| (l * 10.0).round() / 10.0);
            t.audio_start_ms = a.audio_start_ms;
            t.audio_end_ms = a.audio_end_ms;
            if a.fingerprint.is_some() {
                t.fingerprint = a.fingerprint.clone();
            }
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

    /// Sets (`Some`) or clears (`None`) hot cue `slot` on a recording.
    pub fn set_hot_cue(&self, rec_id: &str, slot: u8, ms: Option<i64>) -> Result<()> {
        self.with_recording(rec_id, |t| {
            t.hot_cues.retain(|c| c.slot != slot);
            if let Some(ms) = ms {
                t.hot_cues.push(HotCue { slot, ms: ms.max(0) });
                t.hot_cues.sort_by_key(|c| c.slot);
            }
            Ok(())
        })
    }

    /// Reverts the BPM to the analysed value (`detected`) or the file's tag
    /// (`tag`, passing the tag value if the record doesn't have it yet).
    /// Returns `true` if the recording must be re-analysed first.
    pub fn reset_bpm(&self, rec_id: &str, from: &str, tag_bpm: Option<f64>) -> Result<bool> {
        self.with_recording(rec_id, |t| match from {
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

    pub fn waveform(&self, rec_id: &str) -> Option<Vec<u8>> {
        let id = self.inner.read().canonical_rec(rec_id);
        fs::read(self.waveform_path(&id)).ok()
    }

    /// Recordings with a file on this machine but no analysis (or no cached waveform).
    pub fn needs_analysis(&self) -> Vec<Id> {
        let inner = self.inner.read();
        inner
            .recordings
            .values()
            .filter(|t| {
                let has_file = t.sources.iter().any(|s| s.kind == "file" && inner.by_hash.contains_key(&s.key));
                has_file && (!t.analyzed || t.analysis_version < crate::analysis::VERSION || !self.waveform_path(&t.id).exists())
            })
            .map(|t| t.id.clone())
            .collect()
    }

    // ---------------------------------------------------------------- versions

    /// A version and the id of the song that owns it.
    pub fn version(&self, id: &str) -> Result<(Version, Id)> {
        let inner = self.inner.read();
        let tid = inner.version_owner.get(id).ok_or_else(|| anyhow!("version {id} not found"))?;
        let t = inner.tracks.get(tid).ok_or_else(|| anyhow!("song {tid} not found"))?;
        let mut v = t.versions.iter().find(|v| v.id == id).ok_or_else(|| anyhow!("version {id} not found"))?.clone();
        v.recording = inner.canonical_rec(&v.recording);
        Ok((v, tid.clone()))
    }

    /// Adds a version to a song (playing `v.recording`, or the song's first version's recording).
    pub fn version_create(&self, track_id: &str, mut v: Version) -> Result<Version> {
        v.id = new_id();
        if v.recording.is_empty() {
            v.recording = self.track(track_id)?.rec.versions.first().map(|f| f.recording.clone()).ok_or_else(|| anyhow!("that song has no recording"))?;
        } else {
            self.recording(&v.recording)?;
        }
        let out = v.clone();
        self.with_track(track_id, |t| {
            t.versions.push(v);
            Ok(())
        })?;
        Ok(out)
    }

    pub fn version_update(&self, id: &str, p: &VersionPatch) -> Result<Version> {
        let (_, tid) = self.version(id)?;
        if let Some(r) = &p.recording {
            self.recording(r)?;
        }
        self.with_track(&tid, |t| {
            let v = t.versions.iter_mut().find(|v| v.id == id).ok_or_else(|| anyhow!("version not found"))?;
            if let Some(x) = &p.name { v.name = x.clone(); }
            if let Some(x) = &p.recording { v.recording = x.clone(); }
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
        let (version, tid) = self.version(id)?;
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
                return Err(anyhow!("a song needs at least one version"));
            }
            t.versions.retain(|v| v.id != id);
            if t.primary.as_deref() == Some(id) {
                t.primary = None;
            }
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
        // Its audio, if nothing else plays it, leaves the library.
        self.drop_unused_recordings(&[version.recording])
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

    pub fn recording_file_paths(&self, rec_id: &str) -> Vec<String> {
        self.recording(rec_id).map(|r| r.files).unwrap_or_default()
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

    pub fn setlist_create(&self, name: &str, mode: &str, club: Option<ClubRef>, date: Option<String>) -> Result<Id> {
        let now = now_ms();
        let s = Setlist {
            id: new_id(),
            name: name.into(),
            event_date: date,
            club,
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
            if let Some(v) = &p.club { s.club = v.clone(); }
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
                    club_styles: n.club_styles.as_ref().map(|s| dedup(s)),
                    label: n.label.clone().filter(|l| !l.trim().is_empty()),
                    overrides: n.overrides.clone(),
                    note: n.note.clone(),
                    tags: n.tags.clone(),
                    kind: n.kind,
                    pause_before: n.pause_before,
                    announcement: n.announcement.clone(),
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
            if let Some(v) = &p.club_styles { e.club_styles = v.as_ref().map(|s| dedup(s)); }
            if let Some(v) = &p.label { e.label = v.clone().filter(|l| !l.trim().is_empty()); }
            if let Some(v) = &p.overrides { e.overrides = v.clone(); }
            if let Some(v) = &p.note { e.note = v.clone(); }
            if let Some(v) = &p.tags { e.tags = v.clone(); }
            if let Some(v) = p.hold { e.hold = v; }
            if let Some(v) = p.pause_before { e.pause_before = v.map(|x| x.max(0.0)); }
            if let Some(v) = p.pause_after { e.pause_after = v.map(|x| x.max(0.0)); }
            if let Some(v) = &p.announcement { e.announcement = v.clone(); }
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

    fn file(k: &str) -> SourceRef {
        SourceRef { kind: "file".into(), key: k.into() }
    }

    fn spotify(k: &str) -> SourceRef {
        SourceRef { kind: "spotify".into(), key: format!("spotify:track:{k}") }
    }

    fn song(store: &Store, title: &str, src: SourceRef) -> Id {
        store.insert_track(&NewTrack { title: title.into(), ..Default::default() }, src).unwrap()
    }

    /// The first version of a song, and the recording it plays.
    fn first(store: &Store, song: &str) -> (Id, Id) {
        let v = store.track(song).unwrap().rec.versions[0].clone();
        (v.id, v.recording)
    }

    #[test]
    fn bpm_can_be_reset_after_editing() {
        let (lib, local) = temp_dirs();
        let store = Store::open(&lib, &local).unwrap();
        let nt = NewTrack { title: "W".into(), bpm: Some(120.0), bpm_source: Some("tag".into()), ..Default::default() };
        let id = store.insert_track(&nt, file("h")).unwrap();
        let (_, rid) = first(&store, &id);
        store.set_analysis(&rid, &Analysis { bpm: Some(93.1), ..Default::default() }).unwrap();
        // Tag BPM wins over analysis, but the analysed value is remembered.
        assert_eq!(store.recording(&rid).unwrap().rec.bpm, Some(120.0));
        store.update_recording(&rid, &RecordingPatch { bpm: Some(Some(139.6)), ..Default::default() }).unwrap();
        assert_eq!(store.recording(&rid).unwrap().rec.bpm_source.as_deref(), Some("user"));

        assert!(!store.reset_bpm(&rid, "detected", None).unwrap());
        let t = store.recording(&rid).unwrap().rec;
        assert_eq!((t.bpm, t.bpm_source.as_deref()), (Some(93.1), Some("detected")));
        assert!(!store.reset_bpm(&rid, "tag", None).unwrap());
        assert_eq!(store.recording(&rid).unwrap().rec.bpm, Some(120.0));

        // Survives a reload from disk.
        let store2 = Store::open(&lib, &local).unwrap();
        assert_eq!(store2.recording(&rid).unwrap().rec.detected_bpm, Some(93.1));
        let _ = fs::remove_dir_all(lib.parent().unwrap());
    }

    #[test]
    fn set_only_tracks_are_pruned_when_unused() {
        let (lib, local) = temp_dirs();
        let store = Store::open(&lib, &local).unwrap();
        let id = song(&store, "S", spotify("abc"));
        let (vid, rid) = first(&store, &id);
        store.set_in_library(&id, false).unwrap();
        let sid = store.setlist_create("Set", "live", None, None).unwrap();
        let eids = store.entries_insert(&sid, None, &[NewEntry { version_id: Some(vid), ..Default::default() }]).unwrap();
        assert_eq!(store.prune_unused(&[]).unwrap(), 0, "still used by the set");
        // Reloading keeps the flag (it's only written when false).
        let store = Store::open(&lib, &local).unwrap();
        assert!(!store.track(&id).unwrap().rec.in_library);
        store.entries_delete(&sid, &eids).unwrap();
        assert_eq!(store.prune_unused(&[id.clone()]).unwrap(), 0, "protected while on a deck");
        assert_eq!(store.prune_unused(&[]).unwrap(), 1);
        assert!(store.track(&id).is_err());
        // Its Spotify-only recording goes with it.
        assert!(store.recording(&rid).is_err());
        let _ = fs::remove_dir_all(lib.parent().unwrap());
    }

    #[test]
    fn styles_are_labels_and_crates_mix_styles() {
        let (lib, local) = temp_dirs();
        let store = Store::open(&lib, &local).unwrap();
        let a = song(&store, "A", file("a"));
        let b = song(&store, "B", file("b"));
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
        let tid = song(&store, "S", file("h"));
        let vid = store.track(&tid).unwrap().rec.versions[0].id.clone();
        let mut sets = vec![];
        for _ in 0..2 {
            let sid = store.setlist_create("Set", "live", None, None).unwrap();
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
        let tid = song(&store, "Song", file("h1"));
        let (_, rid) = first(&store, &tid);
        assert!(store.track(&tid).unwrap().missing);
        store.upsert_file("/music/a.flac", FileEntry { hash: "h1".into(), size: 1, mtime: 2, root: None });
        assert_eq!(store.recording(&rid).unwrap().files, vec!["/music/a.flac".to_string()]);
        assert!(!store.track(&tid).unwrap().missing);
        store.save_index().unwrap();

        let v = store
            .version_create(
                &tid,
                Version {
                    id: String::new(),
                    name: "Rotary @150".into(),
                    recording: String::new(),
                    display_name: None,
                    styles: vec!["rotary-waltz".into()],
                    legacy_style: None,
                    settings: Overrides { target_bpm: Some(150.0), ..Default::default() },
                    comment: String::new(),
                    tags: vec![],
                },
            )
            .unwrap();
        assert_eq!(v.recording, rid, "a new version plays the song's recording");
        let sid = store.setlist_create("Social", "preset", None, None).unwrap();
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
        assert_eq!(t.recordings.len(), 1);
        assert_eq!(t.play_count, 1);
        assert_eq!(t.recordings[0].files.len(), 1);
        let (e, _) = store2.entry(&ids[0]).unwrap();
        assert_eq!(e.overrides.target_bpm, Some(148.0));
        assert_eq!(store2.version(&v.id).unwrap().1, tid);

        // A hand-edited, broken file is reported instead of failing the load.
        fs::write(lib.join("tracks").join("broken.json"), "{ not json").unwrap();
        let store3 = Store::open(&lib, &local).unwrap();
        assert_eq!(store3.load_errors().len(), 1);
        assert_eq!(store3.all_tracks().len(), 1);
        let _ = fs::remove_dir_all(lib.parent().unwrap());
    }

    #[test]
    fn older_songs_move_their_recording_out() {
        let (lib, local) = temp_dirs();
        fs::create_dir_all(lib.join("tracks")).unwrap();
        // A song from before recordings were separate, merged from two (a file and its Spotify track).
        let old = r#"{"id":"s1","title":"Song","artist":"A","album":"","durationMs":180000,"bpm":121.5,"bpmSource":"user",
            "beatsPerBar":4,"meterSource":"detected","analyzed":true,"analysisVersion":4,"addedAt":5,"rating":3,
            "sources":[{"kind":"file","key":"h1"},{"kind":"spotify","key":"spotify:track:x"}],
            "hotCues":[{"slot":1,"ms":1000}],"aliases":["s2"],
            "versions":[{"id":"v1","name":"Original","styles":["polka"]},{"id":"v2","name":"Short","cueOutMs":60000}]}"#;
        fs::write(lib.join("tracks").join("s1.json"), old).unwrap();
        let store = Store::open(&lib, &local).unwrap();
        assert!(store.load_errors().is_empty(), "{:?}", store.load_errors());
        let t = store.track("s1").unwrap();
        assert_eq!(t.rec.rating, Some(3));
        assert!(t.rec.versions.iter().all(|v| v.recording == "s1"), "the recording keeps the song's id (waveforms stay valid)");
        let r = store.recording("s1").unwrap().rec;
        assert_eq!((r.bpm, r.bpm_source.as_deref(), r.beats_per_bar, r.analysis_version), (Some(121.5), Some("user"), Some(4), 4));
        assert_eq!(r.sources.len(), 2);
        assert_eq!(r.hot_cues.len(), 1);
        assert_eq!(r.title, "Song");
        assert_eq!(store.track("s2").unwrap().rec.id, "s1", "a merged song's old id still resolves");
        // Both files were rewritten in the new form; loading again changes nothing.
        let text = fs::read_to_string(lib.join("tracks").join("s1.json")).unwrap();
        assert!(!text.contains("sources") && !text.contains("bpm") && text.contains("\"recording\": \"s1\""), "{text}");
        assert!(lib.join("recordings").join("s1.json").exists());
        let again = Store::open(&lib, &local).unwrap();
        assert_eq!(again.recording("s1").unwrap().rec, r);

        // The old merge can be undone: the Spotify track becomes a recording
        // of its own, and the short version plays it.
        let new = again.split_source("s1", &spotify("x"), &["v2".to_string()], false).unwrap();
        assert_eq!(again.recording("s1").unwrap().rec.sources, vec![file("h1")]);
        assert_eq!(again.recording(&new).unwrap().rec.sources, vec![spotify("x")]);
        assert!(!again.recording(&new).unwrap().rec.analyzed, "analysed afresh");
        assert_eq!(again.version("v2").unwrap().0.recording, new);
        assert_eq!(again.recording_for_source(&spotify("x")), Some(new.clone()));
        // … and the short version can become a song of its own.
        let s = again.separate_versions(&["v2".to_string()]).unwrap();
        assert_eq!(again.track(&s).unwrap().rec.versions[0].id, "v2");
        assert_eq!(again.track("s1").unwrap().rec.versions.len(), 1);
        let _ = fs::remove_dir_all(lib.parent().unwrap());
    }

    #[test]
    fn songs_combine_and_separate() {
        let (lib, local) = temp_dirs();
        let store = Store::open(&lib, &local).unwrap();
        // Two people's edits for one line dance.
        let a = song(&store, "Barbie (RP edit)", file("a"));
        let b = song(&store, "Barbie (Lucas edit)", file("b"));
        let (vb, rb) = first(&store, &b);
        let sid = store.setlist_create("Set", "live", None, None).unwrap();
        store.entries_insert(&sid, None, &[NewEntry { version_id: Some(vb.clone()), ..Default::default() }]).unwrap();

        store.combine_songs(&a, &[b.clone()]).unwrap();
        let t = store.track(&a).unwrap();
        assert_eq!(t.rec.versions.len(), 2);
        assert_eq!(t.rec.versions[1].name, "Barbie (Lucas edit)", "named after the song it was, not a second “Original”");
        assert_eq!(t.recordings.len(), 2, "each edit keeps its own audio");
        assert_eq!(store.version(&vb).unwrap().1, a, "set entries follow");
        assert_eq!(store.track(&b).unwrap().rec.id, a, "the old id resolves");
        assert_eq!(store.all_tracks().len(), 1);
        assert!(store.separate_versions(&t.rec.versions.iter().map(|v| v.id.clone()).collect::<Vec<_>>()).is_err(), "one must stay");

        // A primary version: kept by the song, and by the version when separated.
        store.update_track(&a, &TrackPatch { primary: Some(Some(vb.clone())), ..Default::default() }).unwrap();
        assert_eq!(store.track(&a).unwrap().rec.primary, Some(vb.clone()));
        assert!(store.update_track(&a, &TrackPatch { primary: Some(Some("nope".into())), ..Default::default() }).is_err());
        // Undone: the edit is a song again, named after its recording.
        let back = store.separate_versions(&[vb.clone()]).unwrap();
        let t = store.track(&back).unwrap();
        assert_eq!(t.rec.title, "Barbie (Lucas edit)");
        assert_eq!(t.recordings[0].rec.id, rb);
        assert_eq!(store.version(&vb).unwrap().1, back);
        assert_eq!(store.track(&a).unwrap().rec.primary, None, "the primary left with it");
        assert_eq!(store.track(&back).unwrap().rec.primary, Some(vb.clone()));
        let _ = fs::remove_dir_all(lib.parent().unwrap());
    }

    #[test]
    fn combining_the_same_recording_merges_identical_versions() {
        let (lib, local) = temp_dirs();
        let store = Store::open(&lib, &local).unwrap();
        // The same audio as a FLAC and an MP3: two songs, two recordings.
        let a = song(&store, "Blue Danube", file("flac"));
        let b = song(&store, "Blue Danube", file("mp3"));
        let (va, ra) = first(&store, &a);
        let (vb, rb) = first(&store, &b);
        let teaching = store
            .version_create(&b, Version { id: String::new(), name: "Teaching".into(), recording: String::new(), display_name: None, styles: vec![], legacy_style: None, settings: Overrides { target_bpm: Some(100.0), ..Default::default() }, comment: String::new(), tags: vec![] })
            .unwrap();
        let sid = store.setlist_create("Set", "live", None, None).unwrap();
        let eid = store.entries_insert(&sid, None, &[NewEntry { version_id: Some(vb.clone()), ..Default::default() }]).unwrap()[0].clone();
        let cr = store.crate_create("C", None).unwrap();
        store.crate_add(&cr, &[va.clone(), vb.clone()]).unwrap();

        store.merge_recordings(&ra, &[rb]).unwrap();
        store.combine_songs(&a, &[b]).unwrap();
        // The MP3's "Original" is now the FLAC's in every way but its name: one version.
        let t = store.track(&a).unwrap();
        assert_eq!(t.rec.versions.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), ["Original", "Teaching"]);
        assert_eq!(t.rec.versions[1].id, teaching.id, "a version with its own settings stays");
        assert_eq!(store.entry(&eid).unwrap().0.version_id, Some(va.clone()), "the set follows");
        assert_eq!(store.crates()[0].version_ids, vec![va.clone()]);
        assert!(store.version(&vb).is_err());
        let reloaded = Store::open(&lib, &local).unwrap();
        assert_eq!(reloaded.track(&a).unwrap().rec.versions.len(), 2);
        assert_eq!(reloaded.entry(&eid).unwrap().0.version_id, Some(va));
        let _ = fs::remove_dir_all(lib.parent().unwrap());
    }

    #[test]
    fn recordings_merge_and_split() {
        let (lib, local) = temp_dirs();
        let store = Store::open(&lib, &local).unwrap();
        let f = song(&store, "Song", file("h"));
        let s = song(&store, "Song (Spotify)", spotify("x"));
        let (vf, rf) = first(&store, &f);
        let (vs, rs) = first(&store, &s);
        store.set_hot_cue(&rs, 2, Some(500)).unwrap();

        // The same audio: one recording; the songs stay as they are.
        store.merge_recordings(&rf, &[rs.clone()]).unwrap();
        assert_eq!(store.recording(&rf).unwrap().rec.sources, vec![file("h"), spotify("x")]);
        assert_eq!(store.recording(&rf).unwrap().rec.hot_cues.len(), 1, "cues come along when the kept one has none");
        assert_eq!(store.version(&vs).unwrap().0.recording, rf);
        assert_eq!(store.recording(&rs).unwrap().rec.id, rf, "the old id resolves");
        assert_eq!(store.recording(&rf).unwrap().songs.len(), 2);
        assert_eq!(store.all_tracks().len(), 2);
        let reloaded = Store::open(&lib, &local).unwrap();
        assert_eq!(reloaded.version(&vs).unwrap().0.recording, rf);

        // Undone.
        let back = reloaded.split_source(&rf, &spotify("x"), &[vs.clone()], false).unwrap();
        assert_eq!(reloaded.version(&vs).unwrap().0.recording, back);
        assert_eq!(reloaded.version(&vf).unwrap().0.recording, rf);
        assert!(reloaded.split_source(&rf, &file("h"), &[], true).is_err(), "a recording keeps one source");
        assert!(reloaded.split_source(&rf, &file("h"), &[], false).is_err(), "something must play it");
        let _ = fs::remove_dir_all(lib.parent().unwrap());
    }

    #[test]
    fn audio_nothing_plays_leaves_the_library() {
        let (lib, local) = temp_dirs();
        let store = Store::open(&lib, &local).unwrap();
        let a = song(&store, "Walk Like an Egyptian", file("h"));
        let (_, r) = first(&store, &a);
        store.upsert_file("/music/w.flac", FileEntry { hash: "h".into(), size: 1, mtime: 2, root: Some("/music".into()) });
        // A second song (another line dance) plays the same recording: it stays while one does.
        let b = store.song_from_recording(&r, true).unwrap();
        store.delete_tracks(&[a]).unwrap();
        assert_eq!(store.recording(&r).unwrap().songs, vec![b.clone()]);
        store.delete_tracks(&[b]).unwrap();
        // Nothing plays it: out of the library, the file excluded from scanning (not deleted).
        assert!(store.recording(&r).is_err());
        assert!(store.is_excluded("h"));
        let f = &store.library_files()[0];
        assert!(f.removed && f.recording.is_none() && f.path == "/music/w.flac");
        // Added again explicitly.
        store.include_file("h").unwrap();
        assert!(!Store::open(&lib, &local).unwrap().is_excluded("h"));
        // Spotify-only audio simply goes.
        let s = song(&store, "S", spotify("x"));
        store.delete_tracks(&[s]).unwrap();
        assert!(store.recording_for_source(&spotify("x")).is_none());
        assert!(!store.is_excluded("spotify:track:x"));
        let _ = fs::remove_dir_all(lib.parent().unwrap());
    }

    #[test]
    fn removing_audio_takes_its_versions() {
        let (lib, local) = temp_dirs();
        let store = Store::open(&lib, &local).unwrap();
        let a = song(&store, "Barbie (RP edit)", file("a"));
        let b = song(&store, "Barbie (Lucas edit)", file("b"));
        let (va, _) = first(&store, &a);
        let (vb, rb) = first(&store, &b);
        store.combine_songs(&a, &[b]).unwrap();
        let sid = store.setlist_create("Set", "live", None, None).unwrap();
        let eid = store.entries_insert(&sid, None, &[NewEntry { version_id: Some(vb.clone()), ..Default::default() }]).unwrap()[0].clone();
        store.remove_recordings(&[rb.clone()]).unwrap();
        assert_eq!(store.track(&a).unwrap().rec.versions.iter().map(|v| v.id.clone()).collect::<Vec<_>>(), vec![va]);
        assert_eq!(store.entry(&eid).unwrap().0.version_id, None, "the set entry loses its song");
        assert!(store.is_excluded("b"));
        // Older "hidden" recordings (no song) leave the library on load.
        let r = store.insert_recording(&NewTrack { title: "Old".into(), ..Default::default() }, file("c")).unwrap().0;
        // Not while a song file is unreadable (it may be the one playing it).
        fs::write(lib.join("tracks").join("broken.json"), "{ not json").unwrap();
        assert!(Store::open(&lib, &local).unwrap().recording(&r).is_ok());
        fs::remove_file(lib.join("tracks").join("broken.json")).unwrap();
        let reloaded = Store::open(&lib, &local).unwrap();
        assert!(reloaded.recording(&r).is_err());
        assert!(reloaded.is_excluded("c"));
        let _ = fs::remove_dir_all(lib.parent().unwrap());
    }
}

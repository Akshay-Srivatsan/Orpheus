//! The engine's API for user interfaces: one function per command, and `call`
//! to run one by name with JSON arguments (camelCase keys) and result.
//! Calls can block (files, the network, Spotify): run them on a worker
//! thread, never a UI or audio thread, and not inside an async runtime.

use crate::app::{App, PerformState, ProjectorPayload};
use crate::club::{self, Club, ClubInfo, ClubVersion, Imported};
use std::collections::HashMap;
use crate::audio::output::OutputStatus;
use crate::store::NewTrack;
use crate::midi::{LearnedMessage, Mapping, PortInfo};
use crate::model::*;
use crate::settings::Settings;
use crate::spotify::{SpotifyPlaylist, SpotifyStatus, SpotifyTrack};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

type Res<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    format!("{e:#}")
}

fn ae(e: anyhow::Error) -> String {
    format!("{e:#}")
}

/// Slow work: API calls already run on the caller's worker thread.
fn blocking<T>(f: impl FnOnce() -> anyhow::Result<T>) -> Res<T> {
    f().map_err(ae)
}

/// The same, for work that can't fail.
fn spawn_blocking<T>(f: impl FnOnce() -> T) -> Res<T> {
    Ok(f())
}

// ---------------------------------------------------------------- library

pub fn library_tracks(app: &Arc<App>) -> Res<Vec<Track>> {
    Ok(app.store.all_tracks())
}

pub fn track_get(app: &Arc<App>, id: Id) -> Res<Track> {
    app.store.track(&id).map_err(ae)
}

pub fn track_update(app: &Arc<App>, id: Id, patch: serde_json::Value) -> Res<Track> {
    let patch: TrackPatch = serde_json::from_value(patch).map_err(err)?;
    app.store.update_track(&id, &patch).map_err(ae)?;
    crate::app::emit(&app.handle, "library-changed", vec![id.clone()]);
    // The projector shows titles from songs.
    app.emit_perform();
    app.store.track(&id).map_err(ae)
}

/// Takes songs out of the library; sets that play them keep them (hidden).
pub fn tracks_delete(app: &Arc<App>, ids: Vec<Id>) -> Res<()> {
    let keep: Vec<Id> = app.decks.iter().filter_map(|d| d.track_id()).collect();
    app.store.remove_tracks(&ids, &keep).map_err(ae)?;
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    crate::app::emit(&app.handle, "setlists-changed", ());
    crate::app::emit(&app.handle, "crates-changed", ());
    Ok(())
}

/// Tells the UI the songs playing a recording changed.
fn emit_recording_changed(app: &&Arc<App>, rec_id: &str) {
    crate::app::emit(&app.handle, "library-changed", app.store.songs_for_recording(rec_id));
    crate::app::emit(&app.handle, "recordings-changed", ());
}

pub fn recording_update(app: &Arc<App>, id: Id, patch: serde_json::Value) -> Res<Recording> {
    let patch: RecordingPatch = serde_json::from_value(patch).map_err(err)?;
    app.store.update_recording(&id, &patch).map_err(ae)?;
    if patch.gain_db.is_some() {
        app.relevel_decks(true);
    }
    // The meter is detected on the beat grid of the BPM; follow a corrected BPM.
    if patch.bpm.is_some() && app.store.recording(&id).map_err(ae)?.rec.meter_source.as_deref() != Some("user") {
        app.queue_analysis(id.clone(), true);
    }
    emit_recording_changed(&app, &id);
    app.emit_perform();
    app.store.recording(&id).map_err(ae)
}

/// Resets a recording's BPM to the analysed value (`from = "detected"`) or the
/// value in the file's tags (`from = "tag"`).
pub fn recording_reset_bpm(app: &Arc<App>, id: Id, from: String) -> Res<Recording> {
    let mut tag = None;
    if from == "tag" && app.store.recording(&id).map_err(ae)?.rec.tag_bpm.is_none() {
        // Older library entries didn't record the tag value; read it now.
        if let Some(path) = crate::library::resolve_path(&app.store, &id) {
            let t = crate::library::read_tags(&path);
            if t.bpm_source.as_deref() == Some("tag") {
                tag = t.bpm;
            }
        }
    }
    if app.store.reset_bpm(&id, &from, tag).map_err(ae)? {
        app.queue_analysis(id.clone(), true);
    }
    emit_recording_changed(&app, &id);
    app.emit_perform();
    app.store.recording(&id).map_err(ae)
}

pub fn recording_waveform(app: &Arc<App>, id: Id) -> Res<Vec<u8>> {
    Ok(app.store.waveform(&id).unwrap_or_default())
}

/// Cover art: a data URL of the local file's art, else the Spotify artwork URL.
pub fn recording_artwork(app: &Arc<App>, id: Id) -> Res<Option<String>> {
    use base64::Engine;
    let local = spawn_blocking({
        let store = app.store.clone();
        let id = id.clone();
        move || crate::library::resolve_path(&store, &id).and_then(|p| crate::library::artwork(&p))
    })
    .map_err(err)?;
    if let Some((mime, bytes)) = local {
        return Ok(Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))));
    }
    Ok(app.store.recording(&id).ok().and_then(|r| r.rec.artwork_url.clone()))
}

pub fn recordings_analyze(app: &Arc<App>, ids: Vec<Id>) -> Res<()> {
    for id in ids {
        app.queue_analysis(id, true);
    }
    Ok(())
}

/// A new song (in the library) for a recording, e.g. a hidden one.
pub fn song_from_recording(app: &Arc<App>, id: Id) -> Res<Track> {
    let sid = app.store.song_from_recording(&id, true).map_err(ae)?;
    emit_recording_changed(&app, &id);
    app.store.track(&sid).map_err(ae)
}

/// How another recording's audio compares with the first one's (fingerprints).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioMatch {
    pub id: Id,
    /// Both were fingerprinted (analysed).
    pub known: bool,
    /// The longest stretch that is the same audio, as a share of the longer.
    pub coverage: f64,
    /// How much it differs over that stretch (bit error: 0 = identical).
    pub error: f64,
    /// The same recording (a copy: FLAC and MP3, a file and its Spotify track; not an edit).
    pub same: bool,
}

/// Compares recordings with the first: are they the same audio? (For Combine.)
pub fn recordings_compare(app: &Arc<App>, ids: Vec<Id>) -> Res<Vec<AudioMatch>> {
    let store = app.store.clone();
    blocking(move || {
        let fp = |id: &str| store.recording(id).ok().and_then(|r| r.rec.fingerprint).and_then(|f| crate::fingerprint::decode(&f).ok());
        let Some(first) = ids.first() else { return Ok(vec![]) };
        let base = fp(first);
        Ok(ids[1..]
            .iter()
            .map(|id| match (&base, fp(id)) {
                (Some(a), Some(b)) => {
                    let s = crate::fingerprint::similarity(a, &b);
                    AudioMatch { id: id.clone(), known: true, coverage: s.coverage, error: s.error, same: crate::fingerprint::same_version(a, &b) }
                }
                _ => AudioMatch { id: id.clone(), known: false, coverage: 0.0, error: 1.0, same: false },
            })
            .collect())
    })
}

/// Removes recordings from the library; versions playing them stay, showing
/// as missing until it's added again (files stay on disk).
pub fn recordings_remove(app: &Arc<App>, ids: Vec<Id>) -> Res<()> {
    app.store.remove_recordings(&ids).map_err(ae)?;
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    crate::app::emit(&app.handle, "recordings-changed", ());
    crate::app::emit(&app.handle, "setlists-changed", ());
    crate::app::emit(&app.handle, "crates-changed", ());
    Ok(())
}

/// Takes one file out of the library (see `Store::remove_audio_file`).
pub fn audio_file_remove(app: &Arc<App>, hash: String) -> Res<()> {
    app.store.remove_audio_file(&hash).map_err(ae)?;
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    crate::app::emit(&app.handle, "recordings-changed", ());
    Ok(())
}

/// Recordings that sound the same as missing audio songs still play.
pub fn reconnect_matches(app: &Arc<App>) -> Res<Vec<Reconnect>> {
    let store = app.store.clone();
    blocking(move || Ok(store.reconnect_matches(None)))
}

/// Lets `found` stand in for the missing recording `missing`.
pub fn reconnect(app: &Arc<App>, missing: Id, found: Id) -> Res<()> {
    app.store.reconnect(&missing, &found).map_err(ae)?;
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    crate::app::emit(&app.handle, "recordings-changed", ());
    app.emit_perform();
    Ok(())
}

/// Writes a set as a set file (`*.orpheus-set.toml`).
pub fn setlist_export(app: &Arc<App>, id: Id, path: String) -> Res<()> {
    // The club guidelines it was planned with travel with it.
    let club = app.store.setlist(&id).ok().and_then(|s| s.club).and_then(|c| app.club(&c));
    let rules = club.as_deref().map(|c| club::to_toml(&club::self_contained(c)));
    let text = crate::setfile::export(&app.store, &id, club.map(|c| c.name.clone()), rules).map_err(ae)?;
    crate::store::write_atomic(std::path::Path::new(&path), text.as_bytes()).map_err(ae)
}

/// Adds a set from a set file, linking its songs to the library's audio.
pub fn setlist_import(app: &Arc<App>, path: String) -> Res<crate::setfile::ImportReport> {
    let text = std::fs::read_to_string(&path).map_err(err)?;
    let file: crate::setfile::SetFile = toml::from_str(&text).map_err(|e| format!("Not an Orpheus set file: {e}"))?;
    // Its club guidelines: that version if you have it, else added from the
    // file (into the club of that id if you have it), else your latest.
    let clubs = app.clubs();
    let club = match file.club.as_ref() {
        None => None,
        Some(c) if clubs.club(&c.id, Some(&c.version)).is_ok() => Some(ClubRef { id: c.id.clone(), version: c.version.clone() }),
        Some(c) => match c.rules.as_deref() {
            Some(rules) => {
                let into = clubs.latest(&c.id).map(|_| c.id.clone());
                let r = clubs.add(rules, "set file", "", into.as_deref(), &HashMap::new(), &Default::default(), now_ms()).map_err(ae)?;
                clubs_changed(&app);
                Some(ClubRef { id: r.club_id, version: r.version_id })
            }
            None => clubs.latest(&c.id).map(|v| ClubRef { id: c.id.clone(), version: v }),
        },
    };
    let report = crate::setfile::import(&app.store, &text, club).map_err(ae)?;
    crate::app::emit(&app.handle, "setlists-changed", ());
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    crate::app::emit(&app.handle, "recordings-changed", ());
    Ok(report)
}

/// Writes a crate as a crate file (`*.orpheus-crate.toml`).
pub fn crate_export(app: &Arc<App>, id: Id, path: String) -> Res<()> {
    let text = crate::setfile::export_crate(&app.store, &id).map_err(ae)?;
    crate::store::write_atomic(std::path::Path::new(&path), text.as_bytes()).map_err(ae)
}

/// Adds a crate from a crate file, linking its songs to the library's audio.
pub fn crate_import(app: &Arc<App>, path: String) -> Res<crate::setfile::ImportReport> {
    let text = std::fs::read_to_string(&path).map_err(err)?;
    let report = crate::setfile::import_crate(&app.store, &text).map_err(ae)?;
    crate::app::emit(&app.handle, "crates-changed", ());
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    crate::app::emit(&app.handle, "recordings-changed", ());
    Ok(report)
}

/// The audio files in the library folders, with what each is in the library.
pub fn library_files(app: &Arc<App>) -> Res<Vec<LibraryFile>> {
    Ok(app.store.library_files())
}

/// Adds files to the library, including ones removed from it before.
pub fn library_add_files(app: &Arc<App>, paths: Vec<String>) -> Res<()> {
    for p in &paths {
        if let Some(f) = app.store.file_info(p) {
            app.store.include_file(&f.hash).map_err(ae)?;
        }
    }
    app.import_paths(paths);
    Ok(())
}

pub fn library_rescan(app: &Arc<App>) -> Res<()> {
    app.scan_library();
    Ok(())
}

/// Re-reads the library folder (e.g. after `git pull`).
pub fn library_reload(app: &Arc<App>) -> Res<Vec<String>> {
    let dir = app.store.library_dir();
    app.store.reload(&dir).map_err(ae)?;
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    crate::app::emit(&app.handle, "setlists-changed", ());
    crate::app::emit(&app.handle, "crates-changed", ());
    crate::app::emit(&app.handle, "profiles-changed", ());
    app.emit_perform();
    Ok(app.store.load_errors())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryInfo {
    pub library_dir: String,
    pub data_dir: String,
    pub load_errors: Vec<String>,
}

pub fn library_info(app: &Arc<App>) -> Res<LibraryInfo> {
    Ok(LibraryInfo {
        library_dir: app.store.library_dir().to_string_lossy().to_string(),
        data_dir: app.local_dir.to_string_lossy().to_string(),
        load_errors: app.store.load_errors(),
    })
}

pub fn library_import(app: &Arc<App>, paths: Vec<String>) -> Res<()> {
    app.import_paths(paths);
    Ok(())
}

pub fn scan_progress(app: &Arc<App>) -> Res<crate::library::ScanProgress> {
    Ok(app.scan_progress.lock().clone())
}

pub fn history(app: &Arc<App>, limit: usize) -> Res<Vec<HistoryItem>> {
    Ok(app.store.history(limit))
}

// ---------------------------------------------------------------- sources & versions

/// Adds Spotify tracks to the library (idempotent). Returns song ids in order.
pub fn spotify_import(app: &Arc<App>, tracks: Vec<SpotifyTrack>, in_library: Option<bool>) -> Res<Vec<Id>> {
    let in_library = in_library.unwrap_or(true);
    let mut ids = vec![];
    for t in tracks {
        let src = SourceRef { kind: "spotify".into(), key: t.uri.clone() };
        let nt = NewTrack { title: t.title, artist: t.artist, album: t.album, duration_ms: t.duration_ms, artwork_url: t.artwork_url, ..Default::default() };
        let (rid, _) = app.store.insert_recording(&nt, src).map_err(ae)?;
        let id = match app.store.song_for_recording(&rid) {
            Some(existing) => {
                // Already known: promote set-only songs when added to the library.
                if in_library && !app.store.track(&existing).map_err(ae)?.rec.in_library {
                    app.store.set_in_library(&existing, true).map_err(ae)?;
                }
                existing
            }
            None => app.store.song_from_recording(&rid, in_library).map_err(ae)?,
        };
        ids.push(id);
    }
    crate::app::emit(&app.handle, "library-changed", ids.clone());
    crate::app::emit(&app.handle, "recordings-changed", ());
    Ok(ids)
}

/// Adds a set-only song to the library (or hides it again).
pub fn track_set_in_library(app: &Arc<App>, id: Id, value: bool) -> Res<Track> {
    app.store.set_in_library(&id, value).map_err(ae)?;
    if !value {
        app.prune();
    }
    crate::app::emit(&app.handle, "library-changed", vec![id.clone()]);
    app.store.track(&id).map_err(ae)
}

/// Links a Spotify track (URI or URL) as another source of a recording (the same audio).
pub fn source_link_spotify(app: &Arc<App>, recording_id: Id, uri: String) -> Res<Recording> {
    let uri = crate::spotify::normalize_uri(&uri)
        .filter(|u| u.starts_with("spotify:track:"))
        .ok_or("Not a Spotify track link")?;
    app.store.add_source(&recording_id, SourceRef { kind: "spotify".into(), key: uri }).map_err(ae)?;
    emit_recording_changed(&app, &recording_id);
    app.store.recording(&recording_id).map_err(ae)
}

/// Looks for each recording on Spotify and links the same audio (in the background).
pub fn recordings_find_spotify(app: &Arc<App>, ids: Vec<Id>) -> Res<()> {
    app.find_on_spotify(ids).map_err(ae)
}

pub fn source_remove(app: &Arc<App>, recording_id: Id, source: SourceRef) -> Res<Recording> {
    app.store.remove_source(&recording_id, &source).map_err(ae)?;
    emit_recording_changed(&app, &recording_id);
    app.store.recording(&recording_id).map_err(ae)
}

/// Starts an action the DJ can undo (one or more calls); returns its id.
pub fn undo_begin(app: &Arc<App>) -> Res<u64> {
    Ok(app.undo_begin())
}

/// Ends it: `true` if it changed anything (so there's something to undo).
pub fn undo_end(app: &Arc<App>, id: u64) -> Res<bool> {
    Ok(app.undo_end(id))
}

pub fn undo(app: &Arc<App>, id: u64) -> Res<()> {
    app.undo(id).map_err(ae)
}

/// Combines songs into `keep` (their versions move; undo with `versions_separate`).
pub fn songs_combine(app: &Arc<App>, keep: Id, others: Vec<Id>) -> Res<Track> {
    app.store.combine_songs(&keep, &others).map_err(ae)?;
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    crate::app::emit(&app.handle, "recordings-changed", ());
    app.emit_perform();
    app.store.track(&keep).map_err(ae)
}

/// Moves versions of one song into a new song; returns it.
pub fn versions_separate(app: &Arc<App>, version_ids: Vec<Id>) -> Res<Track> {
    let id = app.store.separate_versions(&version_ids).map_err(ae)?;
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    crate::app::emit(&app.handle, "recordings-changed", ());
    app.emit_perform();
    app.store.track(&id).map_err(ae)
}

/// Marks recordings as the same audio, kept as `keep` (undo with `recording_split_source`).
pub fn recordings_merge(app: &Arc<App>, keep: Id, others: Vec<Id>) -> Res<Recording> {
    app.store.merge_recordings(&keep, &others).map_err(ae)?;
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    crate::app::emit(&app.handle, "recordings-changed", ());
    app.emit_perform();
    app.store.recording(&keep).map_err(ae)
}

/// Takes a source out into a recording of its own (analysed afresh): `versions`
/// play it, and with `new_song` a song of its own does.
pub fn recording_split_source(app: &Arc<App>, id: Id, source: SourceRef, versions: Vec<Id>, new_song: bool) -> Res<Recording> {
    let new = app.store.split_source(&id, &source, &versions, new_song).map_err(ae)?;
    app.queue_analysis(new.clone(), true);
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    crate::app::emit(&app.handle, "recordings-changed", ());
    app.emit_perform();
    app.store.recording(&new).map_err(ae)
}

pub fn version_create(app: &Arc<App>, track_id: Id, version: serde_json::Value) -> Res<Version> {
    let mut v: Version = serde_json::from_value(serde_json::json!({ "id": "", "name": "New version" }))
        .map_err(err)?;
    if let Some(o) = version.as_object() {
        let mut base = serde_json::to_value(&v).map_err(err)?;
        for (k, val) in o {
            base[k] = val.clone();
        }
        base["id"] = serde_json::Value::String(String::new());
        v = serde_json::from_value(base).map_err(err)?;
    }
    let v = app.store.version_create(&track_id, v).map_err(ae)?;
    crate::app::emit(&app.handle, "library-changed", vec![track_id]);
    Ok(v)
}

/// Audio for a new version: a file, a Spotify track, or audio already in the library.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioPick {
    pub path: Option<String>,
    pub spotify: Option<SpotifyTrack>,
    pub recording: Option<Id>,
}

/// Adds a version that plays other audio (an edit) to a song, named after that audio.
pub fn version_add_audio(app: &Arc<App>, track_id: Id, audio: AudioPick) -> Res<Version> {
    let (rid, new, name) = if let Some(p) = &audio.path {
        let path = std::path::Path::new(p);
        let (rid, new) = crate::library::add_file(&app.store, path).map_err(ae)?;
        (rid, new, path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default())
    } else if let Some(t) = audio.spotify {
        let src = SourceRef { kind: "spotify".into(), key: t.uri.clone() };
        let nt = NewTrack { title: t.title.clone(), artist: t.artist, album: t.album, duration_ms: t.duration_ms, artwork_url: t.artwork_url, ..Default::default() };
        let (rid, new) = app.store.insert_recording(&nt, src).map_err(ae)?;
        (rid, new, t.title)
    } else if let Some(r) = audio.recording {
        let title = app.store.recording(&r).map_err(ae)?.rec.title;
        (r, false, title)
    } else {
        return Err("no audio chosen".into());
    };
    let track = app.store.track(&track_id).map_err(ae)?;
    let base = if name.trim().is_empty() { "Edit".to_string() } else { name.trim().to_string() };
    let taken = |n: &str| track.rec.versions.iter().any(|v| v.name == n);
    let name = std::iter::once(base.clone()).chain((2..).map(|i| format!("{base} ({i})"))).find(|n| !taken(n)).unwrap_or(base);
    let mut v: Version = serde_json::from_value(serde_json::json!({ "id": "", "name": name })).map_err(err)?;
    v.recording = rid.clone();
    let v = app.store.version_create(&track_id, v).map_err(ae)?;
    if new {
        app.queue_analysis(rid, true);
    }
    crate::app::emit(&app.handle, "library-changed", vec![track_id]);
    crate::app::emit(&app.handle, "recordings-changed", ());
    Ok(v)
}

pub fn version_update(app: &Arc<App>, id: Id, patch: serde_json::Value) -> Res<Version> {
    let patch: VersionPatch = serde_json::from_value(patch).map_err(err)?;
    let v = app.store.version_update(&id, &patch).map_err(ae)?;
    if patch.gain_db.is_some() {
        app.relevel_decks(true);
    }
    let (_, tid) = app.store.version(&id).map_err(ae)?;
    crate::app::emit(&app.handle, "library-changed", vec![tid]);
    app.emit_perform();
    Ok(v)
}

/// Takes a version out of the library (hidden while a set plays it).
/// Puts a song's versions in this order (the Info panel's order).
pub fn versions_reorder(app: &Arc<App>, track_id: Id, ids: Vec<Id>) -> Res<()> {
    app.store.versions_reorder(&track_id, &ids).map_err(ae)?;
    crate::app::emit(&app.handle, "library-changed", vec![track_id]);
    Ok(())
}

pub fn version_delete(app: &Arc<App>, id: Id) -> Res<()> {
    let (_, tid) = app.store.version(&id).map_err(ae)?;
    let keep: Vec<Id> = app.decks.iter().filter_map(|d| d.track_id()).collect();
    app.store.remove_version(&id, &keep).map_err(ae)?;
    crate::app::emit(&app.handle, "library-changed", vec![tid]);
    crate::app::emit(&app.handle, "crates-changed", ());
    Ok(())
}

// ---------------------------------------------------------------- crates

pub fn crates_list(app: &Arc<App>) -> Res<Vec<Crate>> {
    Ok(app.store.crates())
}

pub fn crate_create(app: &Arc<App>, name: String, smart: Option<serde_json::Value>) -> Res<Id> {
    let id = app.store.crate_create(&name, smart).map_err(ae)?;
    crate::app::emit(&app.handle, "crates-changed", ());
    Ok(id)
}

pub fn crate_rename(app: &Arc<App>, id: Id, name: String) -> Res<()> {
    app.store.crate_rename(&id, &name).map_err(ae)?;
    crate::app::emit(&app.handle, "crates-changed", ());
    Ok(())
}

pub fn crate_delete(app: &Arc<App>, id: Id) -> Res<()> {
    app.store.crate_delete(&id).map_err(ae)?;
    app.prune();
    crate::app::emit(&app.handle, "crates-changed", ());
    Ok(())
}

pub fn crate_add(app: &Arc<App>, id: Id, version_ids: Vec<Id>) -> Res<()> {
    app.store.crate_add(&id, &version_ids).map_err(ae)?;
    crate::app::emit(&app.handle, "crates-changed", ());
    Ok(())
}

/// Sets a smart crate's rules (`None` makes it a plain crate).
pub fn crate_set_smart(app: &Arc<App>, id: Id, smart: Option<serde_json::Value>) -> Res<()> {
    app.store.crate_set_smart(&id, smart).map_err(ae)?;
    crate::app::emit(&app.handle, "crates-changed", ());
    Ok(())
}

/// Adds a dance style to versions (a version can have several). Songs used
/// only in setlists (e.g. from Spotify) join the library.
pub fn style_add(app: &Arc<App>, style: String, version_ids: Vec<Id>) -> Res<()> {
    for v in &version_ids {
        app.store.assign_style(v, &style).map_err(ae)?;
        let (_, track_id) = app.store.version(v).map_err(ae)?;
        if !app.store.track(&track_id).map_err(ae)?.rec.in_library {
            app.store.set_in_library(&track_id, true).map_err(ae)?;
        }
    }
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    app.emit_perform();
    Ok(())
}

/// Removes a dance style (or, with `style = null`, all styles) from versions.
pub fn style_remove(app: &Arc<App>, version_ids: Vec<Id>, style: Option<String>) -> Res<()> {
    for v in &version_ids {
        app.store.unassign_style(v, style.as_deref()).map_err(ae)?;
    }
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    app.emit_perform();
    Ok(())
}

pub fn crate_remove(app: &Arc<App>, id: Id, version_ids: Vec<Id>) -> Res<()> {
    app.store.crate_remove(&id, &version_ids).map_err(ae)?;
    app.prune();
    crate::app::emit(&app.handle, "crates-changed", ());
    Ok(())
}

// ---------------------------------------------------------------- setlists

pub fn setlists_list(app: &Arc<App>) -> Res<Vec<SetlistSummary>> {
    Ok(app.store.setlists())
}

pub fn setlist_get(app: &Arc<App>, id: Id) -> Res<Setlist> {
    app.store.setlist(&id).map_err(ae)
}

pub fn setlist_create(app: &Arc<App>, name: String, mode: String, club_id: Option<String>, event_date: Option<String>) -> Res<Setlist> {
    let club = club_id.filter(|c| !c.is_empty()).and_then(|id| Some(ClubRef { version: app.clubs().latest(&id)?, id }));
    let id = app.store.setlist_create(&name, &mode, club, event_date).map_err(ae)?;
    crate::app::emit(&app.handle, "setlists-changed", ());
    app.store.setlist(&id).map_err(ae)
}

pub fn setlist_update(app: &Arc<App>, id: Id, patch: serde_json::Value) -> Res<Setlist> {
    let patch: SetlistPatch = serde_json::from_value(patch).map_err(err)?;
    app.store.setlist_update(&id, &patch).map_err(ae)?;
    crate::app::emit(&app.handle, "setlists-changed", ());
    app.emit_setlist(&id);
    app.emit_perform();
    app.store.setlist(&id).map_err(ae)
}

pub fn setlist_delete(app: &Arc<App>, id: Id) -> Res<()> {
    if app.perform.lock().setlist_id.as_deref() == Some(id.as_str()) {
        app.open_setlist(None).map_err(ae)?;
    }
    app.store.setlist_delete(&id).map_err(ae)?;
    app.prune();
    crate::app::emit(&app.handle, "setlists-changed", ());
    // Play counts are derived from setlists.
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    Ok(())
}

pub fn setlist_duplicate(app: &Arc<App>, id: Id, name: String) -> Res<Setlist> {
    let new_id = app.store.setlist_duplicate(&id, &name).map_err(ae)?;
    crate::app::emit(&app.handle, "setlists-changed", ());
    app.store.setlist(&new_id).map_err(ae)
}

pub fn setlist_reset(app: &Arc<App>, id: Id) -> Res<()> {
    app.store.reset_setlist_status(&id).map_err(ae)?;
    app.emit_setlist(&id);
    app.emit_perform();
    // Play counts are derived from setlists.
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    Ok(())
}

/// Adds entries to a set. Songs going into a club's set get the tempo change
/// its style needs (see `App::club_tempo`).
pub fn entries_insert(app: &Arc<App>, setlist_id: Id, position: Option<usize>, mut entries: Vec<NewEntry>) -> Res<Vec<Id>> {
    let set = app.store.setlist(&setlist_id).map_err(ae)?;
    for e in entries.iter_mut() {
        let as_entry = SetlistEntry {
            version_id: e.version_id.clone(),
            styles: e.styles.clone(),
            club_styles: e.club_styles.clone(),
            label: e.label.clone(),
            overrides: e.overrides.clone(),
            ..Default::default()
        };
        if let Some(bpm) = app.club_tempo(&set, &as_entry) {
            e.overrides.target_bpm = Some(bpm);
        }
    }
    let ids = app.store.entries_insert(&setlist_id, position, &entries).map_err(ae)?;
    app.emit_setlist(&setlist_id);
    app.emit_perform();
    Ok(ids)
}

pub fn entry_update(app: &Arc<App>, id: Id, patch: serde_json::Value) -> Res<SetlistEntry> {
    let patch: EntryPatch = serde_json::from_value(patch).map_err(err)?;
    let replaced = patch.version_id.is_some();
    let mut e = app.store.entry_update(&id, &patch).map_err(ae)?;
    // A song put into a slot of a club's set gets the tempo its style needs.
    if replaced {
        let (_, sid) = app.store.entry(&id).map_err(ae)?;
        let set = app.store.setlist(&sid).map_err(ae)?;
        if let Some(bpm) = app.club_tempo(&set, &e) {
            let mut overrides = e.overrides.clone();
            overrides.target_bpm = Some(bpm);
            e = app.store.entry_update(&id, &EntryPatch { overrides: Some(overrides), ..Default::default() }).map_err(ae)?;
        }
    }
    if replaced {
        app.prune();
        app.entry_version_changed(&e);
    }
    if patch.overrides.is_some() {
        app.relevel_decks(true);
    }
    let (_, sid) = app.store.entry(&id).map_err(ae)?;
    app.emit_setlist(&sid);
    app.emit_perform();
    Ok(e)
}

pub fn entries_delete(app: &Arc<App>, setlist_id: Id, ids: Vec<Id>) -> Res<()> {
    app.store.entries_delete(&setlist_id, &ids).map_err(ae)?;
    app.prune();
    app.emit_setlist(&setlist_id);
    app.emit_perform();
    // Play counts are derived from setlists.
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    Ok(())
}

pub fn entries_reorder(app: &Arc<App>, setlist_id: Id, ids: Vec<Id>) -> Res<()> {
    app.store.entries_reorder(&setlist_id, &ids).map_err(ae)?;
    app.emit_setlist(&setlist_id);
    app.emit_perform();
    Ok(())
}

pub fn write_text_file(path: String, contents: String) -> Res<()> {
    crate::store::write_atomic(std::path::Path::new(&path), contents.as_bytes()).map_err(ae)
}

pub fn read_text_file(path: String) -> Res<String> {
    std::fs::read_to_string(&path).map_err(err)
}

// ---------------------------------------------------------------- your styles

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StylesInfo {
    #[serde(flatten)]
    pub library: StyleLibrary,
    /// How many versions are tagged with each style key.
    pub usage: std::collections::BTreeMap<String, usize>,
    /// How many versions carry each of your tags.
    pub tag_usage: std::collections::BTreeMap<String, usize>,
}

pub fn styles_get(app: &Arc<App>) -> Res<StylesInfo> {
    Ok(StylesInfo { library: app.style_library(), usage: app.store.style_usage(), tag_usage: app.store.tag_usage() })
}

pub fn styles_save(app: &Arc<App>, mut library: StyleLibrary) -> Res<()> {
    library.upgrade();
    app.store.save_style_library(library).map_err(ae)?;
    crate::app::emit(&app.handle, "styles-changed", ());
    app.emit_perform();
    Ok(())
}

/// Renames one of your styles everywhere: songs, setlists, smart crates and
/// club mappings. Renaming onto an existing style merges the two; `to = null`
/// deletes the style (and removes it from every song).
pub fn styles_rekey(app: &Arc<App>, from: String, to: Option<String>) -> Res<()> {
    let to = to.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    let mut lib = app.style_library();
    match to.as_deref() {
        Some(t) if lib.styles.iter().any(|s| s.key == t) => lib.styles.retain(|s| s.key != from),
        Some(t) => lib.styles.iter_mut().filter(|s| s.key == from).for_each(|s| s.key = t.to_string()),
        None => lib.styles.retain(|s| s.key != from),
    }
    // Rules for the style follow it (merging keeps both styles' rules), or go with it.
    for (club, rules) in std::mem::take(&mut lib.club_rules) {
        for mut r in rules {
            if r.mine == from {
                match to.as_deref() {
                    Some(t) => r.mine = t.to_string(),
                    None => continue,
                }
            }
            lib.add_rule(&club, r);
        }
    }
    app.store.save_style_library(lib).map_err(ae)?;
    app.store.rekey_style(&from, to.as_deref()).map_err(ae)?;
    crate::app::emit(&app.handle, "crates-changed", ());
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    crate::app::emit(&app.handle, "styles-changed", ());
    app.emit_perform();
    Ok(())
}

/// Renames one of your tags everywhere (onto an existing tag: merges);
/// `to = null` deletes it from every song.
pub fn tags_rekey(app: &Arc<App>, from: String, to: Option<String>) -> Res<()> {
    let to = to.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    let mut lib = app.style_library();
    match to.as_deref() {
        Some(t) if lib.tags.iter().any(|s| s.key == t) => lib.tags.retain(|s| s.key != from),
        Some(t) => lib.tags.iter_mut().filter(|s| s.key == from).for_each(|s| s.key = t.to_string()),
        None => lib.tags.retain(|s| s.key != from),
    }
    app.store.save_style_library(lib).map_err(ae)?;
    app.store.rekey_tag(&from, to.as_deref()).map_err(ae)?;
    crate::app::emit(&app.handle, "crates-changed", ());
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    crate::app::emit(&app.handle, "styles-changed", ());
    Ok(())
}

/// Adds (`on`) or removes one of your tags on versions.
pub fn tag_set(app: &Arc<App>, tag: String, version_ids: Vec<Id>, on: bool) -> Res<()> {
    for v in &version_ids {
        app.store.set_version_tag(v, &tag, on).map_err(ae)?;
    }
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    Ok(())
}

// ---------------------------------------------------------------- announcements (auto mode)

pub fn tts_status(app: &Arc<App>) -> Res<crate::tts::TtsStatus> {
    Ok(app.tts_status())
}

/// Downloads Piper and the selected voice (in the background).
pub fn tts_install(app: &Arc<App>) -> Res<()> {
    let a = app.clone();
    std::thread::spawn(move || {
        let cfg = a.settings.read().announce.tts_config();
        if let Err(e) = a.tts.install(&cfg) {
            crate::app::emit(&a.handle, "toast", format!("Installing the announcer voice failed: {e:#}"));
        }
        a.emit_tts_status();
    });
    Ok(())
}

pub fn tts_voices(app: &Arc<App>) -> Res<Vec<crate::tts::VoiceInfo>> {
    let a = app.clone();
    blocking(move || a.tts.voices())
}

pub fn tts_remove(app: &Arc<App>) -> Res<()> {
    app.tts.remove().map_err(ae)?;
    app.emit_tts_status();
    Ok(())
}

/// Speaks `text` now (to try a voice or pattern).
pub fn tts_say(app: &Arc<App>, text: String) -> Res<()> {
    app.speak(text, None);
    Ok(())
}

/// Plays a chime (a version) to try it.
pub fn chime_preview(app: &Arc<App>, version_id: Id) -> Res<()> {
    let a = app.clone();
    std::thread::spawn(move || a.preview_chime(&version_id));
    Ok(())
}

/// Plays the next song's chime now.
pub fn chime_next(app: &Arc<App>) -> Res<()> {
    let v = app.next_chime().map_err(ae)?;
    chime_preview(app, v)
}

/// Chime versions in words (see `App::chime_label`).
pub fn chime_labels(app: &Arc<App>, version_ids: Vec<Id>) -> Res<Vec<String>> {
    Ok(version_ids.iter().map(|v| app.chime_label(v)).collect())
}

/// The phonemes the announcer would read for `text` (with your pronunciations).
pub fn tts_phonemes(app: &Arc<App>, text: String) -> Res<String> {
    let a = app.clone();
    blocking(move || {
        let cfg = a.settings.read().announce.tts_config();
        a.tts.phonemes(&cfg, &text, &a.style_library().pronunciations)
    })
}

/// Announces the next dance now.
pub fn announce_next(app: &Arc<App>) -> Res<()> {
    app.announce_next().map_err(ae)
}

// ---------------------------------------------------------------- clubs

fn clubs_changed(app: &App) {
    app.forget_clubs();
    crate::app::emit(&app.handle, "clubs-changed", ());
    app.emit_perform();
}

/// Every club with the versions of its guidelines, newest first.
pub fn clubs_list(app: &Arc<App>) -> Res<Vec<ClubInfo>> {
    Ok(app.clubs().list())
}

/// One version: its file text and what it defines.
pub fn club_get(app: &Arc<App>, club_id: String, version_id: String) -> Res<ClubVersion> {
    app.clubs().get(&club_id, &version_id).map_err(ae)
}

/// Checks a club file's text (the editor's text view): the club, or the error.
pub fn club_parse(text: String) -> Res<Club> {
    club::parse(&text).map_err(ae)
}

/// The editor's form as file text.
pub fn club_format(club: Club) -> Res<String> {
    Ok(club::to_toml(&club))
}

/// How many sets use each version of a club.
pub fn club_usage(app: &Arc<App>, club_id: String) -> Res<HashMap<String, usize>> {
    let mut out = HashMap::new();
    for s in app.store.setlists() {
        if let Some(c) = s.club.filter(|c| c.id == club_id) {
            *out.entry(c.version).or_insert(0) += 1;
        }
    }
    Ok(out)
}

pub fn club_import_file(app: &Arc<App>, path: String) -> Res<Imported> {
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    let c = club::parse(&text).map_err(ae)?;
    let fps = app.block_on(club::fetch_fingerprints(&c, &path, &HashMap::new())).map_err(ae)?;
    let songs = app.block_on(club::fetch_song_libraries(&c, &path)).map_err(ae)?;
    let r = app.clubs().add(&text, &path, &path, None, &fps, &songs, now_ms()).map_err(ae)?;
    clubs_changed(&app);
    Ok(r)
}

/// Imports a club file published on the web.
pub fn club_import_url(app: &Arc<App>, url: String) -> Res<Imported> {
    let url = url.trim().to_string();
    let text = app.block_on(club::fetch(&url)).map_err(ae)?;
    let c = club::parse(&text).map_err(ae)?;
    let fps = app.block_on(club::fetch_fingerprints(&c, &url, &HashMap::new())).map_err(ae)?;
    let songs = app.block_on(club::fetch_song_libraries(&c, &url)).map_err(ae)?;
    let r = app.clubs().add(&text, &url, &url, None, &fps, &songs, now_ms()).map_err(ae)?;
    clubs_changed(&app);
    Ok(r)
}

/// Fetches the club's file again: a new version if it changed (sets keep
/// theirs until moved to it).
pub fn club_update(app: &Arc<App>, club_id: String) -> Res<Imported> {
    let url = app
        .clubs()
        .update_url(&club_id)
        .ok_or_else(|| "This club has no web address to update from (url = \"https://…\" in its file).".to_string())?;
    let text = app.block_on(club::fetch(&url)).map_err(ae)?;
    let c = club::parse(&text).map_err(ae)?;
    let fps = app.block_on(club::fetch_fingerprints(&c, &url, &HashMap::new())).map_err(ae)?;
    let songs = app.block_on(club::fetch_song_libraries(&c, &url)).map_err(ae)?;
    let r = app.clubs().add(&text, &url, &url, Some(&club_id), &fps, &songs, now_ms()).map_err(ae)?;
    clubs_changed(&app);
    Ok(r)
}

/// Saves edits to a version: in place while it's the newest and no set uses
/// it, else as a new version (sets keep the guidelines they were planned with).
pub fn club_save(app: &Arc<App>, club_id: String, version_id: String, text: String) -> Res<Imported> {
    let clubs = app.clubs();
    let used = app.store.setlists().iter().any(|s| s.club.as_ref().is_some_and(|c| c.id == club_id && c.version == version_id));
    let in_place = !used && clubs.latest(&club_id).as_deref() == Some(version_id.as_str());
    // Fingerprints it refers to: those already fetched, and any new ones.
    let c = club::parse(&text).map_err(ae)?;
    let base = clubs.base(&club_id, &version_id);
    let fps = app.block_on(club::fetch_fingerprints(&c, &base, &clubs.fingerprints(&club_id, &version_id))).map_err(ae)?;
    // The same song libraries as before: keep what was fetched (works offline).
    let before = clubs.club(&club_id, Some(&version_id)).ok();
    let songs = match before {
        Some(b) if b.song_library == c.song_library => b.library,
        _ => app.block_on(club::fetch_song_libraries(&c, &base)).map_err(ae)?,
    };
    let r = clubs.save(&club_id, &version_id, &text, in_place, &fps, &songs, now_ms()).map_err(ae)?;
    clubs_changed(&app);
    Ok(r)
}

/// The example clubs that come with Orpheus: (id, name, description).
pub fn club_examples() -> Res<Vec<(String, String, String)>> {
    Ok(club::EXAMPLES
        .iter()
        .filter_map(|(id, text)| club::parse(text).ok().map(|c| (id.to_string(), c.name, c.description)))
        .collect())
}

pub fn club_import_example(app: &Arc<App>, id: String) -> Res<Imported> {
    let (_, text) = club::EXAMPLES.iter().find(|(e, _)| *e == id).ok_or_else(|| format!("no example club \"{id}\""))?;
    let c = club::parse(text).map_err(ae)?;
    let fps = app.block_on(club::fetch_fingerprints(&c, club::EXAMPLE_BASE, &HashMap::new())).map_err(ae)?;
    let songs = app.block_on(club::fetch_song_libraries(&c, club::EXAMPLE_BASE)).map_err(ae)?;
    let r = app.clubs().add(text, "example", club::EXAMPLE_BASE, None, &fps, &songs, now_ms()).map_err(ae)?;
    clubs_changed(&app);
    Ok(r)
}

/// Library songs that are songs the club lists for its styles: track id → style ids.
pub fn club_song_matches(app: &Arc<App>, club_id: String, version_id: String) -> Res<HashMap<Id, Vec<String>>> {
    let a = app.clone();
    blocking(move || Ok(a.club_song_matches(&ClubRef { id: club_id, version: version_id })))
}

/// For each style with suggested songs: each song's matching library tracks.
pub fn club_song_entries(app: &Arc<App>, club_id: String, version_id: String) -> Res<HashMap<String, Vec<Vec<Id>>>> {
    let a = app.clone();
    blocking(move || Ok(a.club_song_entries(&ClubRef { id: club_id, version: version_id })))
}

/// A club-file song entry for a version of one of your songs (to accept it
/// for a style): the song's title, its recording's audio.
pub fn club_song_for_version(app: &Arc<App>, version_id: Id) -> Res<club::ClubSong> {
    let (_, tid) = app.store.version(&version_id).map_err(ae)?;
    let t = app.store.track(&tid).map_err(ae)?;
    let (_, r) = app.store.version_recording(&version_id).map_err(ae)?;
    let r = r.rec;
    Ok(club::ClubSong {
        title: Some(t.rec.title.clone()).filter(|s| !s.is_empty()),
        artist: Some(t.rec.artist.clone()).filter(|s| !s.is_empty()),
        length: (r.duration_ms > 0).then(|| (r.duration_ms as f64 / 1000.0).round()),
        fingerprint: r.fingerprint.clone(),
        spotify: r.sources.iter().find(|s| s.kind == "spotify").map(|s| s.key.clone()),
        hash: r.sources.iter().find(|s| s.kind == "file").map(|s| s.key.clone()),
        ..Default::default()
    })
}

/// Downloads a club's suggested song into Orpheus's downloads folder and
/// adds it to the library, if the file is the one the club lists (`hash`).
pub fn club_song_download(app: &Arc<App>, url: String, hash: String) -> Res<String> {
    if !club::valid_hash(&hash) {
        return Err("The club gives no valid hash for this download".into());
    }
    let resp = app.block_on(reqwest::get(&url)).map_err(|e| format!("{url}: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("{url}: {}", resp.status()));
    }
    let name = url.rsplit('/').next().unwrap_or("").split(['?', '#']).next().unwrap_or("").to_string();
    let name: String = name.chars().filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')).collect();
    let name = if name.is_empty() { format!("{hash}.audio") } else { percent_decode(&name) };
    let bytes = app.block_on(resp.bytes()).map_err(|e| format!("{url}: {e}"))?;
    let dir = app.data_dir.join("downloads");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // Never overwrite: a new name if taken.
    let mut path = dir.join(&name);
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{n}-{name}"));
        n += 1;
    }
    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    let got = crate::decode::content_hash(&path).map_err(ae);
    if got.as_deref() != Ok(hash.as_str()) {
        let _ = std::fs::remove_file(&path);
        return Err(format!("{url} isn't the file the club lists (its hash differs), so it wasn't added"));
    }
    let p = path.to_string_lossy().into_owned();
    app.import_paths(vec![p.clone()]);
    Ok(p)
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The same entry as club-file text (Info → Copy for a club file).
pub fn club_song_text(app: &Arc<App>, version_id: Id) -> Res<String> {
    let song = club_song_for_version(app, version_id)?;
    Ok(club::song_toml(&song))
}

/// A new club with nothing but a name.
pub fn club_new(app: &Arc<App>, name: String) -> Res<Imported> {
    let text = club::to_toml(&Club { name: name.trim().to_string(), ..Default::default() });
    let r = app.clubs().add(&text, "new", "", None, &HashMap::new(), &Default::default(), now_ms()).map_err(ae)?;
    clubs_changed(&app);
    Ok(r)
}

pub fn club_rename_version(app: &Arc<App>, club_id: String, version_id: String, name: String) -> Res<()> {
    app.clubs().rename_version(&club_id, &version_id, &name).map_err(ae)?;
    clubs_changed(&app);
    Ok(())
}

pub fn club_delete_version(app: &Arc<App>, club_id: String, version_id: String) -> Res<()> {
    app.clubs().delete_version(&club_id, &version_id).map_err(ae)?;
    clubs_changed(&app);
    Ok(())
}

pub fn club_delete(app: &Arc<App>, club_id: String) -> Res<()> {
    app.clubs().delete(&club_id).map_err(ae)?;
    clubs_changed(&app);
    Ok(())
}

/// Writes a version's file (to share it, or publish it on a website).
/// Writes a club's guidelines as one self-contained file (to publish on the
/// club's website): suggested songs and their fingerprints written in.
pub fn club_export(app: &Arc<App>, club_id: String, version_id: String, path: String) -> Res<()> {
    let club = app.clubs().club(&club_id, Some(&version_id)).map_err(ae)?;
    std::fs::write(&path, club::to_toml(&club::self_contained(&club))).map_err(|e| format!("{path}: {e}"))
}

// ---------------------------------------------------------------- settings

pub fn settings_get(app: &Arc<App>) -> Res<Settings> {
    Ok(app.settings.read().clone())
}

pub fn settings_set(app: &Arc<App>, settings: Settings) -> Res<()> {
    let (folders_changed, lib_changed) = {
        let cur = app.settings.read();
        (cur.library_folders != settings.library_folders, cur.library_dir != settings.library_dir)
    };
    let a = app.clone();
    blocking(move || a.save_settings(settings))?;
    if folders_changed || lib_changed {
        app.scan_library();
    }
    Ok(())
}

// ---------------------------------------------------------------- audio

pub fn audio_devices() -> Res<Vec<AudioDevice>> {
    blocking(|| Ok(crate::audio::output::list_devices()))
}

pub fn audio_status(app: &Arc<App>) -> Res<OutputStatus> {
    Ok(app.output_status())
}

pub fn audio_reopen(app: &Arc<App>) -> Res<OutputStatus> {
    let a = app.clone();
    blocking(move || {
        a.apply_outputs();
        Ok(a.output_status())
    })
}

pub fn audio_test(app: &Arc<App>, deck: usize) -> Res<()> {
    app.test_tone(deck.min(1));
    Ok(())
}

pub fn deck_states(app: &Arc<App>) -> Res<[crate::app::DeckState; 2]> {
    Ok(app.deck_states())
}

/// Loads a version onto a deck (paused unless `play`). Refused for the deck
/// the room is hearing.
/// Loads a version onto `deck` (none: the free deck, as the engine decides then).
pub fn deck_load(app: &Arc<App>, deck: Option<usize>, version_id: Id, entry_id: Option<Id>, play: bool) -> Res<()> {
    let a = app.clone();
    blocking(move || {
        let deck = deck.map_or_else(|| a.free_deck(), |d| d.min(1));
        a.deck_load(deck, &version_id, entry_id.as_deref(), play)
    })
}

/// Transport actions: `toggle`, `play`, `pause`, `fadeOut`, `cancelFade`, `stop`, `unload`.
pub fn deck_action(app: &Arc<App>, deck: usize, action: String) -> Res<()> {
    let deck = deck.min(1);
    let d = &app.decks[deck];
    match action.as_str() {
        "toggle" => app.deck_play_pause(deck),
        "play" => {
            if d.status() != "playing" {
                app.deck_play_pause(deck)
            }
        }
        "pause" => d.pause(),
        "fadeOut" => d.fade_out(if app.audible(deck) { app.fade_secs() } else { 1.0 }),
        "cancelFade" => {
            d.cancel_fade();
            app.perform.lock().pending = None;
        }
        "stop" => {
            if app.protected(deck) {
                return Err("Fade out or pause the on-air deck first".into());
            }
            d.pause();
            d.seek(d.cue_secs());
        }
        "unload" => {
            if app.audible(deck) {
                return Err("This deck is on air".into());
            }
            d.unload();
        }
        _ => return Err(format!("unknown action {action}")),
    }
    app.emit_perform();
    Ok(())
}

/// CUE button (press / release).
pub fn deck_cue(app: &Arc<App>, deck: usize, down: bool) -> Res<()> {
    app.deck_cue(deck.min(1), down);
    Ok(())
}

/// Hot cue pad: `trigger` (jump if set, else set here) or `clear`.
pub fn deck_hotcue(app: &Arc<App>, deck: usize, slot: u8, action: String) -> Res<()> {
    let deck = deck.min(1);
    match action.as_str() {
        "clear" => app.deck_hotcue_clear(deck, slot),
        _ => app.deck_hotcue(deck, slot),
    }
    .map_err(ae)
}

/// Relative jump in seconds (negative = back).
pub fn deck_jump(app: &Arc<App>, deck: usize, secs: f64) -> Res<()> {
    app.deck_nudge_position(deck.min(1), secs).map_err(ae)
}

/// Scrub to a position (waveform drag); plays snippets when paused.
pub fn deck_scrub(app: &Arc<App>, deck: usize, ms: f64, force: Option<bool>) -> Res<()> {
    app.deck_scrub_to(deck.min(1), ms / 1000.0, force.unwrap_or(false)).map_err(ae)
}

/// Seek (click on the waveform). `force` (⌥ / Alt held) overrides the
/// on-air protection.
pub fn deck_seek(app: &Arc<App>, deck: usize, ms: i64, force: Option<bool>) -> Res<()> {
    let deck = deck.min(1);
    if app.protected(deck) && !force.unwrap_or(false) {
        return Err("Seeking is disabled on the on-air deck while it plays (Settings → General)".into());
    }
    app.decks[deck].seek(ms as f64 / 1000.0);
    Ok(())
}

pub fn deck_go_live(app: &Arc<App>, deck: usize) -> Res<String> {
    let a = app.clone();
    blocking(move || a.go_live(deck.min(1)))
}

/// Sets the loaded version's `start`, `end` or `fade` point here, or `clear`s them.
pub fn deck_set_point(app: &Arc<App>, deck: usize, kind: String) -> Res<()> {
    app.deck_set_point(deck.min(1), &kind).map_err(ae)
}

pub fn deck_set(app: &Arc<App>, deck: usize, tempo: Option<f64>, trim_db: Option<f64>, pfl: Option<bool>, fade_at_ms: Option<i64>, clear_fade_at: Option<bool>) -> Res<()> {
    let deck = deck.min(1);
    let d = &app.decks[deck];
    if let Some(t) = tempo {
        d.set_tempo(t);
        app.untake(&[&format!("deck{deck}.tempo")]);
    }
    if let Some(g) = trim_db {
        d.set_trim_db(g);
    }
    if let Some(p) = pfl {
        app.deck_set_pfl(deck, p);
    }
    if let Some(f) = fade_at_ms {
        d.set_fade_at(Some(f as f64 / 1000.0));
    } else if clear_fade_at == Some(true) {
        d.set_fade_at(None);
    }
    Ok(())
}

/// Stem toggle (`index` in vocals, drums, bass, other order); prepares stems if missing.
pub fn deck_stem(app: &Arc<App>, deck: usize, index: usize) -> Res<()> {
    app.deck_stem_toggle(deck.min(1), index.min(3)).map_err(ae)
}

pub fn stems_status(app: &Arc<App>) -> Res<crate::stems::StemsStatus> {
    Ok(app.stems.status())
}

pub fn stems_download_model(app: &Arc<App>) -> Res<()> {
    app.stems.download_model();
    Ok(())
}

/// Queues stem separation for recordings; returns how many were queued.
pub fn stems_prepare(app: &Arc<App>, recording_ids: Vec<Id>) -> Res<usize> {
    app.prepare_stems(&recording_ids).map_err(ae)
}

pub fn stems_delete(app: &Arc<App>, recording_id: Id) -> Res<()> {
    let t = app.store.recording(&recording_id).map_err(ae)?;
    for s in t.rec.sources.iter().filter(|s| s.kind == "file") {
        app.stems.delete(&s.key).map_err(ae)?;
    }
    crate::app::emit(&app.handle, "stems-status", app.stems.status());
    crate::app::emit(&app.handle, "library-changed", app.store.songs_for_recording(&recording_id));
    Ok(())
}

/// Content hashes (of this track's files) that have prepared stems.
pub fn stems_delete_all(app: &Arc<App>) -> Res<()> {
    app.stems.delete_all().map_err(ae)?;
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    Ok(())
}

pub fn stems_remove_model(app: &Arc<App>) -> Res<()> {
    app.stems.remove_model().map_err(ae)
}

/// "Remove from History": marks those setlist entries as not played.
pub fn history_remove(app: &Arc<App>, entry_ids: Vec<Id>) -> Res<()> {
    for id in &entry_ids {
        let (_, sid) = app.store.entry(id).map_err(ae)?;
        app.store.entry_update(id, &EntryPatch { status: Some("pending".into()), ..Default::default() }).map_err(ae)?;
        app.emit_setlist(&sid);
    }
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    crate::app::emit(&app.handle, "history-changed", ());
    Ok(())
}

pub fn spotify_cache_bytes(app: &Arc<App>) -> Res<u64> {
    Ok(app.spotify.cache_bytes())
}

pub fn spotify_clear_cache(app: &Arc<App>) -> Res<u64> {
    app.spotify.clear_cache().map_err(ae)
}

/// Recordings with prepared stems.
pub fn stems_ready(app: &Arc<App>) -> Res<Vec<Id>> {
    Ok(app
        .store
        .all_recordings()
        .into_iter()
        .filter(|t| t.rec.sources.iter().any(|s| s.kind == "file" && app.stems.files(&s.key).is_some()))
        .map(|t| t.rec.id)
        .collect())
}

pub fn deck_tempo_reset(app: &Arc<App>, deck: usize) -> Res<()> {
    app.deck_tempo_reset(deck.min(1));
    Ok(())
}

pub fn levels_set(app: &Arc<App>, master: Option<f32>, cue: Option<f32>, crossfader: Option<f64>) -> Res<()> {
    if let Some(m) = master {
        app.levels.master.set(m.clamp(0.0, 1.5));
    }
    if let Some(c) = cue {
        app.levels.cue.set(c.clamp(0.0, 1.5));
    }
    if let Some(x) = crossfader {
        app.set_crossfader(x);
        app.emit_perform();
    }
    Ok(())
}

// ---------------------------------------------------------------- performance

pub fn perform_open(app: &Arc<App>, setlist_id: Option<Id>) -> Res<PerformState> {
    app.open_setlist(setlist_id).map_err(ae)?;
    Ok(app.perform_state())
}

pub fn perform_state(app: &Arc<App>) -> Res<PerformState> {
    Ok(app.perform_state())
}

pub fn perform_play_next(app: &Arc<App>) -> Res<String> {
    let a = app.clone();
    blocking(move || a.play_next())
}

pub fn perform_play_entry(app: &Arc<App>, entry_id: Id) -> Res<String> {
    let a = app.clone();
    blocking(move || a.play_entry(entry_id))
}

pub fn perform_skip_next(app: &Arc<App>) -> Res<()> {
    app.skip_next().map_err(ae)
}

pub fn perform_set_auto_advance(app: &Arc<App>, value: Option<bool>) -> Res<()> {
    app.perform.lock().auto_advance_override = value;
    app.emit_perform();
    Ok(())
}

pub fn perform_cancel_pending(app: &Arc<App>) -> Res<()> {
    {
        let mut p = app.perform.lock();
        p.pending = None;
        p.armed_until = None;
    }
    for d in &app.decks {
        d.cancel_fade();
    }
    app.emit_perform();
    Ok(())
}

pub fn projector_message(app: &Arc<App>, message: Option<String>) -> Res<()> {
    app.perform.lock().message = message.filter(|m| !m.trim().is_empty());
    app.emit_perform();
    Ok(())
}

pub fn projector_state(app: &Arc<App>) -> Res<ProjectorPayload> {
    Ok(app.projector_payload())
}








// ---------------------------------------------------------------- Spotify

pub fn spotify_status(app: &Arc<App>) -> Res<SpotifyStatus> {
    Ok(app.spotify.status())
}

pub fn spotify_login(app: &Arc<App>) -> Res<SpotifyStatus> {
    let a = app.clone();
    crate::app::emit(&app.handle, "spotify-status", SpotifyStatus { connecting: true, ..app.spotify.status() });
    let r = blocking(move || {
        a.spotify.login()?;
        Ok(a.spotify.status())
    });
    crate::app::emit(&app.handle, "spotify-status", app.spotify.status());
    r
}

/// Reconnects to Spotify and renews its tokens (e.g. right before a set).
pub fn spotify_refresh(app: &Arc<App>) -> Res<SpotifyStatus> {
    let a = app.clone();
    let r = blocking(move || {
        a.spotify.refresh()?;
        Ok(a.spotify.status())
    });
    crate::app::emit(&app.handle, "spotify-status", app.spotify.status());
    r
}

/// Sign-in with your own Spotify app, for search and playlists.
pub fn spotify_web_login(app: &Arc<App>) -> Res<SpotifyStatus> {
    let a = app.clone();
    let r = blocking(move || {
        a.spotify.web_login()?;
        Ok(a.spotify.status())
    });
    crate::app::emit(&app.handle, "spotify-status", app.spotify.status());
    r
}

pub fn spotify_connect_cached(app: &Arc<App>) -> Res<SpotifyStatus> {
    let a = app.clone();
    let r = blocking(move || {
        a.spotify.connect_cached()?;
        Ok(a.spotify.status())
    });
    crate::app::emit(&app.handle, "spotify-status", app.spotify.status());
    r
}

pub fn spotify_logout(app: &Arc<App>) -> Res<()> {
    app.spotify.logout();
    crate::app::emit(&app.handle, "spotify-status", app.spotify.status());
    Ok(())
}

pub fn spotify_search(app: &Arc<App>, query: String) -> Res<Vec<SpotifyTrack>> {
    let a = app.clone();
    blocking(move || a.spotify.search(&query))
}

pub fn spotify_playlists(app: &Arc<App>) -> Res<Vec<SpotifyPlaylist>> {
    let a = app.clone();
    blocking(move || a.spotify.my_playlists())
}

pub fn spotify_playlist_tracks(app: &Arc<App>, uri: String) -> Res<Vec<SpotifyTrack>> {
    let a = app.clone();
    blocking(move || a.spotify.playlist_tracks(&uri))
}

pub fn spotify_resolve(app: &Arc<App>, input: String) -> Res<Vec<SpotifyTrack>> {
    let a = app.clone();
    blocking(move || a.spotify.resolve(&input))
}

// ---------------------------------------------------------------- MIDI

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MidiInfo {
    pub ports: Vec<PortInfo>,
    pub mappings: Vec<Mapping>,
    /// The mapping in use (in automatic mode: the one matching the controller).
    pub active_mapping: String,
    pub actions: Vec<(String, String, String)>,
    pub led_states: Vec<String>,
}

pub fn midi_info(app: &Arc<App>) -> Res<MidiInfo> {
    let a = app.clone();
    blocking(move || {
        let s = a.settings.read().clone();
        Ok(MidiInfo {
            ports: a.midi.get().map(|m| m.ports()).unwrap_or_default(),
            mappings: a.all_mappings_with(&s),
            active_mapping: a.midi.get().map(|m| m.active_mapping()).unwrap_or_default(),
            actions: crate::midi::actions(),
            led_states: crate::midi::led_states(),
        })
    })
}

pub fn midi_learn(app: &Arc<App>) -> Res<Option<LearnedMessage>> {
    let a = app.clone();
    blocking(move || Ok(a.midi.get().and_then(|m| m.learn(Duration::from_secs(15)))))
}

pub fn midi_learn_cancel(app: &Arc<App>) -> Res<()> {
    if let Some(m) = app.midi.get() {
        m.cancel_learn();
    }
    Ok(())
}

/// The mapping to change: a custom one as it is, a built-in one as a custom
/// copy (which then becomes the mapping in use).
fn editable_mapping(app: &Arc<App>, settings: &Settings, id: &str) -> Res<Mapping> {
    let m = app.all_mappings_with(settings).into_iter().find(|m| m.id == id).ok_or("No such mapping")?;
    if !m.builtin {
        return Ok(m);
    }
    let stamp = crate::model::now_ms();
    Ok(Mapping { id: format!("{}-custom-{stamp:x}", m.id), name: format!("{} (custom)", m.name), builtin: false, ..m })
}

fn save_mapping(app: &Arc<App>, mut settings: Settings, m: Mapping) -> Res<String> {
    let id = m.id.clone();
    settings.midi.custom_mappings.retain(|x| x.id != id);
    settings.midi.custom_mappings.push(m);
    settings.midi.mapping_id = id.clone();
    settings_set(app, settings)?;
    Ok(id)
}

/// Sets a control of a mapping (`index`: replace it; none: add it). Editing a
/// built-in mapping makes a custom copy. Returns the mapping's id.
pub fn midi_control_set(app: &Arc<App>, mapping_id: String, index: Option<usize>, control: crate::midi::Control) -> Res<String> {
    let settings = app.settings.read().clone();
    let mut m = editable_mapping(app, &settings, &mapping_id)?;
    match index {
        Some(i) if i < m.controls.len() => m.controls[i] = control,
        _ => m.controls.push(control),
    }
    save_mapping(app, settings, m)
}

/// Removes a control from a mapping (a built-in one is copied first). Returns the mapping's id.
pub fn midi_control_remove(app: &Arc<App>, mapping_id: String, index: usize) -> Res<String> {
    let settings = app.settings.read().clone();
    let mut m = editable_mapping(app, &settings, &mapping_id)?;
    if index < m.controls.len() {
        m.controls.remove(index);
    }
    save_mapping(app, settings, m)
}

/// Deletes one of your mappings (back to automatic).
pub fn midi_mapping_delete(app: &Arc<App>, mapping_id: String) -> Res<()> {
    let mut settings = app.settings.read().clone();
    settings.midi.custom_mappings.retain(|m| m.id != mapping_id);
    if settings.midi.mapping_id == mapping_id {
        settings.midi.mapping_id = String::new();
    }
    settings_set(app, settings)
}

/// Lets the UI trigger the same actions as a controller (used by keyboard shortcuts).
pub fn midi_action(app: &Arc<App>, action: String, value: Option<f64>, delta: Option<i32>) -> Res<()> {
    let v = match (value, delta) {
        (Some(v), _) => crate::midi::Value::Abs(v),
        (_, Some(d)) => crate::midi::Value::Rel(d),
        _ => crate::midi::Value::Press,
    };
    let a = app.clone();
    blocking(move || {
        a.on_midi(&action, v);
        Ok(())
    })
}


/// Where songs go in the set.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SetPosition {
    /// After what's playing (and what's been played).
    Next,
    End,
    At(usize),
}

/// Adds songs to the open set (opening a new live set if none is): after
/// what's playing, at the end, or at a position. While filtering on a style
/// (`style`), a song added is danced as that style. With one song and a style
/// slot selected (`slot`), it fills the slot instead. `setlist_id`: add to
/// that set instead (e.g. songs dropped on it in the sidebar).
pub fn set_add(app: &Arc<App>, version_ids: Vec<Id>, position: SetPosition, style: Option<String>, slot: Option<Id>, setlist_id: Option<Id>) -> Res<()> {
    if let Some(id) = setlist_id {
        app.store.setlist(&id).map_err(ae)?;
        let entries = version_ids.into_iter().map(|v| NewEntry { version_id: Some(v), ..Default::default() }).collect();
        let at = match position {
            SetPosition::At(i) => Some(i),
            _ => None,
        };
        return entries_insert(app, id, at, entries).map(|_| ());
    }
    let open = app.perform.lock().setlist_id.clone().and_then(|id| app.store.setlist(&id).ok());
    let set = match open {
        Some(s) => s,
        None => {
            let date = jiff::Zoned::now().strftime("%b %-d, %Y").to_string();
            let club = app.clubs().latest(&app.settings.read().default_club).map(|v| ClubRef { id: app.settings.read().default_club.clone(), version: v });
            let id = app.store.setlist_create(&format!("Live set — {date}"), "live", club, None).map_err(ae)?;
            app.open_setlist(Some(id.clone())).map_err(ae)?;
            crate::app::emit(&app.handle, "setlists-changed", ());
            app.store.setlist(&id).map_err(ae)?
        }
    };
    if let (Some(slot), [one], false) = (&slot, version_ids.as_slice(), matches!(position, SetPosition::End)) {
        if set.entries.iter().any(|e| &e.id == slot && e.version_id.is_none()) {
            entry_update(app, slot.clone(), serde_json::json!({ "versionId": one }))?;
            return Ok(());
        }
    }
    let at = match position {
        SetPosition::Next => Some(app.insert_after(&set.entries.iter().collect::<Vec<_>>()).map_or(0, |i| i + 1)),
        SetPosition::End => None,
        SetPosition::At(i) => Some(i),
    };
    let entries = version_ids
        .iter()
        .map(|id| {
            let styles = app.store.version(id).map(|(v, _)| v.styles).unwrap_or_default();
            // An unlabelled song, or one danced as several styles, is danced as the filtered one.
            let pick = style.as_ref().filter(|f| styles.is_empty() || (styles.len() > 1 && styles.contains(f)));
            NewEntry { version_id: Some(id.clone()), styles: pick.map(|f| vec![f.clone()]), ..Default::default() }
        })
        .collect();
    entries_insert(app, set.id, at, entries).map(|_| ())
}

/// Fits set entries' tempos to their styles (`views::entry_fit_tempo`): each
/// one playing outside its style's range gets the nearest tempo inside it;
/// those that fit are left as they are. Returns how many changed.
pub fn entries_fit_tempo(app: &Arc<App>, ids: Vec<Id>) -> Res<usize> {
    let mut changed = 0;
    let mut sets = std::collections::HashSet::new();
    for id in ids {
        let (e, sid) = app.store.entry(&id).map_err(ae)?;
        let Some((v, tid)) = e.version_id.as_ref().and_then(|v| app.store.version(v).ok()) else { continue };
        let track = app.store.track(&tid).map_err(ae)?;
        let set = app.store.setlist(&sid).map_err(ae)?;
        let ctx = app.view_context(set.club.clone());
        if let Some(bpm) = crate::views::entry_fit_tempo(&ctx, &track, &v, &e) {
            let overrides = Overrides { target_bpm: Some(bpm), ..e.overrides.clone() };
            app.store.entry_update(&id, &EntryPatch { overrides: Some(overrides), ..Default::default() }).map_err(ae)?;
            changed += 1;
            sets.insert(sid);
        }
    }
    for sid in &sets {
        app.emit_setlist(sid);
    }
    if changed > 0 {
        app.emit_perform();
    }
    Ok(changed)
}

/// Moves entries after the song the room hears (or heard last), else after
/// the last one played, else to the start (`App::insert_after`). Played or
/// skipped entries moved are to play again.
pub fn entries_move_next(app: &Arc<App>, setlist_id: Id, ids: Vec<Id>) -> Res<()> {
    let set = app.store.setlist(&setlist_id).map_err(ae)?;
    let rest: Vec<&SetlistEntry> = set.entries.iter().filter(|e| !ids.contains(&e.id)).collect();
    let current = app.current_entry_id();
    let at = app.insert_after(&rest).map_or(0, |i| i + 1);
    let moving: Vec<&SetlistEntry> = set.entries.iter().filter(|e| ids.contains(&e.id)).collect();
    for e in moving.iter().filter(|e| e.status != "pending" && Some(&e.id) != current.as_ref()) {
        app.store.entry_update(&e.id, &EntryPatch { status: Some("pending".into()), ..Default::default() }).map_err(ae)?;
    }
    let mut order: Vec<Id> = rest.iter().map(|e| e.id.clone()).collect();
    order.splice(at..at, moving.iter().map(|e| e.id.clone()));
    entries_reorder(app, setlist_id, order)
}

/// Writes a set as a CSV file (number, style, title, artist, version, BPM, length, tags, note, status).
pub fn setlist_export_csv(app: &Arc<App>, id: Id, path: String) -> Res<()> {
    let view = set_view(app, id)?;
    let esc = |v: &str| format!("\"{}\"", v.replace('"', "\"\""));
    let mut out = String::new();
    let head = ["#", "Style", "Title", "Artist", "Version", "BPM", "Length", "Tags", "Note", "Status"];
    out.push_str(&head.iter().map(|h| esc(h)).collect::<Vec<_>>().join(","));
    out.push('\n');
    for r in &view.rows {
        let style = if r.label.is_empty() { r.styles.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(" / ") } else { r.label.clone() };
        let secs = (r.duration_ms / 1000.0).round() as i64;
        let cells = [
            r.number.map(|n| n.to_string()).unwrap_or_default(),
            style,
            if r.track_id.is_some() || r.is_pause { r.title.clone() } else { "(slot)".into() },
            r.artist.clone(),
            r.version_label.clone(),
            r.bpm.map(crate::matching::num).unwrap_or_default(),
            if secs > 0 { format!("{}:{:02}", secs / 60, secs % 60) } else { String::new() },
            r.tags.iter().map(|t| t.name.as_str()).collect::<Vec<_>>().join("; "),
            r.entry.note.clone(),
            r.entry.status.clone(),
        ];
        out.push_str(&cells.iter().map(|c| esc(c)).collect::<Vec<_>>().join(","));
        out.push('\n');
    }
    crate::store::write_atomic(std::path::Path::new(&path), out.as_bytes()).map_err(ae)
}

/// A setlist as a printable page (this computer's `print/` folder); returns
/// its path. It opens the print dialog when shown in a browser.
pub fn setlist_print(app: &Arc<App>, id: Id) -> Res<String> {
    let view = set_view(app, id)?;
    let h = |v: &str| v.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
    let name = view.setlist.name.clone();
    let mut rows = String::new();
    for r in &view.rows {
        let style = if r.label.is_empty() { r.styles.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(" / ") } else { r.label.clone() };
        let secs = (r.duration_ms / 1000.0).round() as i64;
        let title = if r.track_id.is_some() || r.is_pause { r.title.clone() } else { "(slot)".into() };
        let cells = [
            r.number.map(|n| n.to_string()).unwrap_or_default(),
            style,
            title,
            r.artist.clone(),
            r.bpm.map(crate::matching::num).unwrap_or_default(),
            if secs > 0 { format!("{}:{:02}", secs / 60, secs % 60) } else { String::new() },
            r.entry.note.clone(),
        ];
        let class = if r.entry.status == "skipped" { " class=\"skipped\"" } else { "" };
        rows.push_str(&format!("<tr{class}>{}</tr>\n", cells.iter().map(|c| format!("<td>{}</td>", h(c))).collect::<String>()));
    }
    let club = view.club.as_ref().map(|c| format!("<p>{}</p>", h(&c.name))).unwrap_or_default();
    let page = format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>{t}</title><style>\
         body {{ font: 11pt sans-serif; margin: 1.5cm; }} h1 {{ font-size: 16pt; margin: 0; }} p {{ margin: 0 0 1em; color: #555; }}\
         table {{ border-collapse: collapse; width: 100%; }} th, td {{ text-align: left; padding: 3px 6px; border-bottom: 1px solid #ccc; }}\
         th {{ font-size: 9pt; color: #555; }} tr.skipped td {{ text-decoration: line-through; color: #888; }}\
         </style></head><body onload=\"window.print()\"><h1>{t}</h1>{club}<table>\
         <tr><th>#</th><th>Style</th><th>Title</th><th>Artist</th><th>BPM</th><th>Length</th><th>Note</th></tr>\n{rows}</table></body></html>\n",
        t = h(&name)
    );
    let dir = app.local_dir.join("print");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file: String = name.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
    let path = dir.join(format!("{}.html", file.trim()));
    crate::store::write_atomic(&path, page.as_bytes()).map_err(ae)?;
    Ok(path.to_string_lossy().to_string())
}

// ---------------------------------------------------------------- views

/// History rows a library view lists at most.
const HISTORY_ROWS: usize = 300;

/// A library view's rows, style chips and sort, ready to show.
pub fn library_view(app: &Arc<App>, query: crate::views::LibraryQuery) -> Res<crate::views::LibraryView> {
    let ctx = app.view_context(None);
    let tracks = app.store.all_tracks();
    let crates = app.store.crates();
    let setlist = match &query.view {
        crate::views::View::Setlist { id } => app.store.setlist(id).ok(),
        _ => None,
    };
    let open_set = app.perform.lock().setlist_id.clone().and_then(|id| app.store.setlist(&id).ok());
    let history = if query.view == crate::views::View::History { app.store.history(HISTORY_ROWS) } else { vec![] };
    let data = crate::views::LibraryData {
        tracks: &tracks,
        crates: &crates,
        setlist: setlist.as_ref(),
        open_set: open_set.as_ref(),
        history: &history,
        now: crate::model::now_ms(),
    };
    Ok(crate::views::library_view(&ctx, &data, &query))
}

/// A set's entries (labels, styles, warnings, lengths) and its check against the club.
pub fn set_view(app: &Arc<App>, id: Id) -> Res<crate::views::SetView> {
    let set = app.store.setlist(&id).map_err(ae)?;
    let ctx = app.view_context(set.club.clone());
    let tracks = app.store.tracks_of_versions(set.entries.iter().filter_map(|e| e.version_id.as_deref()));
    let mut view = crate::views::set_view(&ctx, &set, &tracks, app.gap_seconds_for(Some(&set)));
    view.club = set.club.as_ref().and_then(|r| club_summary_of(app, r));
    view.default_pause_secs = app.gap_seconds_for(Some(&Setlist { pause_secs: None, ..set.clone() }));
    Ok(view)
}

fn club_summary_of(app: &Arc<App>, r: &ClubRef) -> Option<crate::views::ClubSummary> {
    let club = app.club(r)?;
    let info = app.clubs().list().into_iter().find(|c| c.id == r.id);
    Some(crate::views::club_summary(&club, r, info.as_ref()))
}

/// The club in effect: the open set's, else your default club's latest.
pub fn active_club(app: &Arc<App>) -> Res<Option<crate::views::ClubSummary>> {
    Ok(app.active_club_ref().and_then(|r| club_summary_of(app, &r)))
}

/// Songs, missing ones, the club's songs and each crate's size.
pub fn library_summary(app: &Arc<App>) -> Res<crate::views::LibrarySummary> {
    let ctx = app.view_context(None);
    Ok(crate::views::library_summary(&ctx, &app.store.all_tracks(), &app.store.crates(), crate::model::now_ms()))
}

/// A song as the Info panel shows it (and, with `entry_id`, that set entry).
pub fn song_view(app: &Arc<App>, track_id: Option<Id>, entry_id: Option<Id>) -> Res<Option<crate::views::SongView>> {
    let entry = entry_id.and_then(|id| app.store.entry(&id).ok());
    let tid = match (&entry, track_id) {
        (Some((e, _)), _) if e.version_id.is_some() => app.store.version(e.version_id.as_deref().unwrap_or_default()).ok().map(|(_, t)| t),
        (_, t) => t,
    };
    let set = entry.as_ref().and_then(|(_, sid)| app.store.setlist(sid).ok());
    let ctx = app.view_context(set.as_ref().and_then(|s| s.club.clone()));
    let ready = stems_ready(app)?;
    let is_ready = |r: &Recording| ready.contains(&r.rec.id);
    let announce = app.settings.read().announce.clone();
    let sc = crate::views::SongContext { stems_ready: &is_ready, gap_secs: app.gap_seconds_for(set.as_ref()), announce: &announce, set: set.as_ref() };
    let all = app.store.all_tracks();
    let Some(track) = tid.and_then(|t| all.iter().find(|x| x.rec.id == t).cloned()) else {
        // A slot or a pause: only the entry.
        return Ok(entry.map(|(e, _)| {
            let empty = Track { rec: Default::default(), recordings: vec![], missing: false, play_count: 0, last_played_at: None };
            let mut v = crate::views::song_view(&ctx, &empty, Some(&e), &all, &sc);
            v.track.rec.id = String::new();
            v
        }));
    };
    Ok(Some(crate::views::song_view(&ctx, &track, entry.as_ref().map(|(e, _)| e), &all, &sc)))
}

/// Plays another version of the song in a set entry (e.g. another edit of a
/// line dance). Settings tied to the old audio (tempo, start/end, fade) are
/// dropped, and the club's tempo rule applies afresh.
pub fn entry_switch_version(app: &Arc<App>, entry_id: Id, version_id: Id) -> Res<SetlistEntry> {
    let (e, _) = app.store.entry(&entry_id).map_err(ae)?;
    let keep = Overrides { gain_db: e.overrides.gain_db, stems: e.overrides.stems.clone(), ..Default::default() };
    let overrides = serde_json::to_value(keep).map_err(err)?;
    entry_update(app, entry_id, serde_json::json!({ "versionId": version_id, "overrides": overrides }))
}

/// Sets a version's tempo to the nearest edge of its style's sweet range (or
/// back to its own, if it's there already).
pub fn version_fit_tempo(app: &Arc<App>, id: Id) -> Res<Version> {
    let (v, tid) = app.store.version(&id).map_err(ae)?;
    let track = app.store.track(&tid).map_err(ae)?;
    let ctx = app.view_context(None);
    let target = crate::views::fit_tempo(&ctx, &track, &v).ok_or("This version has no style with a tempo to fit")?;
    version_update(app, id, serde_json::json!({ "targetBpm": target }))
}

// ---------------------------------------------------------------- styles & tags settings

/// A style or tag key songs carry that isn't in your list, and how many versions carry it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyUse {
    pub key: String,
    pub name: String,
    pub count: usize,
}

/// Settings → Styles & Tags: your styles and tags, keys songs carry that
/// aren't in your lists, and (for one club) what its styles are called.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StylesPage {
    #[serde(flatten)]
    pub info: StylesInfo,
    pub undefined_styles: Vec<KeyUse>,
    pub undefined_tags: Vec<KeyUse>,
    /// The club shown, its styles, your rules for it, and the pairs that need no rule (same names).
    pub club: Option<crate::views::Choice>,
    pub club_styles: Vec<crate::views::StyleDef>,
    pub rules: Vec<ClubRule>,
    pub same_named: Vec<ClubRule>,
    /// The club's styles not among yours (by key or name).
    pub importable: Vec<crate::views::StyleDef>,
}

pub fn styles_page(app: &Arc<App>, club_id: Option<String>) -> Res<StylesPage> {
    let info = styles_get(app)?;
    let lib = &info.library;
    let undefined = |usage: &std::collections::BTreeMap<String, usize>, known: &dyn Fn(&str) -> bool| -> Vec<KeyUse> {
        usage
            .iter()
            .filter(|(k, _)| !known(k))
            .map(|(k, n)| KeyUse { key: k.clone(), name: crate::views::prettify(k), count: *n })
            .collect()
    };
    let undefined_styles = undefined(&info.usage, &|k| lib.styles.iter().any(|s| s.key == k));
    let undefined_tags = undefined(&info.tag_usage, &|k| lib.tags.iter().any(|t| t.key == k));
    let clubs = app.clubs().list();
    let ci = club_id.as_deref().and_then(|id| clubs.iter().find(|c| c.id == id)).or(clubs.first());
    let club = ci.and_then(|c| app.club(&ClubRef { id: c.id.clone(), version: c.versions.first()?.id.clone() }));
    let club_styles: Vec<crate::views::StyleDef> = club.as_ref().map(|c| c.styles.iter().map(|(id, s)| crate::views::style_def(id, s)).collect()).unwrap_or_default();
    let same_named = lib
        .styles
        .iter()
        .flat_map(|m| club_styles.iter().filter(|c| crate::views::same_name(&c.name, &m.name)).map(|c| ClubRule { mine: m.key.clone(), club: c.key.clone() }))
        .collect();
    let importable = club_styles.iter().filter(|c| !lib.styles.iter().any(|s| s.key == c.key || crate::views::same_name(&s.name, &c.name))).cloned().collect();
    Ok(StylesPage {
        undefined_styles,
        undefined_tags,
        club: ci.map(|c| crate::views::Choice { id: c.id.clone(), label: c.name.clone() }),
        rules: ci.and_then(|c| lib.club_rules.get(&c.id)).cloned().unwrap_or_default(),
        same_named,
        importable,
        club_styles,
        info,
    })
}

/// How announcements say words (saved with your styles).
pub fn pronunciations_save(app: &Arc<App>, list: Vec<Pronunciation>) -> Res<()> {
    edit_styles(app, |l| l.pronunciations = list)
}

/// A new, blank item in a club's draft guidelines (`kind`: style, group,
/// rule or tag) with an id of its own; returns the draft.
pub fn club_add_item(mut club: Club, kind: String) -> Res<Club> {
    fn free(taken: &[&String], base: &str) -> String {
        let mut id = base.to_string();
        let mut i = 2;
        while taken.iter().any(|t| *t == &id) {
            id = format!("{base}-{i}");
            i += 1;
        }
        id
    }
    match kind.as_str() {
        "style" => {
            let id = free(&club.styles.keys().collect::<Vec<_>>(), "new-style");
            club.styles.insert(id, club::Style { name: "New style".into(), ..Default::default() });
        }
        "group" => {
            let first = club.styles.keys().next().cloned().ok_or("Add a style first")?;
            let id = free(&club.groups.keys().collect::<Vec<_>>(), "group");
            club.groups.insert(id, club::Group { name: "New group".into(), styles: vec![first], ..Default::default() });
        }
        "rule" => {
            let id = free(&club.rules.keys().collect::<Vec<_>>(), "rule");
            let rule = club::Rule {
                name: "First songs".into(),
                songs: Some(club::Range { min: Some(1.0), max: Some(NEW_RULE_SONGS) }),
                length: Some(club::Range { min: None, max: Some(NEW_RULE_LENGTH_SECS) }),
                ..Default::default()
            };
            club.rules.insert(id, rule);
        }
        "tag" => {
            let id = free(&club.tags.keys().collect::<Vec<_>>(), "tag");
            club.tags.insert(id, club::Tag { name: "New tag".into(), announce: true, ..Default::default() });
        }
        other => return Err(format!("unknown kind: {other}")),
    }
    Ok(club)
}

/// A new rule for part of the set starts as "the first 10 songs at most 3:30".
const NEW_RULE_SONGS: f64 = 10.0;
const NEW_RULE_LENGTH_SECS: f64 = 210.0;

/// A club's suggested song as you have it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedHere {
    pub have: bool,
    /// The songs playing it ("Title — Artist", …).
    pub songs: String,
}

/// For each style with suggested songs: whether you have each (in order) and as which songs.
pub fn club_song_status(app: &Arc<App>, club_id: String, version_id: String) -> Res<HashMap<String, Vec<SuggestedHere>>> {
    let entries = club_song_entries(app, club_id, version_id)?;
    let tracks = app.store.all_tracks();
    let name = |title: &str, artist: &str| if artist.is_empty() { title.to_string() } else { format!("{title} — {artist}") };
    let label = |rec: &str| -> String {
        let songs: Vec<String> = tracks.iter().filter(|t| t.rec.versions.iter().any(|v| v.recording == rec)).map(|t| name(&t.rec.title, &t.rec.artist)).collect();
        if !songs.is_empty() {
            return songs.join(", ");
        }
        app.store.recording(rec).map(|r| format!("{} (hidden)", name(&r.rec.title, &r.rec.artist))).unwrap_or_else(|_| rec.to_string())
    };
    Ok(entries
        .into_iter()
        .map(|(style, songs)| {
            let here = songs.into_iter().map(|recs| SuggestedHere { have: !recs.is_empty(), songs: recs.iter().map(|r| label(r)).collect::<Vec<_>>().join("\n") }).collect();
            (style, here)
        })
        .collect())
}

// ---------------------------------------------------------------- combining

/// A field the songs (or audio) to combine differ in, with each one's value.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CombineField {
    pub key: String,
    pub label: String,
    /// `id`: the song or audio it's from; `text`: its value; `of`: which one that is.
    pub values: Vec<CombineValue>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CombineValue {
    pub id: Id,
    pub text: String,
    pub of: String,
}

/// What combining some songs (and their audio) would do.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CombinePreview {
    /// Their audio by fingerprint: "same", "different", "unknown" (not all analysed), or "single" (one audio).
    pub verdict: String,
    /// How much of it lines up (the least), and how much it differs there (the most).
    pub coverage: f64,
    pub error: f64,
    /// "FLAC file + Spotify" for each audio.
    pub sources: Vec<String>,
    /// Song fields that differ, and audio fields that differ (when combined as the same audio).
    pub song_fields: Vec<CombineField>,
    pub audio_fields: Vec<CombineField>,
}

fn source_text(r: &Recording) -> String {
    let ext = r.files.first().and_then(|p| std::path::Path::new(p).extension()).map(|e| e.to_string_lossy().to_uppercase());
    r.rec
        .sources
        .iter()
        .map(|s| if s.kind == "spotify" { "Spotify".to_string() } else { ext.as_ref().map_or("file".to_string(), |e| format!("{e} file")) })
        .collect::<Vec<_>>()
        .join(" + ")
}

fn differing(key: &str, label: &str, values: Vec<CombineValue>) -> Option<CombineField> {
    let first = values.first()?.text.clone();
    values.iter().any(|v| v.text != first).then(|| CombineField { key: key.into(), label: label.into(), values })
}

pub fn combine_preview(app: &Arc<App>, songs: Vec<Id>, recordings: Vec<Id>) -> Res<CombinePreview> {
    let tracks: Vec<Track> = songs.iter().filter_map(|id| app.store.track(id).ok()).collect();
    let recs: Vec<Recording> = recordings.iter().filter_map(|id| app.store.recording(id).ok()).collect();
    let (mut verdict, mut coverage, mut error) = ("single".to_string(), 0.0, 0.0);
    if recs.len() > 1 {
        let m = recordings_compare(app, recordings.clone())?;
        coverage = m.iter().map(|x| x.coverage).fold(f64::INFINITY, f64::min);
        error = m.iter().map(|x| x.error).fold(0.0, f64::max);
        verdict = if !m.iter().all(|x| x.known) { "unknown" } else if m.iter().all(|x| x.same) { "same" } else { "different" }.into();
    }
    let mut song_fields = vec![];
    if tracks.len() > 1 {
        let v = |f: &dyn Fn(&Track) -> String| tracks.iter().map(|t| CombineValue { id: t.rec.id.clone(), text: f(t), of: t.rec.title.clone() }).collect();
        song_fields.extend(differing("title", "Title", v(&|t| t.rec.title.clone())));
        song_fields.extend(differing("artist", "Artist", v(&|t| t.rec.artist.clone())));
        song_fields.extend(differing("album", "Album", v(&|t| t.rec.album.clone())));
        song_fields.extend(differing("genre", "Genre", v(&|t| t.rec.genre.clone())));
        song_fields.extend(differing("rating", "Rating", v(&|t| t.rec.rating.map(|r| "★".repeat(r.max(0) as usize)).unwrap_or_default())));
        song_fields.extend(differing("notes", "Notes", v(&|t| t.rec.notes.clone())));
    }
    let mut audio_fields = vec![];
    if recs.len() > 1 {
        let v = |f: &dyn Fn(&Recording) -> String| {
            recs.iter().map(|r| CombineValue { id: r.rec.id.clone(), text: f(r), of: format!("{} ({})", r.rec.title, source_text(r)) }).collect()
        };
        audio_fields.extend(differing(
            "bpm",
            "BPM",
            v(&|r| format!("{}{}", r.rec.bpm.map(crate::matching::num).unwrap_or_else(|| "–".into()), r.rec.bpm_source.as_ref().map(|s| format!(" ({s})")).unwrap_or_default())),
        ));
        audio_fields.extend(differing(
            "meter",
            "Meter",
            v(&|r| match r.rec.beats_per_bar {
                Some(b) => format!("{b} beats a bar{}", if r.rec.meter_source.as_deref() == Some("user") { " (set by you)" } else { "" }),
                None => "unknown".into(),
            }),
        ));
        audio_fields.extend(differing("gainDb", "Gain", v(&|r| format!("{} dB", r.rec.gain_db))));
        audio_fields.extend(differing(
            "cues",
            "Hot cues",
            v(&|r| {
                let c: Vec<String> = r.rec.hot_cues.iter().map(|c| c.slot.to_string()).collect();
                if c.is_empty() { "none".into() } else { c.join(", ") }
            }),
        ));
    }
    Ok(CombinePreview { verdict, coverage: if coverage.is_finite() { coverage } else { 0.0 }, error, sources: recs.iter().map(source_text).collect(), song_fields, audio_fields })
}

/// Combines songs into the first (their versions become its versions) and,
/// with `same_audio`, their audio into one. `picks`: for each differing field,
/// the song or audio whose value is kept (default: the first's).
pub fn combine(app: &Arc<App>, songs: Vec<Id>, recordings: Vec<Id>, same_audio: bool, picks: HashMap<String, Id>) -> Res<()> {
    if same_audio && recordings.len() > 1 {
        let recs: Vec<Recording> = recordings.iter().filter_map(|id| app.store.recording(id).ok()).collect();
        let from = |field: &str| picks.get(field).and_then(|id| recs.iter().find(|r| &r.rec.id == id)).or(recs.first());
        // Hot cues come from the kept audio: keep the chosen one's by keeping it.
        let kept = from("cues").map(|r| r.rec.id.clone()).ok_or("No audio to combine")?;
        let mut patch = serde_json::Map::new();
        if let Some(r) = picks.get("bpm").and(from("bpm")) {
            patch.insert("bpm".into(), serde_json::json!(r.rec.bpm));
            patch.insert("bpmSource".into(), serde_json::json!(r.rec.bpm_source.clone().unwrap_or_else(|| "user".into())));
        }
        if let Some(r) = picks.get("meter").and(from("meter")) {
            patch.insert("beatsPerBar".into(), serde_json::json!(if r.rec.meter_source.as_deref() == Some("user") { r.rec.beats_per_bar } else { None }));
        }
        if let Some(r) = picks.get("gainDb").and(from("gainDb")) {
            patch.insert("gainDb".into(), serde_json::json!(r.rec.gain_db));
        }
        recordings_merge(app, kept.clone(), recordings.iter().filter(|id| **id != kept).cloned().collect())?;
        if !patch.is_empty() {
            recording_update(app, kept, serde_json::Value::Object(patch))?;
        }
    }
    if songs.len() > 1 {
        let tracks: Vec<Track> = songs.iter().filter_map(|id| app.store.track(id).ok()).collect();
        let from = |field: &str| picks.get(field).and_then(|id| tracks.iter().find(|t| &t.rec.id == id));
        let mut patch = serde_json::Map::new();
        if let Some(t) = from("title") { patch.insert("title".into(), serde_json::json!(t.rec.title)); }
        if let Some(t) = from("artist") { patch.insert("artist".into(), serde_json::json!(t.rec.artist)); }
        if let Some(t) = from("album") { patch.insert("album".into(), serde_json::json!(t.rec.album)); }
        if let Some(t) = from("genre") { patch.insert("genre".into(), serde_json::json!(t.rec.genre)); }
        if let Some(t) = from("rating") { patch.insert("rating".into(), serde_json::json!(t.rec.rating)); }
        if let Some(t) = from("notes") { patch.insert("notes".into(), serde_json::json!(t.rec.notes)); }
        let keep = songs[0].clone();
        songs_combine(app, keep.clone(), songs[1..].to_vec())?;
        if !patch.is_empty() {
            track_update(app, keep, serde_json::Value::Object(patch))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- Files view

/// A file in the Files view, ready to show.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRow {
    #[serde(flatten)]
    pub file: LibraryFile,
    /// Its name, and its folder within its library folder ("" at the top; none: added from elsewhere).
    pub name: String,
    pub folder: Option<String>,
    /// Not in the library: removed, or (e.g. its audio now plays from Spotify) not added.
    pub out: bool,
    /// The songs playing it.
    pub song_titles: Vec<crate::views::Choice>,
    /// Its audio has another source ("file" or "spotify"), so removing this file keeps it playable.
    pub other_source: Option<String>,
}

/// A file here that sounds like missing audio songs still play.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconnectRow {
    #[serde(flatten)]
    pub found: Reconnect,
    pub found_title: String,
    pub song_title: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesView {
    pub files: Vec<FileRow>,
    pub matches: Vec<ReconnectRow>,
}

pub fn files_view(app: &Arc<App>) -> Res<FilesView> {
    let files = library_files(app)?;
    let tracks = app.store.all_tracks();
    let title = |id: &str| tracks.iter().find(|t| t.rec.id == id).map_or_else(|| id.to_string(), |t| t.rec.title.clone());
    let rows = files
        .into_iter()
        .map(|f| {
            let rel = match &f.root {
                Some(root) if f.path.starts_with(root.as_str()) => f.path[root.len()..].trim_start_matches(['/', '\\']).to_string(),
                _ => f.path.clone(),
            };
            let p = std::path::Path::new(&rel);
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| rel.clone());
            let folder = f.root.as_ref().map(|_| p.parent().map(|d| d.to_string_lossy().replace('\\', "/")).unwrap_or_default());
            let other_source = f.recording.as_ref().and_then(|r| app.store.recording(r).ok()).and_then(|r| {
                (r.rec.sources.len() > 1).then(|| if r.rec.sources.iter().any(|s| s.kind == "spotify") { "spotify".to_string() } else { "file".to_string() })
            });
            FileRow {
                name,
                folder,
                out: f.removed || f.recording.is_none(),
                song_titles: f.songs.iter().map(|id| crate::views::Choice { id: id.clone(), label: title(id) }).collect(),
                other_source,
                file: f,
            }
        })
        .collect();
    let rec_title = |id: &str| app.store.recording(id).map_or_else(|_| id.to_string(), |r| r.rec.title);
    let matches = app
        .store
        .reconnect_matches(None)
        .into_iter()
        .map(|m| {
            let song = app.store.recording(&m.missing).ok().and_then(|r| r.songs.first().cloned()).map(|s| title(&s)).unwrap_or_else(|| rec_title(&m.missing));
            ReconnectRow { found_title: rec_title(&m.found), song_title: song, found: m }
        })
        .collect();
    Ok(FilesView { files: rows, matches })
}

// ---------------------------------------------------------------- Spotify tracks

/// Makes Spotify tracks playable and returns each one's version to play (its
/// song's primary). `in_library: false`: only for sets and the decks (songs
/// already in the library stay there).
pub fn spotify_versions(app: &Arc<App>, tracks: Vec<SpotifyTrack>, in_library: bool) -> Res<Vec<Id>> {
    let songs = spotify_import(app, tracks, Some(in_library))?;
    Ok(songs
        .iter()
        .filter_map(|id| app.store.track(id).ok())
        .filter_map(|t| crate::views::primary_version(&t).map(|v| v.id.clone()))
        .collect())
}

/// Which of these Spotify tracks (by URI) are in your library.
pub fn spotify_in_library(app: &Arc<App>, uris: Vec<String>) -> Res<Vec<bool>> {
    Ok(uris
        .iter()
        .map(|uri| {
            app.store
                .recording_for_source(&SourceRef { kind: "spotify".into(), key: uri.clone() })
                .is_some_and(|rid| app.store.songs_for_recording(&rid).iter().any(|s| app.store.track(s).is_ok_and(|t| t.rec.in_library)))
        })
        .collect())
}

/// A planned set of a playlist's songs (only in the set, not the library), opened.
pub fn setlist_from_spotify(app: &Arc<App>, name: String, tracks: Vec<SpotifyTrack>) -> Res<usize> {
    let versions = spotify_versions(app, tracks, false)?;
    let club = app.settings.read().default_club.clone();
    let set = setlist_create(app, name, "preset".into(), Some(club), None)?;
    let n = versions.len();
    entries_insert(app, set.id.clone(), None, versions.into_iter().map(|v| NewEntry { version_id: Some(v), ..Default::default() }).collect())?;
    perform_open(app, Some(set.id))?;
    Ok(n)
}

/// A key for a new style or tag: from its name, not taken yet.
fn unique_key(name: &str, taken: &[&str]) -> String {
    let base = club::slug(name);
    let base = if base == "club" && !name.to_lowercase().contains("club") { "item".to_string() } else { base };
    let mut key = base.clone();
    let mut i = 2;
    while taken.contains(&key.as_str()) {
        key = format!("{base}-{i}");
        i += 1;
    }
    key
}

fn edit_styles(app: &Arc<App>, change: impl FnOnce(&mut StyleLibrary)) -> Res<()> {
    let mut lib = app.style_library();
    change(&mut lib);
    styles_save(app, lib)
}

/// Adds one of your styles (`key`: keep a key songs already carry). Returns its key.
pub fn style_create(app: &Arc<App>, name: String, short: Option<String>, color: Option<String>, key: Option<String>) -> Res<String> {
    let lib = app.style_library();
    let taken: Vec<&str> = lib.styles.iter().map(|s| s.key.as_str()).collect();
    let key = key.filter(|k| !taken.contains(&k.as_str())).unwrap_or_else(|| unique_key(&name, &taken));
    let style = LibraryStyle { key: key.clone(), name, short: short.unwrap_or_default(), color: color.unwrap_or_else(|| crate::views::NEUTRAL.into()), ..Default::default() };
    edit_styles(app, |l| l.styles.push(style))?;
    Ok(key)
}

/// Adds one of your tags (`key`: keep a key songs already carry). Returns its key.
pub fn tag_create(app: &Arc<App>, name: String, key: Option<String>) -> Res<String> {
    let lib = app.style_library();
    let taken: Vec<&str> = lib.tags.iter().map(|t| t.key.as_str()).collect();
    let key = key.filter(|k| !taken.contains(&k.as_str())).unwrap_or_else(|| unique_key(&name, &taken));
    let tag = LibraryTag { key: key.clone(), name, color: crate::views::NEUTRAL.into(), ..Default::default() };
    edit_styles(app, |l| l.tags.push(tag))?;
    Ok(key)
}

/// "What I call `mine`, the club may call `theirs`": adds the rule, or with `on: false` removes it.
pub fn club_rule_set(app: &Arc<App>, club_id: String, mine: String, theirs: String, on: bool) -> Res<()> {
    edit_styles(app, |l| {
        let list = l.club_rules.entry(club_id.clone()).or_default();
        list.retain(|r| !(r.mine == mine && r.club == theirs));
        if on {
            list.push(ClubRule { mine, club: theirs });
        }
        if list.is_empty() {
            l.club_rules.remove(&club_id);
        }
    })
}

/// Adds a club's styles you don't have (by key or name) to yours. Returns how many.
pub fn styles_import_club(app: &Arc<App>, club_id: String) -> Res<usize> {
    let page = styles_page(app, Some(club_id))?;
    let n = page.importable.len();
    for s in page.importable {
        style_create(app, s.name, Some(s.short), Some(s.color), Some(s.key))?;
    }
    Ok(n)
}

/// What smart crate rules can test (for the rule editor).
pub fn smart_fields() -> Res<Vec<crate::smart::FieldDef>> {
    Ok(crate::smart::fields())
}

/// A new condition for a field, with a value to start from.
pub fn smart_default_condition(field: String) -> Res<crate::smart::Condition> {
    Ok(crate::smart::default_condition(&field))
}

/// Songs some rules match now (for the editor): how many, and the first titles.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartPreview {
    pub count: usize,
    pub titles: Vec<String>,
}

/// Titles a smart crate's preview lists.
const SMART_PREVIEW_TITLES: usize = 5;

/// What `rules` match in the library now, leaving out crate `editing` (so a
/// smart crate can't test itself).
pub fn smart_preview(app: &Arc<App>, rules: crate::smart::Rules, editing: Option<Id>) -> Res<SmartPreview> {
    let ctx = app.view_context(None);
    let tracks = app.store.all_tracks();
    let crates: Vec<Crate> = app.store.crates().into_iter().filter(|c| Some(&c.id) != editing.as_ref()).collect();
    let candidates: Vec<crate::smart::Candidate> = tracks
        .iter()
        .filter(|t| t.rec.in_library)
        .flat_map(|t| t.rec.versions.iter().filter(|v| !v.hidden && !v.unlisted).map(move |v| crate::smart::Candidate { track: t, version: v }))
        .collect();
    let ev = crate::smart::Evaluator { ctx: &ctx, crates: &crates, library: &candidates, now: crate::model::now_ms() };
    let hits: Vec<&crate::smart::Candidate> = candidates.iter().filter(|c| ev.matches(&rules, c, 0)).collect();
    Ok(SmartPreview { count: hits.len(), titles: hits.iter().take(SMART_PREVIEW_TITLES).map(|c| c.track.rec.title.clone()).collect() })
}

/// Each condition of some rules in words.
pub fn smart_describe(app: &Arc<App>, rules: crate::smart::Rules) -> Res<Vec<String>> {
    let (styles, crates) = (app.style_library(), app.store.crates());
    Ok(rules.conditions.iter().map(|c| crate::smart::describe(c, &styles, &crates)).collect())
}

// ---------------------------------------------------------------- dispatch

fn parse<T: serde::de::DeserializeOwned>(args: serde_json::Value) -> Res<T> {
    let args = if args.is_null() { serde_json::json!({}) } else { args };
    serde_json::from_value(args).map_err(|e| format!("bad arguments: {e}"))
}

fn json<T: Serialize>(r: Res<T>) -> Res<serde_json::Value> {
    r.and_then(|v| serde_json::to_value(v).map_err(err))
}

/// Runs a command by name with JSON arguments (an object with camelCase keys,
/// or null), returning its JSON result or an error message for the user.
pub fn call(app: &Arc<App>, name: &str, args: serde_json::Value) -> Res<serde_json::Value> {
    match name {
        "library_tracks" => json(library_tracks(app)),
        "track_get" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                id: Id,
            }
            let a: A = parse(args)?;
            json(track_get(app, a.id))
        }
        "track_update" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                id: Id,
                patch: serde_json::Value,
            }
            let a: A = parse(args)?;
            json(track_update(app, a.id, a.patch))
        }
        "tracks_delete" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                ids: Vec<Id>,
            }
            let a: A = parse(args)?;
            json(tracks_delete(app, a.ids))
        }
        "recording_update" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                id: Id,
                patch: serde_json::Value,
            }
            let a: A = parse(args)?;
            json(recording_update(app, a.id, a.patch))
        }
        "recording_reset_bpm" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                id: Id,
                from: String,
            }
            let a: A = parse(args)?;
            json(recording_reset_bpm(app, a.id, a.from))
        }
        "recording_waveform" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                id: Id,
            }
            let a: A = parse(args)?;
            json(recording_waveform(app, a.id))
        }
        "recording_artwork" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                id: Id,
            }
            let a: A = parse(args)?;
            json(recording_artwork(app, a.id))
        }
        "recordings_analyze" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                ids: Vec<Id>,
            }
            let a: A = parse(args)?;
            json(recordings_analyze(app, a.ids))
        }
        "song_from_recording" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                id: Id,
            }
            let a: A = parse(args)?;
            json(song_from_recording(app, a.id))
        }
        "recordings_compare" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                ids: Vec<Id>,
            }
            let a: A = parse(args)?;
            json(recordings_compare(app, a.ids))
        }
        "recordings_remove" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                ids: Vec<Id>,
            }
            let a: A = parse(args)?;
            json(recordings_remove(app, a.ids))
        }
        "audio_file_remove" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                hash: String,
            }
            let a: A = parse(args)?;
            json(audio_file_remove(app, a.hash))
        }
        "reconnect_matches" => json(reconnect_matches(app)),
        "reconnect" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                missing: Id,
                found: Id,
            }
            let a: A = parse(args)?;
            json(reconnect(app, a.missing, a.found))
        }
        "setlist_export" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                id: Id,
                path: String,
            }
            let a: A = parse(args)?;
            json(setlist_export(app, a.id, a.path))
        }
        "setlist_import" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                path: String,
            }
            let a: A = parse(args)?;
            json(setlist_import(app, a.path))
        }
        "crate_export" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                id: Id,
                path: String,
            }
            let a: A = parse(args)?;
            json(crate_export(app, a.id, a.path))
        }
        "crate_import" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                path: String,
            }
            let a: A = parse(args)?;
            json(crate_import(app, a.path))
        }
        "library_files" => json(library_files(app)),
        "library_add_files" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                paths: Vec<String>,
            }
            let a: A = parse(args)?;
            json(library_add_files(app, a.paths))
        }
        "library_rescan" => json(library_rescan(app)),
        "library_reload" => json(library_reload(app)),
        "library_info" => json(library_info(app)),
        "library_import" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                paths: Vec<String>,
            }
            let a: A = parse(args)?;
            json(library_import(app, a.paths))
        }
        "scan_progress" => json(scan_progress(app)),
        "history" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                limit: usize,
            }
            let a: A = parse(args)?;
            json(history(app, a.limit))
        }
        "spotify_import" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                tracks: Vec<SpotifyTrack>,
                in_library: Option<bool>,
            }
            let a: A = parse(args)?;
            json(spotify_import(app, a.tracks, a.in_library))
        }
        "track_set_in_library" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                id: Id,
                value: bool,
            }
            let a: A = parse(args)?;
            json(track_set_in_library(app, a.id, a.value))
        }
        "source_link_spotify" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                recording_id: Id,
                uri: String,
            }
            let a: A = parse(args)?;
            json(source_link_spotify(app, a.recording_id, a.uri))
        }
        "recordings_find_spotify" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                ids: Vec<Id>,
            }
            let a: A = parse(args)?;
            json(recordings_find_spotify(app, a.ids))
        }
        "source_remove" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                recording_id: Id,
                source: SourceRef,
            }
            let a: A = parse(args)?;
            json(source_remove(app, a.recording_id, a.source))
        }
        "undo_begin" => json(undo_begin(app)),
        "undo_end" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                id: u64,
            }
            let a: A = parse(args)?;
            json(undo_end(app, a.id))
        }
        "undo" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                id: u64,
            }
            let a: A = parse(args)?;
            json(undo(app, a.id))
        }
        "songs_combine" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                keep: Id,
                others: Vec<Id>,
            }
            let a: A = parse(args)?;
            json(songs_combine(app, a.keep, a.others))
        }
        "versions_separate" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                version_ids: Vec<Id>,
            }
            let a: A = parse(args)?;
            json(versions_separate(app, a.version_ids))
        }
        "recordings_merge" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                keep: Id,
                others: Vec<Id>,
            }
            let a: A = parse(args)?;
            json(recordings_merge(app, a.keep, a.others))
        }
        "recording_split_source" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                id: Id,
                source: SourceRef,
                versions: Vec<Id>,
                new_song: bool,
            }
            let a: A = parse(args)?;
            json(recording_split_source(app, a.id, a.source, a.versions, a.new_song))
        }
        "version_create" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                track_id: Id,
                version: serde_json::Value,
            }
            let a: A = parse(args)?;
            json(version_create(app, a.track_id, a.version))
        }
        "version_add_audio" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                track_id: Id,
                audio: AudioPick,
            }
            let a: A = parse(args)?;
            json(version_add_audio(app, a.track_id, a.audio))
        }
        "version_update" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                id: Id,
                patch: serde_json::Value,
            }
            let a: A = parse(args)?;
            json(version_update(app, a.id, a.patch))
        }
        "version_delete" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                id: Id,
            }
            let a: A = parse(args)?;
            json(version_delete(app, a.id))
        }
        "crates_list" => json(crates_list(app)),
        "crate_create" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                name: String,
                smart: Option<serde_json::Value>,
            }
            let a: A = parse(args)?;
            json(crate_create(app, a.name, a.smart))
        }
        "crate_rename" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                id: Id,
                name: String,
            }
            let a: A = parse(args)?;
            json(crate_rename(app, a.id, a.name))
        }
        "crate_delete" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                id: Id,
            }
            let a: A = parse(args)?;
            json(crate_delete(app, a.id))
        }
        "crate_add" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                id: Id,
                version_ids: Vec<Id>,
            }
            let a: A = parse(args)?;
            json(crate_add(app, a.id, a.version_ids))
        }
        "crate_set_smart" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                id: Id,
                smart: Option<serde_json::Value>,
            }
            let a: A = parse(args)?;
            json(crate_set_smart(app, a.id, a.smart))
        }
        "style_add" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                style: String,
                version_ids: Vec<Id>,
            }
            let a: A = parse(args)?;
            json(style_add(app, a.style, a.version_ids))
        }
        "style_remove" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                version_ids: Vec<Id>,
                style: Option<String>,
            }
            let a: A = parse(args)?;
            json(style_remove(app, a.version_ids, a.style))
        }
        "crate_remove" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                id: Id,
                version_ids: Vec<Id>,
            }
            let a: A = parse(args)?;
            json(crate_remove(app, a.id, a.version_ids))
        }
        "setlists_list" => json(setlists_list(app)),
        "setlist_get" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                id: Id,
            }
            let a: A = parse(args)?;
            json(setlist_get(app, a.id))
        }
        "setlist_create" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                name: String,
                mode: String,
                club_id: Option<String>,
                event_date: Option<String>,
            }
            let a: A = parse(args)?;
            json(setlist_create(app, a.name, a.mode, a.club_id, a.event_date))
        }
        "setlist_update" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                id: Id,
                patch: serde_json::Value,
            }
            let a: A = parse(args)?;
            json(setlist_update(app, a.id, a.patch))
        }
        "setlist_delete" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                id: Id,
            }
            let a: A = parse(args)?;
            json(setlist_delete(app, a.id))
        }
        "setlist_duplicate" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                id: Id,
                name: String,
            }
            let a: A = parse(args)?;
            json(setlist_duplicate(app, a.id, a.name))
        }
        "setlist_reset" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                id: Id,
            }
            let a: A = parse(args)?;
            json(setlist_reset(app, a.id))
        }
        "entries_insert" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                setlist_id: Id,
                position: Option<usize>,
                entries: Vec<NewEntry>,
            }
            let a: A = parse(args)?;
            json(entries_insert(app, a.setlist_id, a.position, a.entries))
        }
        "entry_update" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                id: Id,
                patch: serde_json::Value,
            }
            let a: A = parse(args)?;
            json(entry_update(app, a.id, a.patch))
        }
        "entries_delete" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                setlist_id: Id,
                ids: Vec<Id>,
            }
            let a: A = parse(args)?;
            json(entries_delete(app, a.setlist_id, a.ids))
        }
        "versions_reorder" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                track_id: Id,
                ids: Vec<Id>,
            }
            let a: A = parse(args)?;
            json(versions_reorder(app, a.track_id, a.ids))
        }
        "entries_reorder" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                setlist_id: Id,
                ids: Vec<Id>,
            }
            let a: A = parse(args)?;
            json(entries_reorder(app, a.setlist_id, a.ids))
        }
        "write_text_file" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                path: String,
                contents: String,
            }
            let a: A = parse(args)?;
            json(write_text_file(a.path, a.contents))
        }
        "read_text_file" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                path: String,
            }
            let a: A = parse(args)?;
            json(read_text_file(a.path))
        }
        "styles_get" => json(styles_get(app)),
        "styles_save" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                library: StyleLibrary,
            }
            let a: A = parse(args)?;
            json(styles_save(app, a.library))
        }
        "styles_rekey" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                from: String,
                to: Option<String>,
            }
            let a: A = parse(args)?;
            json(styles_rekey(app, a.from, a.to))
        }
        "tags_rekey" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                from: String,
                to: Option<String>,
            }
            let a: A = parse(args)?;
            json(tags_rekey(app, a.from, a.to))
        }
        "tag_set" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                tag: String,
                version_ids: Vec<Id>,
                on: bool,
            }
            let a: A = parse(args)?;
            json(tag_set(app, a.tag, a.version_ids, a.on))
        }
        "tts_status" => json(tts_status(app)),
        "tts_install" => json(tts_install(app)),
        "tts_voices" => json(tts_voices(app)),
        "tts_remove" => json(tts_remove(app)),
        "chime_preview" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                version_id: Id,
            }
            let a: A = parse(args)?;
            json(chime_preview(app, a.version_id))
        }
        "chime_labels" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                version_ids: Vec<Id>,
            }
            let a: A = parse(args)?;
            json(chime_labels(app, a.version_ids))
        }
        "tts_say" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                text: String,
            }
            let a: A = parse(args)?;
            json(tts_say(app, a.text))
        }
        "tts_phonemes" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                text: String,
            }
            let a: A = parse(args)?;
            json(tts_phonemes(app, a.text))
        }
        "announce_next" => json(announce_next(app)),
        "chime_next" => json(chime_next(app)),
        "clubs_list" => json(clubs_list(app)),
        "club_get" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                club_id: String,
                version_id: String,
            }
            let a: A = parse(args)?;
            json(club_get(app, a.club_id, a.version_id))
        }
        "club_parse" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                text: String,
            }
            let a: A = parse(args)?;
            json(club_parse(a.text))
        }
        "club_format" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                club: Club,
            }
            let a: A = parse(args)?;
            json(club_format(a.club))
        }
        "club_usage" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                club_id: String,
            }
            let a: A = parse(args)?;
            json(club_usage(app, a.club_id))
        }
        "club_import_file" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                path: String,
            }
            let a: A = parse(args)?;
            json(club_import_file(app, a.path))
        }
        "club_import_url" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                url: String,
            }
            let a: A = parse(args)?;
            json(club_import_url(app, a.url))
        }
        "club_update" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                club_id: String,
            }
            let a: A = parse(args)?;
            json(club_update(app, a.club_id))
        }
        "club_save" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                club_id: String,
                version_id: String,
                text: String,
            }
            let a: A = parse(args)?;
            json(club_save(app, a.club_id, a.version_id, a.text))
        }
        "club_examples" => json(club_examples()),
        "club_import_example" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                id: String,
            }
            let a: A = parse(args)?;
            json(club_import_example(app, a.id))
        }
        "club_song_matches" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                club_id: String,
                version_id: String,
            }
            let a: A = parse(args)?;
            json(club_song_matches(app, a.club_id, a.version_id))
        }
        "club_song_entries" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                club_id: String,
                version_id: String,
            }
            let a: A = parse(args)?;
            json(club_song_entries(app, a.club_id, a.version_id))
        }
        "club_song_for_version" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                version_id: Id,
            }
            let a: A = parse(args)?;
            json(club_song_for_version(app, a.version_id))
        }
        "club_song_download" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                url: String,
                hash: String,
            }
            let a: A = parse(args)?;
            json(club_song_download(app, a.url, a.hash))
        }
        "club_song_text" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                version_id: Id,
            }
            let a: A = parse(args)?;
            json(club_song_text(app, a.version_id))
        }
        "club_new" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                name: String,
            }
            let a: A = parse(args)?;
            json(club_new(app, a.name))
        }
        "club_rename_version" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                club_id: String,
                version_id: String,
                name: String,
            }
            let a: A = parse(args)?;
            json(club_rename_version(app, a.club_id, a.version_id, a.name))
        }
        "club_delete_version" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                club_id: String,
                version_id: String,
            }
            let a: A = parse(args)?;
            json(club_delete_version(app, a.club_id, a.version_id))
        }
        "club_delete" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                club_id: String,
            }
            let a: A = parse(args)?;
            json(club_delete(app, a.club_id))
        }
        "club_export" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                club_id: String,
                version_id: String,
                path: String,
            }
            let a: A = parse(args)?;
            json(club_export(app, a.club_id, a.version_id, a.path))
        }
        "settings_get" => json(settings_get(app)),
        "settings_set" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                settings: Settings,
            }
            let a: A = parse(args)?;
            json(settings_set(app, a.settings))
        }
        "audio_devices" => json(audio_devices()),
        "audio_status" => json(audio_status(app)),
        "audio_reopen" => json(audio_reopen(app)),
        "audio_test" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                deck: usize,
            }
            let a: A = parse(args)?;
            json(audio_test(app, a.deck))
        }
        "deck_states" => json(deck_states(app)),
        "deck_load" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                deck: Option<usize>,
                version_id: Id,
                entry_id: Option<Id>,
                play: bool,
            }
            let a: A = parse(args)?;
            json(deck_load(app, a.deck, a.version_id, a.entry_id, a.play))
        }
        "deck_action" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                deck: usize,
                action: String,
            }
            let a: A = parse(args)?;
            json(deck_action(app, a.deck, a.action))
        }
        "deck_cue" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                deck: usize,
                down: bool,
            }
            let a: A = parse(args)?;
            json(deck_cue(app, a.deck, a.down))
        }
        "deck_hotcue" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                deck: usize,
                slot: u8,
                action: String,
            }
            let a: A = parse(args)?;
            json(deck_hotcue(app, a.deck, a.slot, a.action))
        }
        "deck_jump" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                deck: usize,
                secs: f64,
            }
            let a: A = parse(args)?;
            json(deck_jump(app, a.deck, a.secs))
        }
        "deck_scrub" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                deck: usize,
                ms: f64,
                force: Option<bool>,
            }
            let a: A = parse(args)?;
            json(deck_scrub(app, a.deck, a.ms, a.force))
        }
        "deck_seek" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                deck: usize,
                ms: i64,
                force: Option<bool>,
            }
            let a: A = parse(args)?;
            json(deck_seek(app, a.deck, a.ms, a.force))
        }
        "deck_go_live" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                deck: usize,
            }
            let a: A = parse(args)?;
            json(deck_go_live(app, a.deck))
        }
        "deck_set_point" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                deck: usize,
                kind: String,
            }
            let a: A = parse(args)?;
            json(deck_set_point(app, a.deck, a.kind))
        }
        "deck_set" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                deck: usize,
                tempo: Option<f64>,
                trim_db: Option<f64>,
                pfl: Option<bool>,
                fade_at_ms: Option<i64>,
                clear_fade_at: Option<bool>,
            }
            let a: A = parse(args)?;
            json(deck_set(app, a.deck, a.tempo, a.trim_db, a.pfl, a.fade_at_ms, a.clear_fade_at))
        }
        "deck_stem" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                deck: usize,
                index: usize,
            }
            let a: A = parse(args)?;
            json(deck_stem(app, a.deck, a.index))
        }
        "stems_status" => json(stems_status(app)),
        "stems_download_model" => json(stems_download_model(app)),
        "stems_prepare" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                recording_ids: Vec<Id>,
            }
            let a: A = parse(args)?;
            json(stems_prepare(app, a.recording_ids))
        }
        "stems_delete" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                recording_id: Id,
            }
            let a: A = parse(args)?;
            json(stems_delete(app, a.recording_id))
        }
        "stems_delete_all" => json(stems_delete_all(app)),
        "stems_remove_model" => json(stems_remove_model(app)),
        "history_remove" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                entry_ids: Vec<Id>,
            }
            let a: A = parse(args)?;
            json(history_remove(app, a.entry_ids))
        }
        "spotify_cache_bytes" => json(spotify_cache_bytes(app)),
        "spotify_clear_cache" => json(spotify_clear_cache(app)),
        "stems_ready" => json(stems_ready(app)),
        "deck_release" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                deck: usize,
            }
            let a: A = parse(args)?;
            json(app.release_deck(a.deck.min(1)).map_err(ae))
        }
        "deck_tempo_reset" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                deck: usize,
            }
            let a: A = parse(args)?;
            json(deck_tempo_reset(app, a.deck))
        }
        "levels_set" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                master: Option<f32>,
                cue: Option<f32>,
                crossfader: Option<f64>,
            }
            let a: A = parse(args)?;
            json(levels_set(app, a.master, a.cue, a.crossfader))
        }
        "perform_open" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                setlist_id: Option<Id>,
            }
            let a: A = parse(args)?;
            json(perform_open(app, a.setlist_id))
        }
        "perform_state" => json(perform_state(app)),
        "perform_play_next" => json(perform_play_next(app)),
        "perform_play_entry" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                entry_id: Id,
            }
            let a: A = parse(args)?;
            json(perform_play_entry(app, a.entry_id))
        }
        "perform_skip_next" => json(perform_skip_next(app)),
        "perform_set_auto_advance" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                value: Option<bool>,
            }
            let a: A = parse(args)?;
            json(perform_set_auto_advance(app, a.value))
        }
        "perform_cancel_pending" => json(perform_cancel_pending(app)),
        "projector_message" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                message: Option<String>,
            }
            let a: A = parse(args)?;
            json(projector_message(app, a.message))
        }
        "projector_state" => json(projector_state(app)),
        "spotify_status" => json(spotify_status(app)),
        "spotify_login" => json(spotify_login(app)),
        "spotify_refresh" => json(spotify_refresh(app)),
        "spotify_web_login" => json(spotify_web_login(app)),
        "spotify_connect_cached" => json(spotify_connect_cached(app)),
        "spotify_logout" => json(spotify_logout(app)),
        "spotify_search" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                query: String,
            }
            let a: A = parse(args)?;
            json(spotify_search(app, a.query))
        }
        "spotify_playlists" => json(spotify_playlists(app)),
        "spotify_playlist_tracks" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                uri: String,
            }
            let a: A = parse(args)?;
            json(spotify_playlist_tracks(app, a.uri))
        }
        "spotify_resolve" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                input: String,
            }
            let a: A = parse(args)?;
            json(spotify_resolve(app, a.input))
        }
        "midi_info" => json(midi_info(app)),
        "midi_learn" => json(midi_learn(app)),
        "midi_learn_cancel" => json(midi_learn_cancel(app)),
        "midi_action" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                action: String,
                value: Option<f64>,
                delta: Option<i32>,
            }
            let a: A = parse(args)?;
            json(midi_action(app, a.action, a.value, a.delta))
        }
        "set_add" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                version_ids: Vec<Id>,
                position: SetPosition,
                style: Option<String>,
                slot: Option<Id>,
                #[serde(default)]
                setlist_id: Option<Id>,
            }
            let a: A = parse(args)?;
            json(set_add(app, a.version_ids, a.position, a.style, a.slot, a.setlist_id))
        }
        "entries_fit_tempo" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                ids: Vec<Id>,
            }
            let a: A = parse(args)?;
            json(entries_fit_tempo(app, a.ids))
        }
        "entries_move_next" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                setlist_id: Id,
                ids: Vec<Id>,
            }
            let a: A = parse(args)?;
            json(entries_move_next(app, a.setlist_id, a.ids))
        }
        "setlist_export_csv" => {
            #[derive(Deserialize)]
            struct A {
                id: Id,
                path: String,
            }
            let a: A = parse(args)?;
            json(setlist_export_csv(app, a.id, a.path))
        }
        "setlist_print" => {
            #[derive(Deserialize)]
            struct A {
                id: Id,
            }
            let a: A = parse(args)?;
            json(setlist_print(app, a.id))
        }
        "library_view" => {
            #[derive(Deserialize)]
            struct A {
                query: crate::views::LibraryQuery,
            }
            let a: A = parse(args)?;
            json(library_view(app, a.query))
        }
        "set_view" => {
            #[derive(Deserialize)]
            struct A {
                id: Id,
            }
            let a: A = parse(args)?;
            json(set_view(app, a.id))
        }
        "smart_fields" => json(smart_fields()),
        "smart_preview" => {
            #[derive(Deserialize)]
            struct A {
                rules: crate::smart::Rules,
                editing: Option<Id>,
            }
            let a: A = parse(args)?;
            json(smart_preview(app, a.rules, a.editing))
        }
        "active_club" => json(active_club(app)),
        "files_view" => json(files_view(app)),
        "spotify_versions" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                tracks: Vec<SpotifyTrack>,
                in_library: bool,
            }
            let a: A = parse(args)?;
            json(spotify_versions(app, a.tracks, a.in_library))
        }
        "spotify_in_library" => {
            #[derive(Deserialize)]
            struct A {
                uris: Vec<String>,
            }
            let a: A = parse(args)?;
            json(spotify_in_library(app, a.uris))
        }
        "setlist_from_spotify" => {
            #[derive(Deserialize)]
            struct A {
                name: String,
                tracks: Vec<SpotifyTrack>,
            }
            let a: A = parse(args)?;
            json(setlist_from_spotify(app, a.name, a.tracks))
        }
        "combine_preview" => {
            #[derive(Deserialize)]
            struct A {
                songs: Vec<Id>,
                recordings: Vec<Id>,
            }
            let a: A = parse(args)?;
            json(combine_preview(app, a.songs, a.recordings))
        }
        "combine" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                songs: Vec<Id>,
                recordings: Vec<Id>,
                same_audio: bool,
                #[serde(default)]
                picks: HashMap<String, Id>,
            }
            let a: A = parse(args)?;
            json(combine(app, a.songs, a.recordings, a.same_audio, a.picks))
        }
        "club_add_item" => {
            #[derive(Deserialize)]
            struct A {
                club: Club,
                kind: String,
            }
            let a: A = parse(args)?;
            json(club_add_item(a.club, a.kind))
        }
        "club_song_status" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                club_id: String,
                version_id: String,
            }
            let a: A = parse(args)?;
            json(club_song_status(app, a.club_id, a.version_id))
        }
        "pronunciations_save" => {
            #[derive(Deserialize)]
            struct A {
                list: Vec<Pronunciation>,
            }
            let a: A = parse(args)?;
            json(pronunciations_save(app, a.list))
        }
        "midi_control_set" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                mapping_id: String,
                index: Option<usize>,
                control: crate::midi::Control,
            }
            let a: A = parse(args)?;
            json(midi_control_set(app, a.mapping_id, a.index, a.control))
        }
        "midi_control_remove" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                mapping_id: String,
                index: usize,
            }
            let a: A = parse(args)?;
            json(midi_control_remove(app, a.mapping_id, a.index))
        }
        "midi_mapping_delete" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                mapping_id: String,
            }
            let a: A = parse(args)?;
            json(midi_mapping_delete(app, a.mapping_id))
        }
        "styles_page" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                club_id: Option<String>,
            }
            let a: A = parse(args)?;
            json(styles_page(app, a.club_id))
        }
        "style_create" => {
            #[derive(Deserialize)]
            struct A {
                name: String,
                short: Option<String>,
                color: Option<String>,
                key: Option<String>,
            }
            let a: A = parse(args)?;
            json(style_create(app, a.name, a.short, a.color, a.key))
        }
        "tag_create" => {
            #[derive(Deserialize)]
            struct A {
                name: String,
                key: Option<String>,
            }
            let a: A = parse(args)?;
            json(tag_create(app, a.name, a.key))
        }
        "club_rule_set" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                club_id: String,
                mine: String,
                theirs: String,
                on: bool,
            }
            let a: A = parse(args)?;
            json(club_rule_set(app, a.club_id, a.mine, a.theirs, a.on))
        }
        "styles_import_club" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                club_id: String,
            }
            let a: A = parse(args)?;
            json(styles_import_club(app, a.club_id))
        }
        "song_view" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                track_id: Option<Id>,
                entry_id: Option<Id>,
            }
            let a: A = parse(args)?;
            json(song_view(app, a.track_id, a.entry_id))
        }
        "entry_switch_version" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                entry_id: Id,
                version_id: Id,
            }
            let a: A = parse(args)?;
            json(entry_switch_version(app, a.entry_id, a.version_id))
        }
        "version_fit_tempo" => {
            #[derive(Deserialize)]
            struct A {
                id: Id,
            }
            let a: A = parse(args)?;
            json(version_fit_tempo(app, a.id))
        }
        "library_summary" => json(library_summary(app)),
        "smart_default_condition" => {
            #[derive(Deserialize)]
            struct A {
                field: String,
            }
            let a: A = parse(args)?;
            json(smart_default_condition(a.field))
        }
        "smart_describe" => {
            #[derive(Deserialize)]
            struct A {
                rules: crate::smart::Rules,
            }
            let a: A = parse(args)?;
            json(smart_describe(app, a.rules))
        }
        _ => Err(format!("unknown command: {name}")),
    }
}

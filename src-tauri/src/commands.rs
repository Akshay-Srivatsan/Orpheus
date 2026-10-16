//! Tauri commands (the UI's API). Slow work runs on its own thread so the UI
//! and the main thread never block.

use crate::app::{App, PerformState, ProjectorPayload};
use crate::club::{self, Club, ClubInfo, ClubVersion, Imported};
use std::collections::HashMap;
use crate::audio::output::OutputStatus;
use crate::audio::DeckSnapshot;
use crate::store::NewTrack;
use crate::midi::{LearnedMessage, Mapping, PortInfo};
use crate::model::*;
use crate::settings::Settings;
use crate::spotify::{SpotifyPlaylist, SpotifyStatus, SpotifyTrack};
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;
use tauri::{Manager, State, WebviewUrl, WebviewWindowBuilder};

type Res<T> = Result<T, String>;
type S<'a> = State<'a, Arc<App>>;

fn err<E: std::fmt::Display>(e: E) -> String {
    format!("{e:#}")
}

fn ae(e: anyhow::Error) -> String {
    format!("{e:#}")
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> anyhow::Result<T> + Send + 'static) -> Res<T> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.await.map_err(err)?.map_err(ae)
}

// ---------------------------------------------------------------- library

#[tauri::command]
pub async fn library_tracks(app: S<'_>) -> Res<Vec<Track>> {
    Ok(app.store.all_tracks())
}

#[tauri::command]
pub async fn track_get(app: S<'_>, id: Id) -> Res<Track> {
    app.store.track(&id).map_err(ae)
}

#[tauri::command]
pub async fn track_update(app: S<'_>, id: Id, patch: serde_json::Value) -> Res<Track> {
    let patch: TrackPatch = serde_json::from_value(patch).map_err(err)?;
    app.store.update_track(&id, &patch).map_err(ae)?;
    crate::app::emit(&app.handle, "library-changed", vec![id.clone()]);
    // The projector shows titles from songs.
    app.emit_perform();
    app.store.track(&id).map_err(ae)
}

#[tauri::command]
/// Takes songs out of the library; sets that play them keep them (hidden).
pub async fn tracks_delete(app: S<'_>, ids: Vec<Id>) -> Res<()> {
    let keep: Vec<Id> = app.decks.iter().filter_map(|d| d.track_id()).collect();
    app.store.remove_tracks(&ids, &keep).map_err(ae)?;
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    crate::app::emit(&app.handle, "setlists-changed", ());
    crate::app::emit(&app.handle, "crates-changed", ());
    Ok(())
}

/// Tells the UI the songs playing a recording changed.
fn emit_recording_changed(app: &S<'_>, rec_id: &str) {
    crate::app::emit(&app.handle, "library-changed", app.store.songs_for_recording(rec_id));
    crate::app::emit(&app.handle, "recordings-changed", ());
}

#[tauri::command]
pub async fn recording_update(app: S<'_>, id: Id, patch: serde_json::Value) -> Res<Recording> {
    let patch: RecordingPatch = serde_json::from_value(patch).map_err(err)?;
    app.store.update_recording(&id, &patch).map_err(ae)?;
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
#[tauri::command]
pub async fn recording_reset_bpm(app: S<'_>, id: Id, from: String) -> Res<Recording> {
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

#[tauri::command]
pub async fn recording_waveform(app: S<'_>, id: Id) -> Res<Vec<u8>> {
    Ok(app.store.waveform(&id).unwrap_or_default())
}

/// Cover art: a data URL of the local file's art, else the Spotify artwork URL.
#[tauri::command]
pub async fn recording_artwork(app: S<'_>, id: Id) -> Res<Option<String>> {
    use base64::Engine;
    let local = tauri::async_runtime::spawn_blocking({
        let store = app.store.clone();
        let id = id.clone();
        move || crate::library::resolve_path(&store, &id).and_then(|p| crate::library::artwork(&p))
    })
    .await
    .map_err(err)?;
    if let Some((mime, bytes)) = local {
        return Ok(Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))));
    }
    Ok(app.store.recording(&id).ok().and_then(|r| r.rec.artwork_url.clone()))
}

#[tauri::command]
pub async fn recordings_analyze(app: S<'_>, ids: Vec<Id>) -> Res<()> {
    for id in ids {
        app.queue_analysis(id, true);
    }
    Ok(())
}

/// A new song (in the library) for a recording, e.g. a hidden one.
#[tauri::command]
pub async fn song_from_recording(app: S<'_>, id: Id) -> Res<Track> {
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
#[tauri::command]
pub async fn recordings_compare(app: S<'_>, ids: Vec<Id>) -> Res<Vec<AudioMatch>> {
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
    .await
}

/// Removes recordings from the library; versions playing them stay, showing
/// as missing until it's added again (files stay on disk).
#[tauri::command]
pub async fn recordings_remove(app: S<'_>, ids: Vec<Id>) -> Res<()> {
    app.store.remove_recordings(&ids).map_err(ae)?;
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    crate::app::emit(&app.handle, "recordings-changed", ());
    crate::app::emit(&app.handle, "setlists-changed", ());
    crate::app::emit(&app.handle, "crates-changed", ());
    Ok(())
}

/// Takes one file out of the library (see `Store::remove_audio_file`).
#[tauri::command]
pub async fn audio_file_remove(app: S<'_>, hash: String) -> Res<()> {
    app.store.remove_audio_file(&hash).map_err(ae)?;
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    crate::app::emit(&app.handle, "recordings-changed", ());
    Ok(())
}

/// Recordings that sound the same as missing audio songs still play.
#[tauri::command]
pub async fn reconnect_matches(app: S<'_>) -> Res<Vec<Reconnect>> {
    let store = app.store.clone();
    blocking(move || Ok(store.reconnect_matches(None))).await
}

/// Lets `found` stand in for the missing recording `missing`.
#[tauri::command]
pub async fn reconnect(app: S<'_>, missing: Id, found: Id) -> Res<()> {
    app.store.reconnect(&missing, &found).map_err(ae)?;
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    crate::app::emit(&app.handle, "recordings-changed", ());
    app.emit_perform();
    Ok(())
}

/// Writes a set as a set file (`*.orpheus-set.toml`).
#[tauri::command]
pub async fn setlist_export(app: S<'_>, id: Id, path: String) -> Res<()> {
    // The club guidelines it was planned with travel with it.
    let club = app.store.setlist(&id).ok().and_then(|s| s.club).and_then(|c| app.club(&c));
    let rules = club.as_deref().map(|c| club::to_toml(&club::self_contained(c)));
    let text = crate::setfile::export(&app.store, &id, club.map(|c| c.name.clone()), rules).map_err(ae)?;
    crate::store::write_atomic(std::path::Path::new(&path), text.as_bytes()).map_err(ae)
}

/// Adds a set from a set file, linking its songs to the library's audio.
#[tauri::command]
pub async fn setlist_import(app: S<'_>, path: String) -> Res<crate::setfile::ImportReport> {
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
#[tauri::command]
pub async fn crate_export(app: S<'_>, id: Id, path: String) -> Res<()> {
    let text = crate::setfile::export_crate(&app.store, &id).map_err(ae)?;
    crate::store::write_atomic(std::path::Path::new(&path), text.as_bytes()).map_err(ae)
}

/// Adds a crate from a crate file, linking its songs to the library's audio.
#[tauri::command]
pub async fn crate_import(app: S<'_>, path: String) -> Res<crate::setfile::ImportReport> {
    let text = std::fs::read_to_string(&path).map_err(err)?;
    let report = crate::setfile::import_crate(&app.store, &text).map_err(ae)?;
    crate::app::emit(&app.handle, "crates-changed", ());
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    crate::app::emit(&app.handle, "recordings-changed", ());
    Ok(report)
}

/// The audio files in the library folders, with what each is in the library.
#[tauri::command]
pub async fn library_files(app: S<'_>) -> Res<Vec<LibraryFile>> {
    Ok(app.store.library_files())
}

/// Adds files to the library, including ones removed from it before.
#[tauri::command]
pub async fn library_add_files(app: S<'_>, paths: Vec<String>) -> Res<()> {
    for p in &paths {
        if let Some(f) = app.store.file_info(p) {
            app.store.include_file(&f.hash).map_err(ae)?;
        }
    }
    app.inner().import_paths(paths);
    Ok(())
}

#[tauri::command]
pub async fn library_rescan(app: S<'_>) -> Res<()> {
    app.inner().scan_library();
    Ok(())
}

/// Re-reads the library folder (e.g. after `git pull`).
#[tauri::command]
pub async fn library_reload(app: S<'_>) -> Res<Vec<String>> {
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

#[tauri::command]
pub async fn library_info(app: S<'_>) -> Res<LibraryInfo> {
    Ok(LibraryInfo {
        library_dir: app.store.library_dir().to_string_lossy().to_string(),
        data_dir: app.local_dir.to_string_lossy().to_string(),
        load_errors: app.store.load_errors(),
    })
}

#[tauri::command]
pub async fn library_import(app: S<'_>, paths: Vec<String>) -> Res<()> {
    app.inner().import_paths(paths);
    Ok(())
}

#[tauri::command]
pub async fn scan_progress(app: S<'_>) -> Res<crate::library::ScanProgress> {
    Ok(app.scan_progress.lock().clone())
}

#[tauri::command]
pub async fn history(app: S<'_>, limit: usize) -> Res<Vec<HistoryItem>> {
    Ok(app.store.history(limit))
}

// ---------------------------------------------------------------- sources & versions

/// Adds Spotify tracks to the library (idempotent). Returns song ids in order.
#[tauri::command]
pub async fn spotify_import(app: S<'_>, tracks: Vec<SpotifyTrack>, in_library: Option<bool>) -> Res<Vec<Id>> {
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
#[tauri::command]
pub async fn track_set_in_library(app: S<'_>, id: Id, value: bool) -> Res<Track> {
    app.store.set_in_library(&id, value).map_err(ae)?;
    if !value {
        app.prune();
    }
    crate::app::emit(&app.handle, "library-changed", vec![id.clone()]);
    app.store.track(&id).map_err(ae)
}

/// Links a Spotify track (URI or URL) as another source of a recording (the same audio).
#[tauri::command]
pub async fn source_link_spotify(app: S<'_>, recording_id: Id, uri: String) -> Res<Recording> {
    let uri = crate::spotify::normalize_uri(&uri)
        .filter(|u| u.starts_with("spotify:track:"))
        .ok_or("Not a Spotify track link")?;
    app.store.add_source(&recording_id, SourceRef { kind: "spotify".into(), key: uri }).map_err(ae)?;
    emit_recording_changed(&app, &recording_id);
    app.store.recording(&recording_id).map_err(ae)
}

#[tauri::command]
pub async fn source_remove(app: S<'_>, recording_id: Id, source: SourceRef) -> Res<Recording> {
    app.store.remove_source(&recording_id, &source).map_err(ae)?;
    emit_recording_changed(&app, &recording_id);
    app.store.recording(&recording_id).map_err(ae)
}

/// Combines songs into `keep` (their versions move; undo with `versions_separate`).
#[tauri::command]
pub async fn songs_combine(app: S<'_>, keep: Id, others: Vec<Id>) -> Res<Track> {
    app.store.combine_songs(&keep, &others).map_err(ae)?;
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    crate::app::emit(&app.handle, "recordings-changed", ());
    app.emit_perform();
    app.store.track(&keep).map_err(ae)
}

/// Moves versions of one song into a new song; returns it.
#[tauri::command]
pub async fn versions_separate(app: S<'_>, version_ids: Vec<Id>) -> Res<Track> {
    let id = app.store.separate_versions(&version_ids).map_err(ae)?;
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    crate::app::emit(&app.handle, "recordings-changed", ());
    app.emit_perform();
    app.store.track(&id).map_err(ae)
}

/// Marks recordings as the same audio, kept as `keep` (undo with `recording_split_source`).
#[tauri::command]
pub async fn recordings_merge(app: S<'_>, keep: Id, others: Vec<Id>) -> Res<Recording> {
    app.store.merge_recordings(&keep, &others).map_err(ae)?;
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    crate::app::emit(&app.handle, "recordings-changed", ());
    app.emit_perform();
    app.store.recording(&keep).map_err(ae)
}

/// Takes a source out into a recording of its own (analysed afresh): `versions`
/// play it, and with `new_song` a song of its own does.
#[tauri::command]
pub async fn recording_split_source(app: S<'_>, id: Id, source: SourceRef, versions: Vec<Id>, new_song: bool) -> Res<Recording> {
    let new = app.store.split_source(&id, &source, &versions, new_song).map_err(ae)?;
    app.queue_analysis(new.clone(), true);
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    crate::app::emit(&app.handle, "recordings-changed", ());
    app.emit_perform();
    app.store.recording(&new).map_err(ae)
}

#[tauri::command]
pub async fn version_create(app: S<'_>, track_id: Id, version: serde_json::Value) -> Res<Version> {
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

#[tauri::command]
pub async fn version_update(app: S<'_>, id: Id, patch: serde_json::Value) -> Res<Version> {
    let patch: VersionPatch = serde_json::from_value(patch).map_err(err)?;
    let v = app.store.version_update(&id, &patch).map_err(ae)?;
    let (_, tid) = app.store.version(&id).map_err(ae)?;
    crate::app::emit(&app.handle, "library-changed", vec![tid]);
    app.emit_perform();
    Ok(v)
}

#[tauri::command]
/// Takes a version out of the library (hidden while a set plays it).
pub async fn version_delete(app: S<'_>, id: Id) -> Res<()> {
    let (_, tid) = app.store.version(&id).map_err(ae)?;
    let keep: Vec<Id> = app.decks.iter().filter_map(|d| d.track_id()).collect();
    app.store.remove_version(&id, &keep).map_err(ae)?;
    crate::app::emit(&app.handle, "library-changed", vec![tid]);
    crate::app::emit(&app.handle, "crates-changed", ());
    Ok(())
}

// ---------------------------------------------------------------- crates

#[tauri::command]
pub async fn crates_list(app: S<'_>) -> Res<Vec<Crate>> {
    Ok(app.store.crates())
}

#[tauri::command]
pub async fn crate_create(app: S<'_>, name: String, smart: Option<serde_json::Value>) -> Res<Id> {
    let id = app.store.crate_create(&name, smart).map_err(ae)?;
    crate::app::emit(&app.handle, "crates-changed", ());
    Ok(id)
}

#[tauri::command]
pub async fn crate_rename(app: S<'_>, id: Id, name: String) -> Res<()> {
    app.store.crate_rename(&id, &name).map_err(ae)?;
    crate::app::emit(&app.handle, "crates-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn crate_delete(app: S<'_>, id: Id) -> Res<()> {
    app.store.crate_delete(&id).map_err(ae)?;
    app.prune();
    crate::app::emit(&app.handle, "crates-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn crate_add(app: S<'_>, id: Id, version_ids: Vec<Id>) -> Res<()> {
    app.store.crate_add(&id, &version_ids).map_err(ae)?;
    crate::app::emit(&app.handle, "crates-changed", ());
    Ok(())
}

/// Sets a smart crate's rules (`None` makes it a plain crate).
#[tauri::command]
pub async fn crate_set_smart(app: S<'_>, id: Id, smart: Option<serde_json::Value>) -> Res<()> {
    app.store.crate_set_smart(&id, smart).map_err(ae)?;
    crate::app::emit(&app.handle, "crates-changed", ());
    Ok(())
}

/// Adds a dance style to versions (a version can have several). Songs used
/// only in setlists (e.g. from Spotify) join the library.
#[tauri::command]
pub async fn style_add(app: S<'_>, style: String, version_ids: Vec<Id>) -> Res<()> {
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
#[tauri::command]
pub async fn style_remove(app: S<'_>, version_ids: Vec<Id>, style: Option<String>) -> Res<()> {
    for v in &version_ids {
        app.store.unassign_style(v, style.as_deref()).map_err(ae)?;
    }
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    app.emit_perform();
    Ok(())
}

#[tauri::command]
pub async fn crate_remove(app: S<'_>, id: Id, version_ids: Vec<Id>) -> Res<()> {
    app.store.crate_remove(&id, &version_ids).map_err(ae)?;
    app.prune();
    crate::app::emit(&app.handle, "crates-changed", ());
    Ok(())
}

// ---------------------------------------------------------------- setlists

#[tauri::command]
pub async fn setlists_list(app: S<'_>) -> Res<Vec<SetlistSummary>> {
    Ok(app.store.setlists())
}

#[tauri::command]
pub async fn setlist_get(app: S<'_>, id: Id) -> Res<Setlist> {
    app.store.setlist(&id).map_err(ae)
}

#[tauri::command]
pub async fn setlist_create(
    app: S<'_>,
    name: String,
    mode: String,
    // Plan the set with this club's latest guidelines ("" / none = no club).
    club_id: Option<String>,
    event_date: Option<String>,
) -> Res<Setlist> {
    let club = club_id.filter(|c| !c.is_empty()).and_then(|id| Some(ClubRef { version: app.clubs().latest(&id)?, id }));
    let id = app.store.setlist_create(&name, &mode, club, event_date).map_err(ae)?;
    crate::app::emit(&app.handle, "setlists-changed", ());
    app.store.setlist(&id).map_err(ae)
}

#[tauri::command]
pub async fn setlist_update(app: S<'_>, id: Id, patch: serde_json::Value) -> Res<Setlist> {
    let patch: SetlistPatch = serde_json::from_value(patch).map_err(err)?;
    app.store.setlist_update(&id, &patch).map_err(ae)?;
    crate::app::emit(&app.handle, "setlists-changed", ());
    app.emit_setlist(&id);
    app.emit_perform();
    app.store.setlist(&id).map_err(ae)
}

#[tauri::command]
pub async fn setlist_delete(app: S<'_>, id: Id) -> Res<()> {
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

#[tauri::command]
pub async fn setlist_duplicate(app: S<'_>, id: Id, name: String) -> Res<Setlist> {
    let new_id = app.store.setlist_duplicate(&id, &name).map_err(ae)?;
    crate::app::emit(&app.handle, "setlists-changed", ());
    app.store.setlist(&new_id).map_err(ae)
}

#[tauri::command]
pub async fn setlist_reset(app: S<'_>, id: Id) -> Res<()> {
    app.store.reset_setlist_status(&id).map_err(ae)?;
    app.emit_setlist(&id);
    app.emit_perform();
    // Play counts are derived from setlists.
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    Ok(())
}

#[tauri::command]
pub async fn entries_insert(app: S<'_>, setlist_id: Id, position: Option<usize>, entries: Vec<NewEntry>) -> Res<Vec<Id>> {
    let ids = app.store.entries_insert(&setlist_id, position, &entries).map_err(ae)?;
    app.emit_setlist(&setlist_id);
    app.emit_perform();
    Ok(ids)
}

#[tauri::command]
pub async fn entry_update(app: S<'_>, id: Id, patch: serde_json::Value) -> Res<SetlistEntry> {
    let patch: EntryPatch = serde_json::from_value(patch).map_err(err)?;
    let replaced = patch.version_id.is_some();
    let e = app.store.entry_update(&id, &patch).map_err(ae)?;
    if replaced {
        app.prune();
    }
    let (_, sid) = app.store.entry(&id).map_err(ae)?;
    app.emit_setlist(&sid);
    app.emit_perform();
    Ok(e)
}

#[tauri::command]
pub async fn entries_delete(app: S<'_>, setlist_id: Id, ids: Vec<Id>) -> Res<()> {
    app.store.entries_delete(&setlist_id, &ids).map_err(ae)?;
    app.prune();
    app.emit_setlist(&setlist_id);
    app.emit_perform();
    // Play counts are derived from setlists.
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    Ok(())
}

#[tauri::command]
pub async fn entries_reorder(app: S<'_>, setlist_id: Id, ids: Vec<Id>) -> Res<()> {
    app.store.entries_reorder(&setlist_id, &ids).map_err(ae)?;
    app.emit_setlist(&setlist_id);
    app.emit_perform();
    Ok(())
}

#[tauri::command]
pub async fn write_text_file(path: String, contents: String) -> Res<()> {
    crate::store::write_atomic(std::path::Path::new(&path), contents.as_bytes()).map_err(ae)
}

#[tauri::command]
pub async fn read_text_file(path: String) -> Res<String> {
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

#[tauri::command]
pub async fn styles_get(app: S<'_>) -> Res<StylesInfo> {
    Ok(StylesInfo { library: app.style_library(), usage: app.store.style_usage(), tag_usage: app.store.tag_usage() })
}

#[tauri::command]
pub async fn styles_save(app: S<'_>, mut library: StyleLibrary) -> Res<()> {
    library.upgrade();
    app.store.save_style_library(library).map_err(ae)?;
    crate::app::emit(&app.handle, "styles-changed", ());
    app.emit_perform();
    Ok(())
}

/// Renames one of your styles everywhere: songs, setlists, smart crates and
/// club mappings. Renaming onto an existing style merges the two; `to = null`
/// deletes the style (and removes it from every song).
#[tauri::command]
pub async fn styles_rekey(app: S<'_>, from: String, to: Option<String>) -> Res<()> {
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
#[tauri::command]
pub async fn tags_rekey(app: S<'_>, from: String, to: Option<String>) -> Res<()> {
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
#[tauri::command]
pub async fn tag_set(app: S<'_>, tag: String, version_ids: Vec<Id>, on: bool) -> Res<()> {
    for v in &version_ids {
        app.store.set_version_tag(v, &tag, on).map_err(ae)?;
    }
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    Ok(())
}

// ---------------------------------------------------------------- announcements (auto mode)

#[tauri::command]
pub async fn tts_status(app: S<'_>) -> Res<crate::tts::TtsStatus> {
    Ok(app.tts_status())
}

/// Downloads Piper and the selected voice (in the background).
#[tauri::command]
pub async fn tts_install(app: S<'_>) -> Res<()> {
    let a = app.inner().clone();
    std::thread::spawn(move || {
        let cfg = a.settings.read().announce.tts_config();
        if let Err(e) = a.tts.install(&cfg) {
            crate::app::emit(&a.handle, "toast", format!("Installing the announcer voice failed: {e:#}"));
        }
        a.emit_tts_status();
    });
    Ok(())
}

#[tauri::command]
pub async fn tts_voices(app: S<'_>) -> Res<Vec<crate::tts::VoiceInfo>> {
    let a = app.inner().clone();
    blocking(move || a.tts.voices()).await
}

#[tauri::command]
pub async fn tts_remove(app: S<'_>) -> Res<()> {
    app.tts.remove().map_err(ae)?;
    app.emit_tts_status();
    Ok(())
}

/// Speaks `text` now (to try a voice or pattern).
#[tauri::command]
pub async fn tts_say(app: S<'_>, text: String) -> Res<()> {
    app.speak(text);
    Ok(())
}

/// The phonemes the announcer would read for `text` (with your pronunciations).
#[tauri::command]
pub async fn tts_phonemes(app: S<'_>, text: String) -> Res<String> {
    let a = app.inner().clone();
    blocking(move || {
        let cfg = a.settings.read().announce.tts_config();
        a.tts.phonemes(&cfg, &text, &a.style_library().pronunciations)
    })
    .await
}

/// Announces the next dance now.
#[tauri::command]
pub async fn announce_next(app: S<'_>) -> Res<()> {
    app.announce_next().map_err(ae)
}

/// What an entry is called for dancers (the UI resolves club style names).
#[tauri::command]
pub async fn set_dance_name(app: S<'_>, entry_id: Id, name: String) -> Res<()> {
    app.set_dance_name(entry_id, name);
    Ok(())
}

// ---------------------------------------------------------------- clubs

fn clubs_changed(app: &App) {
    app.forget_clubs();
    crate::app::emit(&app.handle, "clubs-changed", ());
    app.emit_perform();
}

/// Every club with the versions of its guidelines, newest first.
#[tauri::command]
pub async fn clubs_list(app: S<'_>) -> Res<Vec<ClubInfo>> {
    Ok(app.clubs().list())
}

/// One version: its file text and what it defines.
#[tauri::command]
pub async fn club_get(app: S<'_>, club_id: String, version_id: String) -> Res<ClubVersion> {
    app.clubs().get(&club_id, &version_id).map_err(ae)
}

/// Checks a club file's text (the editor's text view): the club, or the error.
#[tauri::command]
pub async fn club_parse(text: String) -> Res<Club> {
    club::parse(&text).map_err(ae)
}

/// The editor's form as file text.
#[tauri::command]
pub async fn club_format(club: Club) -> Res<String> {
    Ok(club::to_toml(&club))
}

/// How many sets use each version of a club.
#[tauri::command]
pub async fn club_usage(app: S<'_>, club_id: String) -> Res<HashMap<String, usize>> {
    let mut out = HashMap::new();
    for s in app.store.setlists() {
        if let Some(c) = s.club.filter(|c| c.id == club_id) {
            *out.entry(c.version).or_insert(0) += 1;
        }
    }
    Ok(out)
}

#[tauri::command]
pub async fn club_import_file(app: S<'_>, path: String) -> Res<Imported> {
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    let c = club::parse(&text).map_err(ae)?;
    let fps = club::fetch_fingerprints(&c, &path, &HashMap::new()).await.map_err(ae)?;
    let songs = club::fetch_song_libraries(&c, &path).await.map_err(ae)?;
    let r = app.clubs().add(&text, &path, &path, None, &fps, &songs, now_ms()).map_err(ae)?;
    clubs_changed(&app);
    Ok(r)
}

/// Imports a club file published on the web.
#[tauri::command]
pub async fn club_import_url(app: S<'_>, url: String) -> Res<Imported> {
    let url = url.trim().to_string();
    let text = club::fetch(&url).await.map_err(ae)?;
    let c = club::parse(&text).map_err(ae)?;
    let fps = club::fetch_fingerprints(&c, &url, &HashMap::new()).await.map_err(ae)?;
    let songs = club::fetch_song_libraries(&c, &url).await.map_err(ae)?;
    let r = app.clubs().add(&text, &url, &url, None, &fps, &songs, now_ms()).map_err(ae)?;
    clubs_changed(&app);
    Ok(r)
}

/// Fetches the club's file again: a new version if it changed (sets keep
/// theirs until moved to it).
#[tauri::command]
pub async fn club_update(app: S<'_>, club_id: String) -> Res<Imported> {
    let url = app
        .clubs()
        .update_url(&club_id)
        .ok_or_else(|| "This club has no web address to update from (url = \"https://…\" in its file).".to_string())?;
    let text = club::fetch(&url).await.map_err(ae)?;
    let c = club::parse(&text).map_err(ae)?;
    let fps = club::fetch_fingerprints(&c, &url, &HashMap::new()).await.map_err(ae)?;
    let songs = club::fetch_song_libraries(&c, &url).await.map_err(ae)?;
    let r = app.clubs().add(&text, &url, &url, Some(&club_id), &fps, &songs, now_ms()).map_err(ae)?;
    clubs_changed(&app);
    Ok(r)
}

/// Saves edits to a version: in place while it's the newest and no set uses
/// it, else as a new version (sets keep the guidelines they were planned with).
#[tauri::command]
pub async fn club_save(app: S<'_>, club_id: String, version_id: String, text: String) -> Res<Imported> {
    let clubs = app.clubs();
    let used = app.store.setlists().iter().any(|s| s.club.as_ref().is_some_and(|c| c.id == club_id && c.version == version_id));
    let in_place = !used && clubs.latest(&club_id).as_deref() == Some(version_id.as_str());
    // Fingerprints it refers to: those already fetched, and any new ones.
    let c = club::parse(&text).map_err(ae)?;
    let base = clubs.base(&club_id, &version_id);
    let fps = club::fetch_fingerprints(&c, &base, &clubs.fingerprints(&club_id, &version_id)).await.map_err(ae)?;
    // The same song libraries as before: keep what was fetched (works offline).
    let before = clubs.club(&club_id, Some(&version_id)).ok();
    let songs = match before {
        Some(b) if b.song_library == c.song_library => b.library,
        _ => club::fetch_song_libraries(&c, &base).await.map_err(ae)?,
    };
    let r = clubs.save(&club_id, &version_id, &text, in_place, &fps, &songs, now_ms()).map_err(ae)?;
    clubs_changed(&app);
    Ok(r)
}

/// The example clubs that come with Orpheus: (id, name, description).
#[tauri::command]
pub async fn club_examples() -> Res<Vec<(String, String, String)>> {
    Ok(club::EXAMPLES
        .iter()
        .filter_map(|(id, text)| club::parse(text).ok().map(|c| (id.to_string(), c.name, c.description)))
        .collect())
}

#[tauri::command]
pub async fn club_import_example(app: S<'_>, id: String) -> Res<Imported> {
    let (_, text) = club::EXAMPLES.iter().find(|(e, _)| *e == id).ok_or_else(|| format!("no example club \"{id}\""))?;
    let c = club::parse(text).map_err(ae)?;
    let fps = club::fetch_fingerprints(&c, club::EXAMPLE_BASE, &HashMap::new()).await.map_err(ae)?;
    let songs = club::fetch_song_libraries(&c, club::EXAMPLE_BASE).await.map_err(ae)?;
    let r = app.clubs().add(text, "example", club::EXAMPLE_BASE, None, &fps, &songs, now_ms()).map_err(ae)?;
    clubs_changed(&app);
    Ok(r)
}

/// Library songs that are songs the club lists for its styles: track id → style ids.
#[tauri::command]
pub async fn club_song_matches(app: S<'_>, club_id: String, version_id: String) -> Res<HashMap<Id, Vec<String>>> {
    let a = app.inner().clone();
    blocking(move || Ok(a.club_song_matches(&ClubRef { id: club_id, version: version_id }))).await
}

/// For each style with suggested songs: each song's matching library tracks.
#[tauri::command]
pub async fn club_song_entries(app: S<'_>, club_id: String, version_id: String) -> Res<HashMap<String, Vec<Vec<Id>>>> {
    let a = app.inner().clone();
    blocking(move || Ok(a.club_song_entries(&ClubRef { id: club_id, version: version_id }))).await
}

/// A club-file song entry for a version of one of your songs (to accept it
/// for a style): the song's title, its recording's audio.
#[tauri::command]
pub async fn club_song_for_version(app: S<'_>, version_id: Id) -> Res<club::ClubSong> {
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
#[tauri::command]
pub async fn club_song_download(app: S<'_>, url: String, hash: String) -> Res<String> {
    if !club::valid_hash(&hash) {
        return Err("The club gives no valid hash for this download".into());
    }
    let resp = reqwest::get(&url).await.map_err(|e| format!("{url}: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("{url}: {}", resp.status()));
    }
    let name = url.rsplit('/').next().unwrap_or("").split(['?', '#']).next().unwrap_or("").to_string();
    let name: String = name.chars().filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')).collect();
    let name = if name.is_empty() { format!("{hash}.audio") } else { percent_decode(&name) };
    let bytes = resp.bytes().await.map_err(|e| format!("{url}: {e}"))?;
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
    app.inner().import_paths(vec![p.clone()]);
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
#[tauri::command]
pub async fn club_song_text(app: S<'_>, version_id: Id) -> Res<String> {
    let song = club_song_for_version(app, version_id).await?;
    Ok(club::song_toml(&song))
}

/// A new club with nothing but a name.
#[tauri::command]
pub async fn club_new(app: S<'_>, name: String) -> Res<Imported> {
    let text = club::to_toml(&Club { name: name.trim().to_string(), ..Default::default() });
    let r = app.clubs().add(&text, "new", "", None, &HashMap::new(), &Default::default(), now_ms()).map_err(ae)?;
    clubs_changed(&app);
    Ok(r)
}

#[tauri::command]
pub async fn club_rename_version(app: S<'_>, club_id: String, version_id: String, name: String) -> Res<()> {
    app.clubs().rename_version(&club_id, &version_id, &name).map_err(ae)?;
    clubs_changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn club_delete_version(app: S<'_>, club_id: String, version_id: String) -> Res<()> {
    app.clubs().delete_version(&club_id, &version_id).map_err(ae)?;
    clubs_changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn club_delete(app: S<'_>, club_id: String) -> Res<()> {
    app.clubs().delete(&club_id).map_err(ae)?;
    clubs_changed(&app);
    Ok(())
}

/// Writes a version's file (to share it, or publish it on a website).
#[tauri::command]
/// Writes a club's guidelines as one self-contained file (to publish on the
/// club's website): suggested songs and their fingerprints written in.
pub async fn club_export(app: S<'_>, club_id: String, version_id: String, path: String) -> Res<()> {
    let club = app.clubs().club(&club_id, Some(&version_id)).map_err(ae)?;
    std::fs::write(&path, club::to_toml(&club::self_contained(&club))).map_err(|e| format!("{path}: {e}"))
}

// ---------------------------------------------------------------- settings

#[tauri::command]
pub async fn settings_get(app: S<'_>) -> Res<Settings> {
    Ok(app.settings.read().clone())
}

#[tauri::command]
pub async fn settings_set(app: S<'_>, settings: Settings) -> Res<()> {
    let (folders_changed, lib_changed) = {
        let cur = app.settings.read();
        (cur.library_folders != settings.library_folders, cur.library_dir != settings.library_dir)
    };
    let a = app.inner().clone();
    blocking(move || a.save_settings(settings)).await?;
    if folders_changed || lib_changed {
        app.inner().scan_library();
    }
    Ok(())
}

// ---------------------------------------------------------------- audio

#[tauri::command]
pub async fn audio_devices() -> Res<Vec<AudioDevice>> {
    blocking(|| Ok(crate::audio::output::list_devices())).await
}

#[tauri::command]
pub async fn audio_status(app: S<'_>) -> Res<OutputStatus> {
    Ok(app.output_status())
}

#[tauri::command]
pub async fn audio_reopen(app: S<'_>) -> Res<OutputStatus> {
    let a = app.inner().clone();
    blocking(move || {
        a.apply_outputs();
        Ok(a.output_status())
    })
    .await
}

#[tauri::command]
pub async fn audio_test(app: S<'_>, deck: usize) -> Res<()> {
    app.test_tone(deck.min(1));
    Ok(())
}

#[tauri::command]
pub async fn deck_states(app: S<'_>) -> Res<[DeckSnapshot; 2]> {
    Ok(app.deck_snapshots())
}

/// Loads a version onto a deck (paused unless `play`). Refused for the deck
/// the room is hearing.
#[tauri::command]
pub async fn deck_load(app: S<'_>, deck: usize, version_id: Id, entry_id: Option<Id>, play: bool) -> Res<()> {
    let a = app.inner().clone();
    blocking(move || a.deck_load(deck.min(1), &version_id, entry_id.as_deref(), play)).await
}

/// Transport actions: `toggle`, `play`, `pause`, `fadeOut`, `cancelFade`, `stop`, `unload`.
#[tauri::command]
pub async fn deck_action(app: S<'_>, deck: usize, action: String) -> Res<()> {
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
#[tauri::command]
pub async fn deck_cue(app: S<'_>, deck: usize, down: bool) -> Res<()> {
    app.deck_cue(deck.min(1), down);
    Ok(())
}

/// Hot cue pad: `trigger` (jump if set, else set here) or `clear`.
#[tauri::command]
pub async fn deck_hotcue(app: S<'_>, deck: usize, slot: u8, action: String) -> Res<()> {
    let deck = deck.min(1);
    match action.as_str() {
        "clear" => app.deck_hotcue_clear(deck, slot),
        _ => app.deck_hotcue(deck, slot),
    }
    .map_err(ae)
}

/// Relative jump in seconds (negative = back).
#[tauri::command]
pub async fn deck_jump(app: S<'_>, deck: usize, secs: f64) -> Res<()> {
    app.deck_nudge_position(deck.min(1), secs).map_err(ae)
}

/// Scrub to a position (waveform drag); plays snippets when paused.
#[tauri::command]
pub async fn deck_scrub(app: S<'_>, deck: usize, ms: f64, force: Option<bool>) -> Res<()> {
    app.deck_scrub_to(deck.min(1), ms / 1000.0, force.unwrap_or(false)).map_err(ae)
}

/// Seek (click on the waveform). `force` (⌥ / Alt held) overrides the
/// on-air protection.
#[tauri::command]
pub async fn deck_seek(app: S<'_>, deck: usize, ms: i64, force: Option<bool>) -> Res<()> {
    let deck = deck.min(1);
    if app.protected(deck) && !force.unwrap_or(false) {
        return Err("Seeking is disabled on the on-air deck while it plays (Settings → General)".into());
    }
    app.decks[deck].seek(ms as f64 / 1000.0);
    Ok(())
}

#[tauri::command]
pub async fn deck_go_live(app: S<'_>, deck: usize) -> Res<String> {
    let a = app.inner().clone();
    blocking(move || a.go_live(deck.min(1))).await
}

/// Sets the loaded version's `start`, `end` or `fade` point here, or `clear`s them.
#[tauri::command]
pub async fn deck_set_point(app: S<'_>, deck: usize, kind: String) -> Res<()> {
    app.deck_set_point(deck.min(1), &kind).map_err(ae)
}

#[tauri::command]
pub async fn deck_set(
    app: S<'_>,
    deck: usize,
    tempo: Option<f64>,
    trim_db: Option<f64>,
    pfl: Option<bool>,
    fade_at_ms: Option<i64>,
    clear_fade_at: Option<bool>,
) -> Res<()> {
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
#[tauri::command]
pub async fn deck_stem(app: S<'_>, deck: usize, index: usize) -> Res<()> {
    app.deck_stem_toggle(deck.min(1), index.min(3)).map_err(ae)
}

#[tauri::command]
pub async fn stems_status(app: S<'_>) -> Res<crate::stems::StemsStatus> {
    Ok(app.stems.status())
}

#[tauri::command]
pub async fn stems_download_model(app: S<'_>) -> Res<()> {
    app.stems.download_model();
    Ok(())
}

/// Queues stem separation for recordings; returns how many were queued.
#[tauri::command]
pub async fn stems_prepare(app: S<'_>, recording_ids: Vec<Id>) -> Res<usize> {
    app.prepare_stems(&recording_ids).map_err(ae)
}

#[tauri::command]
pub async fn stems_delete(app: S<'_>, recording_id: Id) -> Res<()> {
    let t = app.store.recording(&recording_id).map_err(ae)?;
    for s in t.rec.sources.iter().filter(|s| s.kind == "file") {
        app.stems.delete(&s.key).map_err(ae)?;
    }
    crate::app::emit(&app.handle, "stems-status", app.stems.status());
    crate::app::emit(&app.handle, "library-changed", app.store.songs_for_recording(&recording_id));
    Ok(())
}

/// Content hashes (of this track's files) that have prepared stems.
#[tauri::command]
pub async fn stems_delete_all(app: S<'_>) -> Res<()> {
    app.stems.delete_all().map_err(ae)?;
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    Ok(())
}

#[tauri::command]
pub async fn stems_remove_model(app: S<'_>) -> Res<()> {
    app.stems.remove_model().map_err(ae)
}

/// "Remove from History": marks those setlist entries as not played.
#[tauri::command]
pub async fn history_remove(app: S<'_>, entry_ids: Vec<Id>) -> Res<()> {
    for id in &entry_ids {
        let (_, sid) = app.store.entry(id).map_err(ae)?;
        app.store.entry_update(id, &EntryPatch { status: Some("pending".into()), ..Default::default() }).map_err(ae)?;
        app.emit_setlist(&sid);
    }
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    crate::app::emit(&app.handle, "history-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn spotify_cache_bytes(app: S<'_>) -> Res<u64> {
    Ok(app.spotify.cache_bytes())
}

#[tauri::command]
pub async fn spotify_clear_cache(app: S<'_>) -> Res<u64> {
    app.spotify.clear_cache().map_err(ae)
}

/// Recordings with prepared stems.
#[tauri::command]
pub async fn stems_ready(app: S<'_>) -> Res<Vec<Id>> {
    Ok(app
        .store
        .all_recordings()
        .into_iter()
        .filter(|t| t.rec.sources.iter().any(|s| s.kind == "file" && app.stems.files(&s.key).is_some()))
        .map(|t| t.rec.id)
        .collect())
}

#[tauri::command]
pub async fn deck_tempo_reset(app: S<'_>, deck: usize) -> Res<()> {
    app.deck_tempo_reset(deck.min(1));
    Ok(())
}

#[tauri::command]
pub async fn levels_set(app: S<'_>, master: Option<f32>, cue: Option<f32>, crossfader: Option<f64>) -> Res<()> {
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

#[tauri::command]
pub async fn perform_open(app: S<'_>, setlist_id: Option<Id>) -> Res<PerformState> {
    app.open_setlist(setlist_id).map_err(ae)?;
    Ok(app.perform_state())
}

#[tauri::command]
pub async fn perform_state(app: S<'_>) -> Res<PerformState> {
    Ok(app.perform_state())
}

#[tauri::command]
pub async fn perform_play_next(app: S<'_>) -> Res<String> {
    let a = app.inner().clone();
    blocking(move || a.play_next()).await
}

#[tauri::command]
pub async fn perform_play_entry(app: S<'_>, entry_id: Id) -> Res<String> {
    let a = app.inner().clone();
    blocking(move || a.play_entry(entry_id)).await
}

#[tauri::command]
pub async fn perform_skip_next(app: S<'_>) -> Res<()> {
    app.skip_next().map_err(ae)
}

#[tauri::command]
pub async fn perform_set_auto_advance(app: S<'_>, value: Option<bool>) -> Res<()> {
    app.perform.lock().auto_advance_override = value;
    app.emit_perform();
    Ok(())
}

#[tauri::command]
pub async fn perform_cancel_pending(app: S<'_>) -> Res<()> {
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

#[tauri::command]
pub async fn projector_message(app: S<'_>, message: Option<String>) -> Res<()> {
    app.perform.lock().message = message.filter(|m| !m.trim().is_empty());
    app.emit_perform();
    Ok(())
}

#[tauri::command]
pub async fn projector_state(app: S<'_>) -> Res<ProjectorPayload> {
    Ok(app.projector_payload())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    pub index: usize,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub primary: bool,
}

#[tauri::command]
pub async fn monitors_list(app: S<'_>) -> Res<Vec<MonitorInfo>> {
    let win = app.handle.get_webview_window("main").ok_or("no main window")?;
    let primary = win.primary_monitor().ok().flatten().map(|m| m.position().clone());
    let mons = win.available_monitors().map_err(err)?;
    Ok(mons
        .iter()
        .enumerate()
        .map(|(i, m)| MonitorInfo {
            index: i,
            name: m.name().cloned().unwrap_or_else(|| format!("Display {}", i + 1)),
            width: m.size().width,
            height: m.size().height,
            primary: primary.as_ref().map(|p| p == m.position()).unwrap_or(false),
        })
        .collect())
}

#[tauri::command]
pub async fn projector_open(app: S<'_>, monitor: Option<usize>, fullscreen: bool) -> Res<()> {
    let handle = app.handle.clone();
    let win = match handle.get_webview_window("projector") {
        Some(w) => w,
        None => WebviewWindowBuilder::new(&handle, "projector", WebviewUrl::App("projector.html".into()))
            .title("Orpheus — Projector")
            .inner_size(1280.0, 720.0)
            .build()
            .map_err(err)?,
    };
    if let Some(i) = monitor {
        if let Ok(mons) = win.available_monitors() {
            if let Some(m) = mons.get(i) {
                let _ = win.set_fullscreen(false);
                let _ = win.set_position(*m.position());
            }
        }
    }
    win.set_fullscreen(fullscreen).map_err(err)?;
    let _ = win.show();
    Ok(())
}

#[tauri::command]
pub async fn projector_close(app: S<'_>) -> Res<()> {
    if let Some(w) = app.handle.get_webview_window("projector") {
        w.close().map_err(err)?;
    }
    Ok(())
}

// ---------------------------------------------------------------- Spotify

#[tauri::command]
pub async fn spotify_status(app: S<'_>) -> Res<SpotifyStatus> {
    Ok(app.spotify.status())
}

/// Your own Spotify app's client id for search and playlists ("" = the shared one).
pub fn spotify_web_client_id(settings: &crate::settings::Settings) -> String {
    let id = settings.spotify.client_id.trim().to_string();
    if !id.is_empty() {
        return id;
    }
    std::env::var("ORPHEUS_SPOTIFY_CLIENT_ID").unwrap_or_default()
}

#[tauri::command]
pub async fn spotify_login(app: S<'_>) -> Res<SpotifyStatus> {
    let a = app.inner().clone();
    crate::app::emit(&app.handle, "spotify-status", SpotifyStatus { connecting: true, ..app.spotify.status() });
    let r = blocking(move || {
        a.spotify.login()?;
        Ok(a.spotify.status())
    })
    .await;
    crate::app::emit(&app.handle, "spotify-status", app.spotify.status());
    r
}

/// Reconnects to Spotify and renews its tokens (e.g. right before a set).
#[tauri::command]
pub async fn spotify_refresh(app: S<'_>) -> Res<SpotifyStatus> {
    let a = app.inner().clone();
    let r = blocking(move || {
        a.spotify.refresh()?;
        Ok(a.spotify.status())
    })
    .await;
    crate::app::emit(&app.handle, "spotify-status", app.spotify.status());
    r
}

/// Sign-in with your own Spotify app, for search and playlists.
#[tauri::command]
pub async fn spotify_web_login(app: S<'_>) -> Res<SpotifyStatus> {
    let a = app.inner().clone();
    let r = blocking(move || {
        a.spotify.web_login()?;
        Ok(a.spotify.status())
    })
    .await;
    crate::app::emit(&app.handle, "spotify-status", app.spotify.status());
    r
}

#[tauri::command]
pub async fn spotify_connect_cached(app: S<'_>) -> Res<SpotifyStatus> {
    let a = app.inner().clone();
    let r = blocking(move || {
        a.spotify.connect_cached()?;
        Ok(a.spotify.status())
    })
    .await;
    crate::app::emit(&app.handle, "spotify-status", app.spotify.status());
    r
}

#[tauri::command]
pub async fn spotify_logout(app: S<'_>) -> Res<()> {
    app.spotify.logout();
    crate::app::emit(&app.handle, "spotify-status", app.spotify.status());
    Ok(())
}

#[tauri::command]
pub async fn spotify_search(app: S<'_>, query: String) -> Res<Vec<SpotifyTrack>> {
    let a = app.inner().clone();
    blocking(move || a.spotify.search(&query)).await
}

#[tauri::command]
pub async fn spotify_playlists(app: S<'_>) -> Res<Vec<SpotifyPlaylist>> {
    let a = app.inner().clone();
    blocking(move || a.spotify.my_playlists()).await
}

#[tauri::command]
pub async fn spotify_playlist_tracks(app: S<'_>, uri: String) -> Res<Vec<SpotifyTrack>> {
    let a = app.inner().clone();
    blocking(move || a.spotify.playlist_tracks(&uri)).await
}

#[tauri::command]
pub async fn spotify_resolve(app: S<'_>, input: String) -> Res<Vec<SpotifyTrack>> {
    let a = app.inner().clone();
    blocking(move || a.spotify.resolve(&input)).await
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

#[tauri::command]
pub async fn midi_info(app: S<'_>) -> Res<MidiInfo> {
    let a = app.inner().clone();
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
    .await
}

#[tauri::command]
pub async fn midi_learn(app: S<'_>) -> Res<Option<LearnedMessage>> {
    let a = app.inner().clone();
    blocking(move || Ok(a.midi.get().and_then(|m| m.learn(Duration::from_secs(15))))).await
}

#[tauri::command]
pub async fn midi_learn_cancel(app: S<'_>) -> Res<()> {
    if let Some(m) = app.midi.get() {
        m.cancel_learn();
    }
    Ok(())
}

/// Lets the UI trigger the same actions as a controller (used by keyboard shortcuts).
#[tauri::command]
pub async fn midi_action(app: S<'_>, action: String, value: Option<f64>, delta: Option<i32>) -> Res<()> {
    let v = match (value, delta) {
        (Some(v), _) => crate::midi::Value::Abs(v),
        (_, Some(d)) => crate::midi::Value::Rel(d),
        _ => crate::midi::Value::Press,
    };
    let a = app.inner().clone();
    blocking(move || {
        a.on_midi(&action, v);
        Ok(())
    })
    .await
}


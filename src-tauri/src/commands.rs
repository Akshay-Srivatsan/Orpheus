//! Tauri commands (the UI's API). Slow work runs on its own thread so the UI
//! and the main thread never block.

use crate::app::{App, PerformState, ProjectorPayload, MAIN};
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
    // The projector shows titles from tracks.
    app.emit_perform();
    app.store.track(&id).map_err(ae)
}

#[tauri::command]
pub async fn tracks_delete(app: S<'_>, ids: Vec<Id>) -> Res<()> {
    app.store.delete_tracks(&ids).map_err(ae)?;
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    crate::app::emit(&app.handle, "setlists-changed", ());
    crate::app::emit(&app.handle, "crates-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn track_waveform(app: S<'_>, id: Id) -> Res<Vec<u8>> {
    Ok(app.store.waveform(&id).unwrap_or_default())
}

#[tauri::command]
pub async fn track_analyze(app: S<'_>, ids: Vec<Id>) -> Res<()> {
    for id in ids {
        app.queue_analysis(id, true);
    }
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

/// Adds Spotify tracks to the library (idempotent). Returns track ids in order.
#[tauri::command]
pub async fn spotify_import(app: S<'_>, tracks: Vec<SpotifyTrack>) -> Res<Vec<Id>> {
    let mut ids = vec![];
    for t in tracks {
        let src = SourceRef { kind: "spotify".into(), key: t.uri.clone() };
        let id = app
            .store
            .insert_track(
                &NewTrack {
                    title: t.title,
                    artist: t.artist,
                    album: t.album,
                    duration_ms: t.duration_ms,
                    artwork_url: t.artwork_url,
                    ..Default::default()
                },
                src,
            )
            .map_err(ae)?;
        ids.push(id);
    }
    crate::app::emit(&app.handle, "library-changed", ids.clone());
    Ok(ids)
}

/// Links a Spotify track (URI or URL) as an additional source of a track.
#[tauri::command]
pub async fn source_link_spotify(app: S<'_>, track_id: Id, uri: String) -> Res<Track> {
    let uri = crate::spotify::normalize_uri(&uri)
        .filter(|u| u.starts_with("spotify:track:"))
        .ok_or("Not a Spotify track link")?;
    app.store.add_source(&track_id, SourceRef { kind: "spotify".into(), key: uri }).map_err(ae)?;
    crate::app::emit(&app.handle, "library-changed", vec![track_id.clone()]);
    app.store.track(&track_id).map_err(ae)
}

#[tauri::command]
pub async fn source_remove(app: S<'_>, track_id: Id, source: SourceRef) -> Res<Track> {
    app.store.remove_source(&track_id, &source).map_err(ae)?;
    crate::app::emit(&app.handle, "library-changed", vec![track_id.clone()]);
    app.store.track(&track_id).map_err(ae)
}

#[tauri::command]
pub async fn tracks_merge(app: S<'_>, keep: Id, others: Vec<Id>) -> Res<Track> {
    app.store.merge_tracks(&keep, &others).map_err(ae)?;
    crate::app::emit(&app.handle, "library-changed", Vec::<Id>::new());
    app.emit_perform();
    app.store.track(&keep).map_err(ae)
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
pub async fn version_delete(app: S<'_>, id: Id) -> Res<()> {
    let (_, tid) = app.store.version(&id).map_err(ae)?;
    app.store.version_delete(&id).map_err(ae)?;
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
pub async fn crate_create(app: S<'_>, name: String) -> Res<Id> {
    let id = app.store.crate_create(&name).map_err(ae)?;
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
    crate::app::emit(&app.handle, "crates-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn crate_add(app: S<'_>, id: Id, version_ids: Vec<Id>) -> Res<()> {
    app.store.crate_add(&id, &version_ids).map_err(ae)?;
    crate::app::emit(&app.handle, "crates-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn crate_remove(app: S<'_>, id: Id, version_ids: Vec<Id>) -> Res<()> {
    app.store.crate_remove(&id, &version_ids).map_err(ae)?;
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
    profile_id: String,
    lesson_style: Option<String>,
    event_date: Option<String>,
) -> Res<Setlist> {
    let id = app.store.setlist_create(&name, &mode, &profile_id, lesson_style, event_date).map_err(ae)?;
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
    crate::app::emit(&app.handle, "setlists-changed", ());
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
    let e = app.store.entry_update(&id, &patch).map_err(ae)?;
    let (_, sid) = app.store.entry(&id).map_err(ae)?;
    app.emit_setlist(&sid);
    app.emit_perform();
    Ok(e)
}

#[tauri::command]
pub async fn entries_delete(app: S<'_>, setlist_id: Id, ids: Vec<Id>) -> Res<()> {
    app.store.entries_delete(&setlist_id, &ids).map_err(ae)?;
    app.emit_setlist(&setlist_id);
    app.emit_perform();
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

// ---------------------------------------------------------------- profiles & settings

#[tauri::command]
pub async fn profiles_list(app: S<'_>) -> Res<Vec<serde_json::Value>> {
    app.profiles_json().map_err(ae)
}

#[tauri::command]
pub async fn profile_save(app: S<'_>, profile: serde_json::Value) -> Res<()> {
    let mut p = profile;
    if let Some(o) = p.as_object_mut() {
        o.remove("modified");
        o.remove("builtin");
    }
    app.store.profile_save(p).map_err(ae)?;
    crate::app::emit(&app.handle, "profiles-changed", ());
    app.emit_perform();
    Ok(())
}

#[tauri::command]
pub async fn profile_delete(app: S<'_>, id: String) -> Res<()> {
    app.store.profile_delete(&id).map_err(ae)?;
    crate::app::emit(&app.handle, "profiles-changed", ());
    app.emit_perform();
    Ok(())
}

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

#[tauri::command]
pub async fn preview_load(app: S<'_>, version_id: Id, entry_id: Option<Id>, play: bool) -> Res<()> {
    let a = app.inner().clone();
    blocking(move || a.preview_load(&version_id, entry_id.as_deref(), play)).await
}

/// Transport actions: `play`, `pause`, `toggle`, `fadeOut`, `cancelFade`, `stop`.
#[tauri::command]
pub async fn deck_action(app: S<'_>, deck: usize, action: String) -> Res<()> {
    let d = &app.decks[deck.min(1)];
    match action.as_str() {
        "play" => {
            if deck == MAIN && matches!(d.status().as_str(), "empty" | "ended" | "error") {
                app.play_next().map_err(ae)?;
            } else {
                d.play();
            }
        }
        "pause" => d.pause(),
        "toggle" => {
            if deck == MAIN && matches!(d.status().as_str(), "empty" | "ended" | "error") {
                app.play_next().map_err(ae)?;
            } else {
                d.toggle();
            }
        }
        "fadeOut" => {
            if deck == MAIN {
                app.main_fade_out()
            } else {
                d.fade_out(1.5)
            }
        }
        "cancelFade" => {
            d.cancel_fade();
            app.perform.lock().pending = None;
        }
        "stop" => {
            if deck == MAIN && d.status() == "playing" {
                return Err("Fade out or pause the main deck first".into());
            }
            d.pause();
            d.seek(d.snapshot().start_ms as f64 / 1000.0);
        }
        "unload" => {
            if deck == MAIN && d.status() == "playing" {
                return Err("The main deck is playing".into());
            }
            d.unload();
        }
        _ => return Err(format!("unknown action {action}")),
    }
    app.emit_perform();
    Ok(())
}

#[tauri::command]
pub async fn deck_seek(app: S<'_>, deck: usize, ms: i64) -> Res<()> {
    app.decks[deck.min(1)].seek(ms as f64 / 1000.0);
    Ok(())
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
    let d = &app.decks[deck.min(1)];
    if let Some(t) = tempo {
        d.set_tempo(t);
    }
    if let Some(g) = trim_db {
        d.set_trim_db(g);
    }
    if let Some(p) = pfl {
        d.set_pfl(p);
    }
    if let Some(f) = fade_at_ms {
        d.set_fade_at(Some(f as f64 / 1000.0));
    } else if clear_fade_at == Some(true) {
        d.set_fade_at(None);
    }
    Ok(())
}

#[tauri::command]
pub async fn levels_set(app: S<'_>, master: Option<f32>, cue: Option<f32>) -> Res<()> {
    if let Some(m) = master {
        app.levels.master.set(m.clamp(0.0, 1.5));
    }
    if let Some(c) = cue {
        app.levels.cue.set(c.clamp(0.0, 1.5));
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
    app.decks[MAIN].cancel_fade();
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

fn spotify_client_id(app: &App) -> String {
    let id = app.settings.read().spotify.client_id.clone();
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
        let cid = spotify_client_id(&a);
        a.spotify.login(&cid)?;
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
        let cid = spotify_client_id(&a);
        a.spotify.connect_cached(&cid)?;
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
            actions: crate::midi::ACTIONS.iter().map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string())).collect(),
            led_states: crate::midi::LED_STATES.iter().map(|s| s.to_string()).collect(),
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


//! Orpheus: DJ software for social dancing.

pub mod analysis;
pub mod app;
pub mod audio;
pub mod commands;
pub mod controls;
pub mod store;
pub mod decode;
pub mod library;
pub mod midi;
pub mod model;
pub mod perform;
pub mod settings;
pub mod spotify;

use std::sync::Arc;
use tauri::{Manager, RunEvent};

pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,librespot=warn,symphonia=warn"))
        .init();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = match std::env::var_os("ORPHEUS_DATA_DIR") {
                Some(d) => d.into(),
                None => app.path().app_data_dir()?,
            };
            log::info!("data directory: {}", data_dir.display());
            let state = app::App::new(app.handle().clone(), data_dir)?;
            app.manage(state.clone());
            state.prune();
            state.scan_library();
            let s = state.clone();
            std::thread::spawn(move || {
                let (auto, cid) = {
                    let st = s.settings.read();
                    (st.spotify.auto_connect, st.spotify.client_id.clone())
                };
                if auto && s.spotify.status().has_cached_credentials {
                    if let Err(e) = s.spotify.connect_cached(&cid) {
                        log::warn!("Spotify auto-connect failed: {e:#}");
                    }
                    app::emit(&s.handle, "spotify-status", s.spotify.status());
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::library_tracks,
            commands::track_get,
            commands::track_update,
            commands::tracks_delete,
            commands::library_reload,
            commands::library_info,
            commands::setlist_get,
            commands::track_waveform,
            commands::track_reset_bpm,
            commands::track_analyze,
            commands::library_rescan,
            commands::library_import,
            commands::scan_progress,
            commands::history,
            commands::crates_list,
            commands::crate_create,
            commands::crate_rename,
            commands::crate_delete,
            commands::crate_add,
            commands::crate_remove,
            commands::setlists_list,
            commands::setlist_create,
            commands::setlist_update,
            commands::setlist_delete,
            commands::setlist_duplicate,
            commands::setlist_reset,
            commands::entries_insert,
            commands::entry_update,
            commands::entries_delete,
            commands::entries_reorder,
            commands::write_text_file,
            commands::read_text_file,
            commands::profiles_list,
            commands::profile_save,
            commands::profile_delete,
            commands::settings_get,
            commands::settings_set,
            commands::audio_devices,
            commands::audio_status,
            commands::audio_reopen,
            commands::audio_test,
            commands::deck_states,
            commands::deck_load,
            commands::deck_action,
            commands::deck_cue,
            commands::deck_hotcue,
            commands::deck_jump,
            commands::deck_scrub,
            commands::deck_seek,
            commands::deck_go_live,
            commands::deck_set_point,
            commands::deck_set,
            commands::deck_tempo_reset,
            commands::levels_set,
            commands::perform_open,
            commands::perform_state,
            commands::perform_play_next,
            commands::perform_play_entry,
            commands::perform_skip_next,
            commands::perform_set_auto_advance,
            commands::perform_cancel_pending,
            commands::projector_message,
            commands::projector_state,
            commands::monitors_list,
            commands::projector_open,
            commands::projector_close,
            commands::spotify_status,
            commands::spotify_login,
            commands::spotify_connect_cached,
            commands::spotify_logout,
            commands::spotify_search,
            commands::spotify_playlists,
            commands::spotify_playlist_tracks,
            commands::spotify_resolve,
            commands::spotify_import,
            commands::source_link_spotify,
            commands::track_set_in_library,
            commands::source_remove,
            commands::tracks_merge,
            commands::version_create,
            commands::version_update,
            commands::version_delete,
            commands::midi_info,
            commands::midi_learn,
            commands::midi_learn_cancel,
            commands::midi_action,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Orpheus")
        .run(|handle, event| {
            if let RunEvent::WindowEvent { label, event: tauri::WindowEvent::CloseRequested { .. }, .. } = &event {
                // Closing the main window quits the app (projector included).
                if label == "main" {
                    if let Some(p) = handle.get_webview_window("projector") {
                        let _ = p.close();
                    }
                }
            }
            if let RunEvent::Exit = event {
                if let Some(s) = handle.try_state::<Arc<app::App>>() {
                    for d in &s.decks {
                        d.pause();
                    }
                }
            }
        });
}

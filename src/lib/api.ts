// Typed wrappers around the backend's Tauri commands.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type * as T from "./types";

const call = <R>(cmd: string, args?: Record<string, unknown>) => invoke<R>(cmd, args);

export const api = {
  // library
  tracks: () => call<T.Track[]>("library_tracks"),
  track: (id: T.Id) => call<T.Track>("track_get", { id }),
  updateTrack: (id: T.Id, patch: T.TrackPatch) => call<T.Track>("track_update", { id, patch }),
  deleteTracks: (ids: T.Id[]) => call<void>("tracks_delete", { ids }),
  resetBpm: (id: T.Id, from: "detected" | "tag") => call<T.Track>("track_reset_bpm", { id, from }),
  /** Cover art: a data URL (local files) or a Spotify image URL. */
  artwork: (id: T.Id) => call<string | null>("track_artwork", { id }),
  waveform: (id: T.Id) => call<number[]>("track_waveform", { id }),
  analyze: (ids: T.Id[]) => call<void>("track_analyze", { ids }),
  rescan: () => call<void>("library_rescan"),
  reloadLibrary: () => call<string[]>("library_reload"),
  libraryInfo: () => call<T.LibraryInfo>("library_info"),
  importPaths: (paths: string[]) => call<void>("library_import", { paths }),
  scanProgress: () => call<T.ScanProgress>("scan_progress"),
  history: (limit: number) => call<T.HistoryItem[]>("history", { limit }),
  /** "Remove from History": marks those setlist entries as not played. */
  removeHistory: (entryIds: T.Id[]) => call<void>("history_remove", { entryIds }),

  // sources & versions
  linkSpotify: (trackId: T.Id, uri: string) => call<T.Track>("source_link_spotify", { trackId, uri }),
  removeSource: (trackId: T.Id, source: T.SourceRef) => call<T.Track>("source_remove", { trackId, source }),
  mergeTracks: (keep: T.Id, others: T.Id[]) => call<T.Track>("tracks_merge", { keep, others }),
  createVersion: (trackId: T.Id, version: T.VersionPatch) => call<T.Version>("version_create", { trackId, version }),
  updateVersion: (id: T.Id, patch: T.VersionPatch) => call<T.Version>("version_update", { id, patch }),
  deleteVersion: (id: T.Id) => call<void>("version_delete", { id }),

  // crates
  crates: () => call<T.Crate[]>("crates_list"),
  createCrate: (name: string, smart: import("./smart").SmartRules | null = null) => call<T.Id>("crate_create", { name, smart }),
  setCrateSmart: (id: T.Id, smart: import("./smart").SmartRules | null) => call<void>("crate_set_smart", { id, smart }),
  /** Adds one of your dance styles to versions (a version can have several). */
  styleAdd: (style: string, versionIds: T.Id[]) => call<void>("style_add", { style, versionIds }),
  /** Removes a dance style from versions; without `style`, all of their styles. */
  styleRemove: (versionIds: T.Id[], style: string | null = null) => call<void>("style_remove", { versionIds, style }),
  renameCrate: (id: T.Id, name: string) => call<void>("crate_rename", { id, name }),
  deleteCrate: (id: T.Id) => call<void>("crate_delete", { id }),
  addToCrate: (id: T.Id, versionIds: T.Id[]) => call<void>("crate_add", { id, versionIds }),
  removeFromCrate: (id: T.Id, versionIds: T.Id[]) => call<void>("crate_remove", { id, versionIds }),

  // setlists
  setlists: () => call<T.SetlistSummary[]>("setlists_list"),
  setlist: (id: T.Id) => call<T.Setlist>("setlist_get", { id }),
  createSetlist: (a: { name: string; mode: T.SetlistMode; profileId: string; eventDate?: string | null }) =>
    call<T.Setlist>("setlist_create", { name: a.name, mode: a.mode, profileId: a.profileId, eventDate: a.eventDate ?? null }),
  updateSetlist: (id: T.Id, patch: Partial<Omit<T.SetlistSummary, "id" | "entryCount">>) => call<T.Setlist>("setlist_update", { id, patch }),
  deleteSetlist: (id: T.Id) => call<void>("setlist_delete", { id }),
  duplicateSetlist: (id: T.Id, name: string) => call<T.Setlist>("setlist_duplicate", { id, name }),
  resetSetlist: (id: T.Id) => call<void>("setlist_reset", { id }),
  insertEntries: (setlistId: T.Id, entries: T.NewEntry[], position?: number | null) =>
    call<T.Id[]>("entries_insert", { setlistId, position: position ?? null, entries }),
  updateEntry: (id: T.Id, patch: T.EntryPatch) => call<T.SetlistEntry>("entry_update", { id, patch }),
  deleteEntries: (setlistId: T.Id, ids: T.Id[]) => call<void>("entries_delete", { setlistId, ids }),
  reorderEntries: (setlistId: T.Id, ids: T.Id[]) => call<void>("entries_reorder", { setlistId, ids }),
  writeTextFile: (path: string, contents: string) => call<void>("write_text_file", { path, contents }),
  readTextFile: (path: string) => call<string>("read_text_file", { path }),

  // your styles
  styles: () => call<T.StylesInfo>("styles_get"),
  saveStyles: (library: T.StyleLibrary) => call<void>("styles_save", { library }),
  /** Renames a style key on every song; `to = null` removes the style from all songs. */
  rekeyStyle: (from: string, to: string | null) => call<void>("styles_rekey", { from, to }),
  /** Renames one of your tags on every song; `to = null` deletes it. */
  rekeyTag: (from: string, to: string | null) => call<void>("tags_rekey", { from, to }),
  /** Adds (`on`) or removes one of your tags on versions. */
  setTag: (tag: string, versionIds: T.Id[], on: boolean) => call<void>("tag_set", { tag, versionIds, on }),

  // profiles & settings
  profiles: () => call<T.Profile[]>("profiles_list"),
  saveProfile: (profile: T.Profile) => call<void>("profile_save", { profile }),
  deleteProfile: (id: string) => call<void>("profile_delete", { id }),
  settings: () => call<T.Settings>("settings_get"),
  saveSettings: (settings: T.Settings) => call<void>("settings_set", { settings }),

  // audio
  audioDevices: () => call<T.AudioDevice[]>("audio_devices"),
  audioStatus: () => call<T.OutputStatus>("audio_status"),
  audioReopen: () => call<T.OutputStatus>("audio_reopen"),
  audioTest: (deck: 0 | 1) => call<void>("audio_test", { deck }),
  deckStates: () => call<[T.DeckState, T.DeckState]>("deck_states"),
  deckLoad: (deck: 0 | 1, versionId: T.Id, opts: { entryId?: T.Id | null; play?: boolean } = {}) =>
    call<void>("deck_load", { deck, versionId, entryId: opts.entryId ?? null, play: opts.play ?? false }),
  deckAction: (deck: 0 | 1, action: "play" | "pause" | "toggle" | "fadeOut" | "cancelFade" | "stop" | "unload") =>
    call<void>("deck_action", { deck, action }),
  deckCue: (deck: 0 | 1, down: boolean) => call<void>("deck_cue", { deck, down }),
  deckHotcue: (deck: 0 | 1, slot: number, action: "trigger" | "clear") => call<void>("deck_hotcue", { deck, slot, action }),
  deckJump: (deck: 0 | 1, secs: number) => call<void>("deck_jump", { deck, secs }),
  deckScrub: (deck: 0 | 1, ms: number) => call<void>("deck_scrub", { deck, ms }),
  deckGoLive: (deck: 0 | 1) => call<string>("deck_go_live", { deck }),
  deckSetPoint: (deck: 0 | 1, kind: "start" | "end" | "fade" | "clear") => call<void>("deck_set_point", { deck, kind }),
  deckTempoReset: (deck: 0 | 1) => call<void>("deck_tempo_reset", { deck }),
  /** Toggles a stem (0 vocals, 1 drums, 2 bass, 3 other); starts preparing stems if the song has none. */
  deckStem: (deck: 0 | 1, index: number) => call<void>("deck_stem", { deck, index }),
  stemsStatus: () => call<T.StemsStatus>("stems_status"),
  stemsDownloadModel: () => call<void>("stems_download_model"),
  stemsPrepare: (trackIds: T.Id[]) => call<number>("stems_prepare", { trackIds }),
  stemsDelete: (trackId: T.Id) => call<void>("stems_delete", { trackId }),
  /** Tracks that have prepared stems on this computer. */
  stemsReady: () => call<T.Id[]>("stems_ready"),
  stemsDeleteAll: () => call<void>("stems_delete_all"),
  stemsRemoveModel: () => call<void>("stems_remove_model"),
  spotifyCacheBytes: () => call<number>("spotify_cache_bytes"),
  spotifyClearCache: () => call<number>("spotify_clear_cache"),
  deckSeek: (deck: 0 | 1, ms: number) => call<void>("deck_seek", { deck, ms: Math.round(ms) }),
  deckSet: (deck: 0 | 1, o: { tempo?: number; trimDb?: number; pfl?: boolean; fadeAtMs?: number | null }) =>
    call<void>("deck_set", {
      deck,
      tempo: o.tempo ?? null,
      trimDb: o.trimDb ?? null,
      pfl: o.pfl ?? null,
      fadeAtMs: o.fadeAtMs ?? null,
      clearFadeAt: "fadeAtMs" in o && o.fadeAtMs === null,
    }),
  setLevels: (o: { master?: number; cue?: number; crossfader?: number }) =>
    call<void>("levels_set", { master: o.master ?? null, cue: o.cue ?? null, crossfader: o.crossfader ?? null }),

  // performance
  performOpen: (setlistId: T.Id | null) => call<T.PerformState>("perform_open", { setlistId }),
  performState: () => call<T.PerformState>("perform_state"),
  playNext: () => call<"armed" | "fading" | "queued" | "started">("perform_play_next"),
  playEntry: (entryId: T.Id) => call<string>("perform_play_entry", { entryId }),
  skipNext: () => call<void>("perform_skip_next"),
  setAutoAdvance: (value: boolean | null) => call<void>("perform_set_auto_advance", { value }),
  cancelPending: () => call<void>("perform_cancel_pending"),
  projectorMessage: (message: string | null) => call<void>("projector_message", { message }),
  projectorState: () => call<T.ProjectorPayload>("projector_state"),
  monitors: () => call<T.MonitorInfo[]>("monitors_list"),
  projectorOpen: (monitor: number | null, fullscreen: boolean) => call<void>("projector_open", { monitor, fullscreen }),
  projectorClose: () => call<void>("projector_close"),

  // Spotify
  spotifyStatus: () => call<T.SpotifyStatus>("spotify_status"),
  spotifyLogin: () => call<T.SpotifyStatus>("spotify_login"),
  /** Sign-in with your own Spotify app, for search and playlists. */
  spotifyWebLogin: () => call<T.SpotifyStatus>("spotify_web_login"),
  spotifyConnectCached: () => call<T.SpotifyStatus>("spotify_connect_cached"),
  spotifyLogout: () => call<void>("spotify_logout"),
  spotifySearch: (query: string) => call<T.SpotifyTrack[]>("spotify_search", { query }),
  spotifyPlaylists: () => call<T.SpotifyPlaylist[]>("spotify_playlists"),
  spotifyPlaylistTracks: (uri: string) => call<T.SpotifyTrack[]>("spotify_playlist_tracks", { uri }),
  spotifyResolve: (input: string) => call<T.SpotifyTrack[]>("spotify_resolve", { input }),
  /** Creates tracks for Spotify songs. `inLibrary: false` = for setlists only (hidden from the library). */
  spotifyImport: (tracks: T.SpotifyTrack[], inLibrary = true) => call<T.Id[]>("spotify_import", { tracks, inLibrary }),
  setInLibrary: (id: T.Id, value: boolean) => call<T.Track>("track_set_in_library", { id, value }),

  // announcements
  ttsStatus: () => call<T.TtsStatus>("tts_status"),
  /** Downloads Piper and the selected voice in the background ("tts-status" events). */
  ttsInstall: () => call<void>("tts_install"),
  ttsVoices: () => call<T.VoiceInfo[]>("tts_voices"),
  ttsRemove: () => call<void>("tts_remove"),
  ttsSay: (text: string) => call<void>("tts_say", { text }),
  /** The phonemes the announcer would read (with your pronunciations). */
  ttsPhonemes: (text: string) => call<string>("tts_phonemes", { text }),
  announceNext: () => call<void>("announce_next"),
  /** Tells the backend what an entry is called for dancers (club names are resolved here). */
  setDanceName: (entryId: T.Id, name: string) => call<void>("set_dance_name", { entryId, name }),

  // MIDI
  midiInfo: () => call<T.MidiInfo>("midi_info"),
  midiLearn: () => call<{ status: number; data1: number; kind: "button" | "absolute" } | null>("midi_learn"),
  midiLearnCancel: () => call<void>("midi_learn_cancel"),
  midiAction: (action: string, value?: number, delta?: number) =>
    call<void>("midi_action", { action, value: value ?? null, delta: delta ?? null }),
};

export interface Events {
  "deck-state": [T.DeckState, T.DeckState];
  levels: T.Levels;
  "perform-state": T.PerformState;
  projector: T.ProjectorPayload;
  "library-changed": T.Id[];
  "setlist-changed": T.Id;
  "setlists-changed": null;
  "crates-changed": null;
  "profiles-changed": null;
  "styles-changed": null;
  "settings-changed": T.Settings;
  "scan-progress": T.ScanProgress;
  "spotify-status": T.SpotifyStatus;
  "output-status": T.OutputStatus;
  "midi-ui": { action: string; delta?: number; deck?: 0 | 1 };
  "toast-info": string;
  "stems-status": T.StemsStatus;
  "tts-status": T.TtsStatus;
  "open-settings": string;
  "history-changed": null;
  toast: string;
  imported: T.Id[];
}

export function on<K extends keyof Events>(event: K, cb: (payload: Events[K]) => void): Promise<UnlistenFn> {
  return listen<Events[K]>(event, (e) => cb(e.payload));
}

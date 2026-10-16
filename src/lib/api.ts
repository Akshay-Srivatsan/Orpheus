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
  // recordings (ids are recording ids)
  updateRecording: (id: T.Id, patch: T.RecordingPatch) => call<T.Recording>("recording_update", { id, patch }),
  resetBpm: (id: T.Id, from: "detected" | "tag") => call<T.Recording>("recording_reset_bpm", { id, from }),
  /** Cover art: a data URL (local files) or a Spotify image URL. */
  artwork: (id: T.Id) => call<string | null>("recording_artwork", { id }),
  waveform: (id: T.Id) => call<number[]>("recording_waveform", { id }),
  analyze: (ids: T.Id[]) => call<void>("recordings_analyze", { ids }),
  /** A new song (in the library) for a recording, e.g. a hidden one. */
  songFromRecording: (id: T.Id) => call<T.Track>("song_from_recording", { id }),
  /** Compares recordings with the first (fingerprints): are they the same audio? */
  compareRecordings: (ids: T.Id[]) => call<{ id: T.Id; known: boolean; coverage: number; error: number; same: boolean }[]>("recordings_compare", { ids }),
  /** Removes recordings from the library with the versions playing them (files stay on disk). */
  removeRecordings: (ids: T.Id[]) => call<void>("recordings_remove", { ids }),
  /** The audio files in the library folders, and what each is in the library. */
  libraryFiles: () => call<T.LibraryFile[]>("library_files"),
  /** Adds files to the library, including ones removed from it before. */
  addFiles: (paths: string[]) => call<void>("library_add_files", { paths }),
  /** Marks recordings as the same audio, kept as `keep`. */
  mergeRecordings: (keep: T.Id, others: T.Id[]) => call<T.Recording>("recordings_merge", { keep, others }),
  /** Takes a source out into a recording of its own: `versions` play it, and with `newSong` a song of its own. */
  splitSource: (id: T.Id, source: T.SourceRef, versions: T.Id[], newSong = false) =>
    call<T.Recording>("recording_split_source", { id, source, versions, newSong }),
  rescan: () => call<void>("library_rescan"),
  reloadLibrary: () => call<string[]>("library_reload"),
  libraryInfo: () => call<T.LibraryInfo>("library_info"),
  importPaths: (paths: string[]) => call<void>("library_import", { paths }),
  scanProgress: () => call<T.ScanProgress>("scan_progress"),
  history: (limit: number) => call<T.HistoryItem[]>("history", { limit }),
  /** "Remove from History": marks those setlist entries as not played. */
  removeHistory: (entryIds: T.Id[]) => call<void>("history_remove", { entryIds }),

  // sources & versions
  linkSpotify: (recordingId: T.Id, uri: string) => call<T.Recording>("source_link_spotify", { recordingId, uri }),
  removeSource: (recordingId: T.Id, source: T.SourceRef) => call<T.Recording>("source_remove", { recordingId, source }),
  /** Combines songs into `keep` (their versions move). */
  combineSongs: (keep: T.Id, others: T.Id[]) => call<T.Track>("songs_combine", { keep, others }),
  /** Moves versions of one song into a new song. */
  separateVersions: (versionIds: T.Id[]) => call<T.Track>("versions_separate", { versionIds }),
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
  /** `clubId`: plan it with that club's latest guidelines ("" = none). */
  createSetlist: (a: { name: string; mode: T.SetlistMode; clubId: string; eventDate?: string | null }) =>
    call<T.Setlist>("setlist_create", { name: a.name, mode: a.mode, clubId: a.clubId || null, eventDate: a.eventDate ?? null }),
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

  // clubs (see club.rs)
  clubs: () => call<T.ClubInfo[]>("clubs_list"),
  club: (clubId: string, versionId: string) => call<T.ClubVersion>("club_get", { clubId, versionId }),
  /** Checks a club file's text: the club, or an error naming the line. */
  clubParse: (text: string) => call<T.Club>("club_parse", { text }),
  clubFormat: (club: T.Club) => call<string>("club_format", { club }),
  /** Sets using each version. */
  clubUsage: (clubId: string) => call<Record<string, number>>("club_usage", { clubId }),
  clubImportFile: (path: string) => call<T.ClubImported>("club_import_file", { path }),
  clubImportUrl: (url: string) => call<T.ClubImported>("club_import_url", { url }),
  clubUpdate: (clubId: string) => call<T.ClubImported>("club_update", { clubId }),
  /** Saves edits: in place if no set uses the version, else as a new version. */
  clubSave: (clubId: string, versionId: string, text: string) => call<T.ClubImported>("club_save", { clubId, versionId, text }),
  clubNew: (name: string) => call<T.ClubImported>("club_new", { name }),
  clubRenameVersion: (clubId: string, versionId: string, name: string) => call<void>("club_rename_version", { clubId, versionId, name }),
  clubDeleteVersion: (clubId: string, versionId: string) => call<void>("club_delete_version", { clubId, versionId }),
  clubDelete: (clubId: string) => call<void>("club_delete", { clubId }),
  clubExport: (clubId: string, versionId: string, path: string) => call<void>("club_export", { clubId, versionId, path }),
  /** Recordings that are the club's suggested songs: recording id → style ids. */
  clubSongMatches: (clubId: string, versionId: string) => call<Record<T.Id, string[]>>("club_song_matches", { clubId, versionId }),
  /** For each style with suggested songs: each song's matching recordings. */
  clubSongEntries: (clubId: string, versionId: string) => call<Record<string, T.Id[][]>>("club_song_entries", { clubId, versionId }),
  /** A club-file song entry for a version of one of your songs. */
  clubSongForVersion: (versionId: T.Id) => call<import("./clubs").ClubSong>("club_song_for_version", { versionId }),
  /** The same as text to paste into a club file. */
  clubSongText: (versionId: T.Id) => call<string>("club_song_text", { versionId }),
  /** Downloads a suggested song (checked against its hash) and adds it to the library. */
  clubSongDownload: (url: string, hash: string) => call<string>("club_song_download", { url, hash }),
  /** Bundled examples: [id, name, description]. */
  clubExamples: () => call<[string, string, string][]>("club_examples"),
  clubImportExample: (id: string) => call<T.ClubImported>("club_import_example", { id }),
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
  /** `force`: override the on-air protection (⌥ / Alt held). */
  deckScrub: (deck: 0 | 1, ms: number, force = false) => call<void>("deck_scrub", { deck, ms, force }),
  deckGoLive: (deck: 0 | 1) => call<string>("deck_go_live", { deck }),
  deckSetPoint: (deck: 0 | 1, kind: "start" | "end" | "fade" | "clear") => call<void>("deck_set_point", { deck, kind }),
  deckTempoReset: (deck: 0 | 1) => call<void>("deck_tempo_reset", { deck }),
  /** Toggles a stem (0 vocals, 1 drums, 2 bass, 3 other); starts preparing stems if the song has none. */
  deckStem: (deck: 0 | 1, index: number) => call<void>("deck_stem", { deck, index }),
  stemsStatus: () => call<T.StemsStatus>("stems_status"),
  stemsDownloadModel: () => call<void>("stems_download_model"),
  stemsPrepare: (recordingIds: T.Id[]) => call<number>("stems_prepare", { recordingIds }),
  stemsDelete: (recordingId: T.Id) => call<void>("stems_delete", { recordingId }),
  /** Recordings that have prepared stems on this computer. */
  stemsReady: () => call<T.Id[]>("stems_ready"),
  stemsDeleteAll: () => call<void>("stems_delete_all"),
  stemsRemoveModel: () => call<void>("stems_remove_model"),
  spotifyCacheBytes: () => call<number>("spotify_cache_bytes"),
  spotifyClearCache: () => call<number>("spotify_clear_cache"),
  /** `force`: override the on-air protection (⌥ / Alt held). */
  deckSeek: (deck: 0 | 1, ms: number, force = false) => call<void>("deck_seek", { deck, ms: Math.round(ms), force }),
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
  /** Reconnects and renews the tokens (Orpheus also reconnects by itself when Spotify drops the session). */
  spotifyRefresh: () => call<T.SpotifyStatus>("spotify_refresh"),
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
  "clubs-changed": null;
  "styles-changed": null;
  "recordings-changed": null;
  "settings-changed": T.Settings;
  "scan-progress": T.ScanProgress;
  "spotify-status": T.SpotifyStatus;
  "output-status": T.OutputStatus;
  "midi-ui": { action: string; delta?: number; deck?: 0 | 1 };
  "toast-info": string;
  /** Play Next / Go Live / auto mode refused: the crossfader is off center toward the other deck. */
  "crossfader-blocked": 0 | 1;
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

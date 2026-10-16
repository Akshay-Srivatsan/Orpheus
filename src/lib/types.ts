// Mirrors the backend's serde types (src-tauri/src/model.rs, settings.rs, app.rs).

export type Id = string;

/** Where a track's audio comes from: a local file (by content hash) or Spotify. */
export interface SourceRef {
  kind: "file" | "spotify";
  /** Content hash of the audio (files) or Spotify track URI. */
  key: string;
}

/** Playback settings a version defines and a setlist entry can override. Absent = inherit. */
export interface Overrides {
  targetBpm?: number | null;
  cueInMs?: number | null;
  cueOutMs?: number | null;
  fadeOutMs?: number | null;
  gainDb?: number | null;
  stems?: Record<string, boolean> | null;
}

/** A playable version of a song: the recording it plays, and how (styles, tempo, cue points, comments). */
export interface Version extends Overrides {
  id: Id;
  name: string;
  /** The recording it plays. */
  recording: Id;
  /** Shown to dancers instead of the style names (e.g. a line dance's name). */
  displayName?: string | null;
  /** Your dance styles (keys into styles.json) this version can be danced as. */
  styles?: string[];
  comment?: string;
  /** Your tags (keys into styles.json's tags). */
  tags?: string[];
}

/**
 * A recording: a piece of audio (its sources are the same audio) and its
 * analysis. Songs' versions play recordings; several songs can share one, and
 * one no song uses is hidden.
 */
export interface Recording {
  id: Id;
  /** From the file's tags or Spotify (a song made from it is named after them). */
  title: string;
  artist: string;
  album: string;
  genre?: string;
  durationMs: number;
  bpm: number | null;
  bpmSource: string | null;
  /** Last BPM found by analysis (kept so an edited BPM can be reset). */
  detectedBpm?: number | null;
  /** BPM from the file's tags, if any. */
  tagBpm?: number | null;
  /** Beats per bar: 3 (triple, e.g. waltzes) or 4 (duple); null = unknown. */
  beatsPerBar?: number | null;
  /** "detected" or "user". */
  meterSource?: string | null;
  detectedMeter?: number | null;
  gainDb?: number;
  loudnessDb: number | null;
  audioStartMs: number | null;
  audioEndMs: number | null;
  /** Chromaprint fingerprint (text form), from analysis. */
  fingerprint?: string | null;
  analyzed: boolean;
  preferredSource?: "file" | "spotify" | null;
  artworkUrl?: string | null;
  addedAt: number;
  sources: SourceRef[];
  aliases?: Id[];
  /** Saved cue points (pads 1–4), shared by every version playing it. */
  hotCues?: { slot: number; ms: number }[];
  /** Paths of its files on this machine. */
  files: string[];
  missing: boolean;
  /** Songs whose versions play it (none: hidden). */
  songs: Id[];
}

/** A song: what the DJ thinks of as one song, with versions to play (which may play different recordings). */
export interface Track {
  id: Id;
  title: string;
  artist: string;
  album: string;
  genre?: string;
  rating: number | null;
  notes?: string;
  addedAt: number;
  versions: Version[];
  /** The version that stands for the song (folded rows, default choice); absent: the first. Use `primaryVersion`. */
  primary?: Id | null;
  aliases?: Id[];
  /** false = used only in setlists (e.g. a Spotify song added straight to a set). Absent = true. */
  inLibrary?: boolean;
  /** The recordings its versions play. */
  recordings: Recording[];
  /** No version has a playable source. */
  missing: boolean;
  playCount: number;
  lastPlayedAt: number | null;
}

export type TrackPatch = Partial<Pick<Track, "title" | "artist" | "album" | "genre" | "rating" | "notes" | "primary">>;

/** A song's primary version: the one chosen, else the first. */
export function primaryVersion(track: Track): Version | undefined {
  return track.versions.find((v) => v.id === track.primary) ?? track.versions[0];
}

export type RecordingPatch = Partial<Pick<Recording, "bpm" | "bpmSource" | "beatsPerBar" | "gainDb" | "preferredSource">>;

/** The recording a version of a song plays. */
export function recordingOf(track: Track | undefined, version: Version | undefined): Recording | undefined {
  if (!track || !version) return undefined;
  return track.recordings.find((r) => r.id === version.recording);
}

/** Stands in for a recording that isn't in the library (e.g. deleted elsewhere): nothing to play. */
export function missingRecording(id: Id, track: Track): Recording {
  return {
    id, title: track.title, artist: track.artist, album: track.album, durationMs: 0, bpm: null, bpmSource: null,
    loudnessDb: null, audioStartMs: null, audioEndMs: null, analyzed: false, addedAt: track.addedAt, sources: [],
    files: [], missing: true, songs: [track.id],
  };
}

/** A song's rows: each version with the recording it plays. */
export function rowsOf(track: Track): Row[] {
  return track.versions.map((v) => ({ track, version: v, rec: recordingOf(track, v) ?? missingRecording(v.recording, track) }));
}

export type VersionPatch = Partial<Omit<Version, "id">>;

/** A library row: one version of a song, and the recording it plays. */
export interface Row {
  track: Track;
  version: Version;
  rec: Recording;
}

/**
 * A crate: the DJ's own collection of songs, mixing dance styles freely.
 * Smart crates have rules instead of a hand-picked list (see lib/smart.ts).
 */
export interface Crate {
  id: Id;
  name: string;
  position: number;
  /** Hand-picked members (empty for smart crates). */
  versionIds: Id[];
  smart?: import("./smart").SmartRules | null;
}

export type SetlistMode = "live" | "preset";

export interface SetlistSummary {
  id: Id;
  name: string;
  eventDate: string | null;
  /** The club guidelines (one version) the set is planned for. */
  club?: ClubRef | null;
  notes: string;
  mode: SetlistMode;
  createdAt: number;
  updatedAt: number;
  entryCount: number;
}

export interface Setlist extends Omit<SetlistSummary, "entryCount"> {
  entries: SetlistEntry[];
}

export type EntryStatus = "pending" | "playing" | "played" | "skipped";

export interface SetlistEntry {
  id: Id;
  /** Absent = a style slot; the DJ picks the song live. */
  versionId?: Id | null;
  /** Your styles to show, in order: a subset of the version's, or a slot's styles. Absent = all of the version's. */
  styles?: string[] | null;
  /** In a club's set: club style ids to show (or a slot's style). Absent = what the song matches (see clubs.ts). */
  clubStyles?: string[] | null;
  /** Custom text shown instead of style names. */
  label?: string | null;
  overrides?: Overrides;
  note?: string;
  tags?: string[];
  /** "pause": no song, only a pause, an announcement and/or a stop. Absent = a song or style slot. */
  kind?: "song" | "pause";
  /** Auto mode stops before this entry: start it with Play Next. */
  hold?: boolean;
  /** Pause before this entry (before its announcement), overriding the default; a pause entry's length. */
  pauseBefore?: number | null;
  /** Pause between this entry's announcement and its start. */
  pauseAfter?: number | null;
  /** Said before this entry instead of a random sentence ({dance} allowed); "" = nothing. */
  announcement?: string | null;
  status: EntryStatus;
  playedAt?: number | null;
}

export type NewEntry = Partial<Pick<SetlistEntry, "versionId" | "styles" | "clubStyles" | "label" | "overrides" | "note" | "tags" | "kind" | "pauseBefore" | "announcement">>;
export type EntryPatch = Partial<
  Pick<SetlistEntry, "versionId" | "styles" | "clubStyles" | "label" | "overrides" | "note" | "tags" | "status" | "hold" | "pauseBefore" | "pauseAfter" | "announcement">
>;

/** A song played in a setlist (history is derived from setlists). */
export interface HistoryItem {
  playedAt: number;
  trackId: Id;
  versionId: Id | null;
  styles: string[];
  setlistId: Id;
  entryId: Id;
}

// ---------------------------------------------------------------- your styles

/** One of your dance styles (what songs are tagged with). */
export interface LibraryStyle {
  key: string;
  name: string;
  short: string;
  color: string;
  note?: string;
}

export interface LibraryTag {
  key: string;
  name: string;
  color: string;
}

/** styles.json: your styles and tags and, per club profile id, the mapping rules. */
export interface StyleLibrary {
  styles: LibraryStyle[];
  /** Per club id: "what I call `mine`, the club may call `club`" (same names need no rule). */
  clubRules?: Record<string, ClubRule[]>;
  tags?: LibraryTag[];
  /** How announcements say words: a respelling, or phonemes between slashes. */
  pronunciations?: Pronunciation[];
}

/** "What I call `mine` (your style key), the club may call `club` (its style id)." */
export interface ClubRule {
  mine: string;
  club: string;
}

export interface Pronunciation {
  word: string;
  say: string;
}

export interface StylesInfo extends StyleLibrary {
  /** Number of versions tagged with each style key. */
  usage: Record<string, number>;
  /** Number of versions carrying each of your tags. */
  tagUsage: Record<string, number>;
}

export interface LibraryInfo {
  libraryDir: string;
  dataDir: string;
  loadErrors: string[];
}

// ---------------------------------------------------------------- clubs

export type { Club } from "./clubs";

export interface ClubRef {
  id: string;
  version: string;
}

export interface ClubVersionInfo {
  id: string;
  /** Name you gave it ("" = by its import date). */
  name: string;
  imported: number;
  /** A URL, a file path, "edited", "example" or "new". */
  from: string;
  error: string | null;
}

export interface ClubInfo {
  id: string;
  name: string;
  description: string;
  url: string | null;
  /** Newest first. */
  versions: ClubVersionInfo[];
}

export interface ClubVersion {
  clubId: string;
  versionId: string;
  text: string;
  club: import("./clubs").Club | null;
  error: string | null;
}

export interface ClubImported {
  clubId: string;
  versionId: string;
  /** Identical to the latest version: nothing new stored. */
  unchanged: boolean;
}

/** A club style with its tempo ranges as [min, max] (for tempo fit displays). */
export interface StyleDef {
  key: string;
  name: string;
  short: string;
  color: string;
  /** Preferred tempo range [min, max] in BPM. */
  sweet: [number, number] | null;
  /** Acceptable tempo range. */
  extended: [number, number] | null;
  note?: string;
}

// ---------------------------------------------------------------- settings

export interface AudioSettings {
  masterDevice: string | null;
  masterChannels: [number, number];
  cueDevice: string | null;
  cueChannels: [number, number];
  bufferMs: number;
}

export interface ProjectorSettings {
  title: string;
  displayMode: "auto" | "live" | "preset";
  historyCount: number;
  /** Live layout: upcoming songs / slots to list (0 = only "Up next"). */
  upcomingCount: number;
  showNext: boolean;
  revealNextInGap: boolean;
  showArtist: boolean;
  showBpm: boolean;
  showClock: boolean;
  showArtwork: boolean;
  fontScale: number;
  theme: "dark" | "light";
}

export interface MidiControl {
  status: number;
  data1: number;
  action: string;
  kind: "button" | "hold" | "absolute" | "relative";
  invert?: boolean;
}

export interface MidiLed {
  status: number;
  data1: number;
  state: string;
  on: number;
  off: number;
}

export interface MidiMapping {
  id: string;
  name: string;
  builtin?: boolean;
  deviceMatch: string[];
  init: number[][];
  controls: MidiControl[];
  leds: MidiLed[];
}

export interface Settings {
  /** Shareable library folder (git/Dropbox friendly). null = inside the app data folder. */
  libraryDir: string | null;
  /** Club whose latest guidelines new sets use ("" = none). */
  defaultClub: string;
  libraryFolders: string[];
  audio: AudioSettings;
  gapSeconds: number | null;
  autoAdvance: boolean | null;
  manualNextFadeMs: number;
  fadeOutMs: number | null;
  keylock: boolean;
  /** Time-stretch engine. */
  stretchEngine: "signalsmith" | "soundtouch" | "bungee" | "rubberband";
  tempoRangePct: number;
  autoGain: boolean;
  targetLoudnessDb: number;
  trimSilence: boolean;
  confirmNextWhilePlaying: boolean;
  /** Load the up-next song onto the free deck automatically. */
  autoLoadNext: boolean;
  /** Ignore jog / CUE / jumps on the on-air deck while it plays. */
  protectOnAir: boolean;
  /** Switch headphones to the free deck when a deck goes on air. */
  headphonesFollowFreeDeck: boolean;
  /** The crossfader snaps to the center within this fraction of its travel. */
  crossfaderSnap: number;
  projector: ProjectorSettings;
  /** mappingId "" = pick automatically from the connected controller. */
  midi: { enabled: boolean; mappingId: string; customMappings: MidiMapping[] };
  spotify: { clientId: string; autoConnect: boolean };
  announce: AnnounceSettings;
}

/** Spoken announcements of the next dance in auto mode. */
export interface AnnounceSettings {
  enabled: boolean;
  /** `{dance}` is replaced by the next dance's name. */
  patterns: string[];
  pauseBeforeSecs: number;
  pauseAfterSecs: number;
  /** Kokoro voice, e.g. "af_heart". */
  voice: string;
  /** Kokoro model: "fp16" (~160 MB, default) or "fp32" (~325 MB). */
  model: "fp16" | "fp32";
  /** Speech rate: 1 = normal, >1 faster. */
  speed: number;
  volume: number;
}

export type TtsState =
  | { state: "missing" }
  | { state: "installing"; done: number; total: number; what: string }
  | { state: "ready" }
  | { state: "error"; message: string };

export type TtsStatus = TtsState & { voice: string; installedVoices: string[]; dir: string };

export interface VoiceInfo {
  key: string;
  name: string;
  language: string;
  gender: "female" | "male";
}

// ---------------------------------------------------------------- live state

export type DeckStatus = "empty" | "loading" | "paused" | "playing" | "ended" | "error";

export interface DeckState {
  deck: 0 | 1;
  status: DeckStatus;
  trackId: Id | null;
  /** The recording it plays (waveform, hot cues, artwork). */
  recordingId: Id | null;
  versionId: Id | null;
  entryId: Id | null;
  positionMs: number;
  durationMs: number;
  startMs: number;
  endMs: number | null;
  fadeAtMs: number | null;
  /** Main cue point (CUE button). */
  cueMs: number;
  /** The room can hear this deck (crossfader). */
  onAir: boolean;
  /** On/off per stem (vocals, drums, bass, other) when playing from stems. */
  stems: [boolean, boolean, boolean, boolean] | null;
  fading: boolean;
  /** Playback rate (1 = original tempo). */
  tempo: number;
  keylock: boolean;
  gainDb: number;
  trimDb: number;
  pfl: boolean;
  error: string | null;
}

export interface Levels {
  master: number;
  cue: number;
  masterVolume: number;
  cueVolume: number;
  /** Each deck's level (after its volume, before the crossfader). */
  decks: [number, number];
}

export interface PerformState {
  setlistId: Id | null;
  mode: SetlistMode;
  currentEntryId: Id | null;
  nextEntryId: Id | null;
  /** Epoch ms when the pause between songs began. */
  gapStartedAt: number | null;
  gapSeconds: number;
  autoAdvance: boolean;
  autoAdvanceAt: number | null;
  /** Auto mode: in the pause, speaking the announcement, or held before the next entry. */
  autoPhase: "waiting" | "announcing" | "held" | null;
  /** Play Next was pressed during a song; it starts when the fade completes. */
  pendingNext: boolean;
  /** Play Next pressed once while playing: press again to confirm. */
  armed: boolean;
  message: string | null;
  startedAt: number | null;
  /** 0 = deck A in the room, 1 = deck B. */
  crossfader: number;
  /** The deck the room heard last (the current song's deck). */
  liveDeck: 0 | 1;
}

export interface ProjTag {
  id: string;
  name: string;
  color: string;
}

export interface ProjEntry {
  id: Id;
  kind?: "song" | "pause";
  trackId: Id | null;
  /** For cover art. */
  recordingId?: Id | null;
  title: string;
  artist: string;
  /** Your style keys; the page shows club styles via `club` and `styleLibrary`. */
  styles: string[];
  clubStyles: string[] | null;
  /** For matching club styles. */
  meter: number | null;
  lengthSecs: number | null;
  /** Club styles that suggest this song. */
  clubListed: string[];
  label: string | null;
  tags: ProjTag[];
  status: EntryStatus;
  bpm: number | null;
}

export interface ProjectorPayload {
  title: string;
  mode: SetlistMode;
  settings: ProjectorSettings;
  current: ProjEntry | null;
  next: ProjEntry | null;
  history: ProjEntry[];
  entries: ProjEntry[];
  currentIndex: number | null;
  inGap: boolean;
  playing: boolean;
  message: string | null;
  /** Deck whose position drives the progress bar. */
  liveDeck: 0 | 1;
  /** The set's club guidelines (null: none). */
  club: import("./clubs").Club | null;
  clubId: string | null;
  /** Tempo change allowed when the club doesn't say (percent). */
  tempoRangePct: number;
  styleLibrary: StyleLibrary;
}

export interface ScanProgress {
  phase: string;
  done: number;
  total: number;
  added: number;
  relinked: number;
  removed: number;
  running: boolean;
}

export interface AudioDevice {
  id: string;
  name: string;
  channels: number;
  isDefault: boolean;
}

export interface OutputStatus {
  master: { device: string; sampleRate: number; channels: number } | null;
  cue: { device: string; sampleRate: number; channels: number } | null;
  cueOnMasterChannels: boolean;
  errors: string[];
}

export interface SpotifyStatus {
  connected: boolean;
  connecting: boolean;
  username: string | null;
  error: string | null;
  hasCachedCredentials: boolean;
  /** Search and playlists (Web API) are available; otherwise sign in again. */
  webApi: boolean;
  /** Search and playlists use your own Spotify app (Settings → Spotify). */
  ownWebApp: boolean;
}

export interface SpotifyTrack {
  uri: string;
  title: string;
  artist: string;
  album: string;
  durationMs: number;
  artworkUrl: string | null;
}

export interface SpotifyPlaylist {
  uri: string;
  name: string;
  owner: string;
  trackCount: number;
  artworkUrl: string | null;
}

export interface MidiInfo {
  ports: { name: string; connected: boolean }[];
  mappings: MidiMapping[];
  /** Mapping in use (with automatic selection: the one matching the controller). */
  activeMapping: string;
  actions: [string, string, string][];
  ledStates: string[];
}

export interface MonitorInfo {
  index: number;
  name: string;
  width: number;
  height: number;
  primary: boolean;
}

export const STEMS = ["vocals", "drums", "bass", "other"] as const;
export type StemName = (typeof STEMS)[number];

export type StemJob = { state: "queued" } | { state: "running"; percent: number } | { state: "failed"; message: string };

export interface StemsStatus {
  model:
    | { state: "missing" }
    | { state: "downloading"; done: number; total: number }
    | { state: "ready" }
    | { state: "error"; message: string };
  jobs: Record<Id, StemJob>;
  cacheBytes: number;
  cacheDir: string;
}

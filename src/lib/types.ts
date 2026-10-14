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

/** A playable configuration of a track: style, tempo, cue points, comments. */
export interface Version extends Overrides {
  id: Id;
  name: string;
  style?: string | null;
  comment?: string;
  tags?: string[];
}

export interface Track {
  id: Id;
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
  energy: number | null;
  rhythm: number | null;
  rating: number | null;
  familiar?: boolean;
  gainDb?: number;
  loudnessDb: number | null;
  audioStartMs: number | null;
  audioEndMs: number | null;
  notes?: string;
  analyzed: boolean;
  preferredSource?: "file" | "spotify" | null;
  artworkUrl?: string | null;
  addedAt: number;
  sources: SourceRef[];
  versions: Version[];
  aliases?: Id[];
  /** Saved cue points (pads 1–4), shared by all versions. */
  hotCues?: { slot: number; ms: number }[];
  /** false = used only in setlists (e.g. a Spotify song added straight to a set). Absent = true. */
  inLibrary?: boolean;
  /** Paths of this track's files on this machine. */
  files: string[];
  missing: boolean;
  playCount: number;
  lastPlayedAt: number | null;
}

export type TrackPatch = Partial<
  Pick<
    Track,
    | "title" | "artist" | "album" | "genre" | "bpm" | "bpmSource" | "energy" | "rhythm" | "rating"
    | "familiar" | "gainDb" | "notes" | "preferredSource"
  >
>;

export type VersionPatch = Partial<Omit<Version, "id">>;

/** A library row: one version of a track. */
export interface Row {
  track: Track;
  version: Version;
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
  profileId: string;
  lessonStyle: string | null;
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
  /** Slot style, or an override of the version's style. */
  style?: string | null;
  overrides?: Overrides;
  note?: string;
  tags?: string[];
  status: EntryStatus;
  playedAt?: number | null;
}

export type NewEntry = Partial<Pick<SetlistEntry, "versionId" | "style" | "overrides" | "note" | "tags">>;
export type EntryPatch = Partial<Pick<SetlistEntry, "versionId" | "style" | "overrides" | "note" | "tags" | "status">>;

export interface HistoryItem {
  playedAt: number;
  trackId: Id;
  versionId?: Id;
  style?: string;
  bpm?: number;
  setlistId?: Id;
}

export interface LibraryInfo {
  libraryDir: string;
  dataDir: string;
  loadErrors: string[];
}

// ---------------------------------------------------------------- profiles

export interface StyleDef {
  key: string;
  name: string;
  short: string;
  color: string;
  group: string | null;
  /** Preferred tempo range [min, max] in BPM. */
  sweet: [number, number] | null;
  /** Acceptable tempo range. */
  extended: [number, number] | null;
  beatsPerBar: number;
  lessonCategory: string;
  energy: "slow" | "medium" | "fast";
  note?: string;
}

export interface TagDef {
  id: string;
  name: string;
  color: string;
  /** Show on the projector. */
  announce: boolean;
}

export type CountSpec = number | Record<string, number>;
export type Severity = "error" | "warning" | "info";

/** Rule targets: a style key, `group:<id>`, or `@lesson`. */
export type Rule =
  | { type: "songCount"; min?: number; max?: number; severity?: Severity; label?: string }
  | { type: "maxDuration"; seconds: number; from?: number; to?: number; severity?: Severity; label?: string }
  | { type: "styleCount"; target: string; min?: CountSpec; max?: CountSpec; severity?: Severity; label?: string }
  | { type: "styleInFirst"; target: string; n: number; min: number; severity?: Severity; label?: string }
  | { type: "tagCount"; tag: string; target?: string; half?: "first" | "second"; min?: number; max?: number; severity?: Severity; label?: string }
  | {
      type: "tempoRange";
      range: "sweet" | "extended";
      maxExceptions?: number;
      notInFirst?: number;
      noConsecutive?: boolean;
      severity?: Severity;
      label?: string;
    }
  | {
      type: "endingSequence";
      sequence: string[];
      /** The first N of the sequence may appear in any order. */
      flexibleCount?: number;
      /** Targets allowed after the sequence (e.g. an optional closing routine). */
      allowAfter?: string[];
      severity?: Severity;
      label?: string;
    }
  | { type: "noRepeat"; severity?: Severity; label?: string }
  | { type: "noConsecutiveSame"; tempoPct?: number; severity?: Severity; label?: string }
  | { type: "maxConsecutive"; energy: "slow" | "medium" | "fast"; count: number; severity?: Severity; label?: string };

export interface Profile {
  id: string;
  name: string;
  builtin?: boolean;
  modified?: boolean;
  description: string;
  setMinutes: number;
  gap: { targetSeconds: number; minSeconds: number; maxSeconds: number };
  autoAdvance: boolean;
  fadeOutSeconds: number;
  tempo: { maxSpeedupPct: number; maxSlowdownPct: number };
  lessonCategories: { id: string; name: string }[];
  groups: { id: string; name: string }[];
  styles: StyleDef[];
  tags: TagDef[];
  announcements: string[];
  rules: Rule[];
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
  showNext: boolean;
  revealNextInGap: boolean;
  showArtist: boolean;
  showBpm: boolean;
  showClock: boolean;
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
  activeProfileId: string;
  libraryFolders: string[];
  audio: AudioSettings;
  gapSeconds: number | null;
  autoAdvance: boolean | null;
  manualNextFadeMs: number;
  fadeOutMs: number | null;
  keylock: boolean;
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
  projector: ProjectorSettings;
  midi: { enabled: boolean; mappingId: string; customMappings: MidiMapping[] };
  spotify: { clientId: string; autoConnect: boolean };
}

// ---------------------------------------------------------------- live state

export type DeckStatus = "empty" | "loading" | "paused" | "playing" | "ended" | "error";

export interface DeckState {
  deck: 0 | 1;
  status: DeckStatus;
  trackId: Id | null;
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
  title: string;
  artist: string;
  style: string | null;
  styleName: string;
  styleShort: string;
  color: string;
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

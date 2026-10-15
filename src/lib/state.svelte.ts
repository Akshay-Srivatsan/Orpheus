// Global reactive state for the main window (Svelte 5 runes).

import { ask } from "@tauri-apps/plugin-dialog";
import { api, on, type Events } from "./api";
import type {
  Crate, DeckState, Id, Levels, TtsStatus, LibraryInfo, StemsStatus, OutputStatus, PerformState, Profile, Row, ScanProgress, Setlist,
  SetlistEntry, SetlistSummary, Settings, SpotifyStatus, StyleDef, StylesInfo, Track, Version,
} from "./types";
import { NEUTRAL, clubStyle, clubStyleOfEntry, clubTargets, entryStyleKeys, myStyle, prettify, showStyle, showStyles, type Shown } from "./styles";
import type { ClubStyleOf } from "./tempo";
import type { SmartContext } from "./smart";

export type SourceView =
  | { kind: "all" }
  | { kind: "crate"; id: Id }
  | { kind: "setlist"; id: Id }
  | { kind: "history" }
  | { kind: "missing" }
  | { kind: "spotify-search" }
  | { kind: "spotify-playlist"; uri: string; name: string };

export interface Toast {
  id: number;
  text: string;
  kind: "error" | "info";
}

/** Style-filter value matching songs that have no dance style. */
export const NO_STYLE = "__none";

const emptyDeck = (deck: 0 | 1): DeckState => ({
  deck, status: "empty", trackId: null, versionId: null, entryId: null, positionMs: 0, durationMs: 0, startMs: 0,
  endMs: null, fadeAtMs: null, cueMs: 0, onAir: deck === 0, stems: null, fading: false, tempo: 1, keylock: true, gainDb: 0, trimDb: 0,
  pfl: deck === 1, error: null,
});

class AppState {
  tracks = $state<Track[]>([]);
  crates = $state<Crate[]>([]);
  setlists = $state<SetlistSummary[]>([]);
  profiles = $state<Profile[]>([]);
  /** Your dance styles and their mappings onto club styles (styles.json). */
  styleLib = $state<StylesInfo | null>(null);
  settings = $state<Settings | null>(null);
  decks = $state<[DeckState, DeckState]>([emptyDeck(0), emptyDeck(1)]);
  levels = $state<Levels>({ master: 0, cue: 0, masterVolume: 1, cueVolume: 0.8, decks: [0, 0] });
  perform = $state<PerformState | null>(null);
  /** The setlist being performed / edited in the set panel. */
  current = $state<Setlist | null>(null);
  /** A setlist being browsed in the library (not the current one). */
  browsed = $state<Setlist | null>(null);
  scan = $state<ScanProgress | null>(null);
  spotify = $state<SpotifyStatus | null>(null);
  outputs = $state<OutputStatus | null>(null);
  library = $state<LibraryInfo | null>(null);
  stems = $state<StemsStatus | null>(null);
  tts = $state<TtsStatus | null>(null);
  /** Tracks with prepared stems on this computer. */
  stemsReady = $state<Set<Id>>(new Set());
  toasts = $state<Toast[]>([]);

  // UI state
  view = $state<SourceView>({ kind: "all" });
  search = $state("");
  /**
   * Dance-style filter applied to every view (library, crates, setlists...).
   * Style keys; `NO_STYLE` matches songs without a style. Empty = no filter.
   */
  styleFilter = $state<string[]>([]);
  /** With a single style filtered, also list unlabelled songs whose tempo fits it. */
  suggestByTempo = $state(true);
  /** Selected library rows (version ids). */
  selected = $state<Id[]>([]);
  /** Selected set entries. */
  selectedEntries = $state<Id[]>([]);
  /** Track shown in the inspector. */
  inspectTrack = $state<Id | null>(null);
  /** Entry shown in the inspector (its per-setlist overrides). */
  inspectEntry = $state<Id | null>(null);
  rightTab = $state<"set" | "info">("set");
  settingsOpen = $state<false | string>(false);
  newSetlistOpen = $state(false);
  /** A style slot in the open set the DJ is choosing a song for. */
  fillingSlot = $state<Id | null>(null);
  /** Smart crate being edited: its id, or "new". */
  smartEditor = $state<string | false>(false);
  /** Bumped by MIDI browse actions; the library table reacts. */
  midiNav = $state<{ action: string; delta?: number; deck?: 0 | 1; n: number }>({ action: "", n: 0 });

  trackMap = $derived(new Map(this.tracks.map((t) => [t.id, t])));
  versionMap = $derived.by(() => {
    const m = new Map<Id, Row>();
    for (const t of this.tracks) for (const v of t.versions) m.set(v.id, { track: t, version: v });
    return m;
  });
  /** Rows in the library (excludes set-only songs, which are still in versionMap). */
  rows = $derived.by(() => {
    const out: Row[] = [];
    for (const t of this.tracks) if (t.inLibrary !== false) for (const v of t.versions) out.push({ track: t, version: v });
    return out;
  });
  libraryTracks = $derived(this.tracks.filter((t) => t.inLibrary !== false));
  profile = $derived.by((): Profile | null => {
    const id = this.current?.profileId ?? this.settings?.activeProfileId;
    return this.profiles.find((p) => p.id === id) ?? this.profiles[0] ?? null;
  });
  /** Your dance styles (what songs are tagged with). */
  myStyles = $derived(this.styleLib?.styles ?? []);
  /** Your tags (labels that aren't dance styles). */
  myTags = $derived(this.styleLib?.tags ?? []);

  tagName(key: string): string {
    return this.myTags.find((t) => t.key === key)?.name ?? prettify(key);
  }

  tagColor(key: string): string {
    return this.myTags.find((t) => t.key === key)?.color || NEUTRAL;
  }

  /** The current club has its own styles (otherwise yours are shown as they are). */
  hasClub = $derived((this.profile?.styles.length ?? 0) > 0);

  /** The club style (with tempo ranges) one of your styles counts as at `bpm`, at the current club. */
  style(key: string | null | undefined, bpm: number | null = null): StyleDef | undefined {
    return key ? (clubStyle(this.styleLib, this.profile, key, bpm) ?? undefined) : undefined;
  }

  /** How one of your styles is shown at the current club (its naming when it maps, else yours). */
  show(key: string, bpm: number | null = null): Shown {
    return showStyle(this.styleLib, this.profile, key, bpm);
  }

  /** Club styles one of your styles can count as at the current club. */
  clubTargets(key: string): StyleDef[] {
    return clubTargets(this.styleLib, this.profile, key);
  }

  /** One of your styles as you name it (ignoring the club). */
  mine(key: string): Shown {
    return showStyle(this.styleLib, null, key, null);
  }

  /** Your name for one of your styles. */
  styleName(key: string | null | undefined): string {
    if (!key) return "";
    return myStyle(this.styleLib, key)?.name ?? prettify(key);
  }

  styleColor(key: string | null | undefined): string {
    return (key && myStyle(this.styleLib, key)?.color) || NEUTRAL;
  }

  /** The styles an entry (or a version) shows at the current club, deduplicated. */
  shownFor(version: Version | undefined, entry: Partial<SetlistEntry> | null | undefined, bpm: number | null): Shown[] {
    return showStyles(this.styleLib, this.profile, entryStyleKeys(version, entry), bpm);
  }

  clubStyleOf: ClubStyleOf = (version, entry, bpm) => clubStyleOfEntry(this.styleLib, this.profile, version, entry, bpm);

  /** Tracks already played (or playing) in the open setlist. */
  playedInSet = $derived.by(() => {
    const out = new Set<Id>();
    for (const e of this.current?.entries ?? []) {
      if ((e.status === "played" || e.status === "playing") && e.versionId) {
        const t = this.versionMap.get(e.versionId)?.track.id;
        if (t) out.add(t);
      }
    }
    return out;
  });

  /** The single style being filtered on, if exactly one is. */
  filterStyle = $derived(this.styleFilter.length === 1 && this.styleFilter[0] !== NO_STYLE ? this.styleFilter[0] : null);

  /** The deck the room heard last, and the free (cueing) deck. */
  liveDeck = $derived<0 | 1>(this.perform?.liveDeck ?? 0);
  freeDeck = $derived<0 | 1>(this.liveDeck === 0 ? 1 : 0);

  fadeSeconds = $derived(this.settings?.fadeOutMs != null ? this.settings.fadeOutMs / 1000 : this.profile?.fadeOutSeconds ?? 5);

  /** Context for evaluating smart crates. */
  smartCtx = $derived<SmartContext>({
    profile: this.profile,
    styles: this.styleLib,
    crates: this.crates,
    library: this.rows,
    fadeSeconds: this.fadeSeconds,
    trimSilence: this.settings?.trimSilence ?? true,
  });

  private toastTimers = new Map<number, ReturnType<typeof setTimeout>>();

  toast(text: string, kind: Toast["kind"] = "error") {
    const ms = kind === "error" ? 6000 : 3500;
    // The same message again (e.g. from a controller held down): keep one, show it longer.
    const same = this.toasts.find((t) => t.text === text && t.kind === kind);
    const id = same?.id ?? Date.now() + Math.random();
    if (!same) this.toasts.push({ id, text, kind });
    clearTimeout(this.toastTimers.get(id));
    this.toastTimers.set(id, setTimeout(() => {
      this.toasts = this.toasts.filter((t) => t.id !== id);
      this.toastTimers.delete(id);
    }, ms));
  }

  /** Runs an API call, showing errors as toasts. */
  async run<T>(p: Promise<T>): Promise<T | undefined> {
    try {
      return await p;
    } catch (e) {
      this.toast(String(e));
      return undefined;
    }
  }

  async reloadTracks(ids?: Id[]) {
    if (ids && ids.length > 0 && ids.length < 20 && this.tracks.length > 0) {
      for (const id of ids) {
        try {
          const t = await api.track(id);
          const i = this.tracks.findIndex((x) => x.id === t.id);
          if (i >= 0) this.tracks[i] = t;
          else this.tracks.push(t);
        } catch {
          this.tracks = this.tracks.filter((x) => x.id !== id);
        }
      }
    } else {
      this.tracks = await api.tracks();
    }
  }

  async reloadCurrent() {
    const id = this.perform?.setlistId;
    this.current = id ? ((await this.run(api.setlist(id))) ?? null) : null;
  }

  async reloadBrowsed() {
    if (this.view.kind === "setlist") {
      const id = this.view.id;
      this.browsed = id === this.current?.id ? this.current : ((await this.run(api.setlist(id))) ?? null);
    } else {
      this.browsed = null;
    }
  }

  /** Event subscriptions; replaced (not duplicated) if `init` runs again, e.g. after a hot reload. */
  private unlisten: Promise<() => void>[] = [];

  private listen<K extends keyof Events>(event: K, cb: (payload: Events[K]) => void) {
    this.unlisten.push(on(event, cb));
  }

  async init() {
    for (const u of this.unlisten.splice(0)) u.then((f) => f());
    const [tracks, crates, setlists, profiles, settings, perform, decks, scan, spotify, outputs, library] = await Promise.all([
      api.tracks(), api.crates(), api.setlists(), api.profiles(), api.settings(), api.performState(),
      api.deckStates(), api.scanProgress(), api.spotifyStatus(), api.audioStatus(), api.libraryInfo(),
    ]);
    Object.assign(this, { tracks, crates, setlists, profiles, settings, perform, decks, scan, spotify, outputs, library });
    this.styleLib = await api.styles();
    await this.reloadCurrent();
    if (library.loadErrors.length) this.toast(`${library.loadErrors.length} library file(s) could not be read — see Settings → Library`);

    this.listen("deck-state", (d) => (this.decks = d));
    this.listen("levels", (l) => (this.levels = l));
    this.listen("perform-state", async (p) => {
      const changed = p.setlistId !== this.perform?.setlistId;
      this.perform = p;
      if (changed) await this.reloadCurrent();
    });
    this.listen("library-changed", async (ids) => {
      await this.reloadTracks(ids);
      // Style usage counts may have changed.
      this.styleLib = await api.styles();
    });
    this.listen("styles-changed", async () => (this.styleLib = await api.styles()));
    this.listen("setlist-changed", async (id) => {
      if (id === this.current?.id) await this.reloadCurrent();
      if (this.view.kind === "setlist" && this.view.id === id) await this.reloadBrowsed();
      this.setlists = await api.setlists();
    });
    this.listen("setlists-changed", async () => (this.setlists = await api.setlists()));
    this.listen("crates-changed", async () => (this.crates = await api.crates()));
    this.listen("profiles-changed", async () => (this.profiles = await api.profiles()));
    this.listen("settings-changed", (s) => (this.settings = s));
    this.listen("scan-progress", (s) => (this.scan = s));
    this.listen("spotify-status", (s) => (this.spotify = s));
    this.listen("output-status", (s) => (this.outputs = s));
    this.listen("toast", (t) => this.toast(t));
    this.listen("toast-info", (t) => this.toast(t, "info"));
    this.listen("open-settings", (tab) => (this.settingsOpen = tab));
    let lastJobs = "";
    this.listen("stems-status", async (s) => {
      this.stems = s;
      // Refresh the "has stems" set when a job finishes.
      const jobs = Object.keys(s.jobs).sort().join(",");
      if (jobs !== lastJobs) {
        lastJobs = jobs;
        this.stemsReady = new Set(await api.stemsReady());
      }
    });
    this.listen("tts-status", (t) => (this.tts = t));
    this.tts = await api.ttsStatus();
    this.stems = await api.stemsStatus();
    this.stemsReady = new Set(await api.stemsReady());
    this.listen("midi-ui", (m) => (this.midiNav = { ...m, n: this.midiNav.n + 1 }));
  }

  async saveSettings(patch: Partial<Settings>) {
    if (!this.settings) return;
    const next = { ...$state.snapshot(this.settings), ...patch } as Settings;
    this.settings = next;
    await this.run(api.saveSettings(next));
  }

  /** Entry lookup helpers for the current setlist. */
  entryRow(entryId: Id | null | undefined): { entry: Setlist["entries"][number]; row?: Row } | null {
    const e = this.current?.entries.find((x) => x.id === entryId);
    if (!e) return null;
    return { entry: e, row: e.versionId ? this.versionMap.get(e.versionId) : undefined };
  }

  versionLabel(v: Version, t: Track): string {
    return t.versions.length > 1 || v.name !== "Original" ? v.name : "";
  }
}

export const app = new AppState();

/** Adds versions to the current setlist (creating a live set if none is open). */
export async function addToSet(versionIds: Id[], where: "next" | "end" | number) {
  let set = app.current;
  if (!set) {
    const name = `Live set — ${new Date().toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" })}`;
    const created = await app.run(
      api.createSetlist({ name, mode: "live", profileId: app.settings?.activeProfileId ?? app.profile?.id ?? "" }),
    );
    if (!created) return;
    await app.run(api.performOpen(created.id));
    await app.reloadCurrent();
    set = app.current;
    if (!set) return;
  }
  let position: number | null = null;
  if (where === "next") {
    const cur = set.entries.findIndex((e) => e.id === app.perform?.currentEntryId);
    const lastDone = set.entries.findLastIndex((e) => e.status !== "pending");
    position = Math.max(cur, lastDone) + 1;
  } else if (typeof where === "number") {
    position = where;
  }
  // If a slot is selected and we're adding one song, fill the slot instead.
  const slot = set.entries.find((e) => app.selectedEntries.length === 1 && e.id === app.selectedEntries[0] && !e.versionId);
  if (slot && versionIds.length === 1 && where !== "end") {
    await app.run(api.updateEntry(slot.id, { versionId: versionIds[0] }));
    return;
  }
  const f = app.filterStyle;
  const entries = versionIds.map((id) => {
    const styles = app.versionMap.get(id)?.version.styles ?? [];
    // While filtering on a style, a song added is danced as that style
    // (an unlabelled one, or one that can be danced as several).
    const pick = f && (styles.length === 0 || (styles.length > 1 && styles.includes(f)));
    return { versionId: id, styles: pick ? [f] : undefined };
  });
  await app.run(api.insertEntries(set.id, entries, position));
}

/**
 * Makes Spotify songs playable and returns the default version id of each.
 * With `inLibrary: false` they are only used by setlists / the preview deck and
 * don't appear in the library (songs already in the library stay there).
 */
export async function spotifyVersions(tracks: import("./types").SpotifyTrack[], inLibrary: boolean): Promise<Id[]> {
  const ids = (await app.run(api.spotifyImport(tracks, inLibrary))) ?? [];
  await app.reloadTracks(ids);
  return ids.map((id) => app.trackMap.get(id)?.versions[0]?.id).filter(Boolean) as Id[];
}

/** Loads a version onto the free deck and plays it in the headphones. */
export async function audition(versionId: Id, entryId?: Id | null) {
  const row = app.versionMap.get(versionId);
  if (row?.track.missing) {
    app.toast(`"${row.track.title}" has no playable source on this computer.`);
    return;
  }
  await app.run(api.deckLoad(app.freeDeck, versionId, { entryId, play: true }));
}

/** Starts choosing a song for a style slot: the library shows that style. */
export function startFillingSlot(entry: SetlistEntry) {
  app.fillingSlot = entry.id;
  if (entry.styles?.length) app.styleFilter = [...entry.styles];
  app.selected = [];
}

/** Puts a song into the slot being filled (or `slotId`). */
export async function fillSlot(versionId: Id, slotId: Id | null = app.fillingSlot) {
  if (!slotId) return;
  const ok = await app.run(api.updateEntry(slotId, { versionId }));
  if (ok !== undefined && app.fillingSlot === slotId) app.fillingSlot = null;
}

/** Loads a version onto a specific deck (paused). */
export async function loadDeck(deck: 0 | 1, versionId: Id, entryId?: Id | null) {
  await app.run(api.deckLoad(deck, versionId, { entryId, play: false }));
}

/** Native confirmation dialog for destructive actions. */
export async function confirmAction(message: string, okLabel = "Delete"): Promise<boolean> {
  try {
    return await ask(message, { title: "Orpheus", kind: "warning", okLabel, cancelLabel: "Cancel" });
  } catch {
    return window.confirm(message);
  }
}

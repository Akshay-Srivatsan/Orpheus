// Global reactive state for the main window (Svelte 5 runes).

import { api, on } from "./api";
import type {
  Crate, DeckState, Id, Levels, LibraryInfo, OutputStatus, PerformState, Profile, Row, ScanProgress, Setlist,
  SetlistSummary, Settings, SpotifyStatus, StyleDef, Track, Version,
} from "./types";

export type SourceView =
  | { kind: "all" }
  | { kind: "style"; key: string }
  | { kind: "unstyled" }
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

const emptyDeck = (deck: 0 | 1): DeckState => ({
  deck, status: "empty", trackId: null, versionId: null, entryId: null, positionMs: 0, durationMs: 0, startMs: 0,
  endMs: null, fadeAtMs: null, fading: false, tempo: 1, keylock: true, gainDb: 0, trimDb: 0, pfl: deck === 1, error: null,
});

class AppState {
  tracks = $state<Track[]>([]);
  crates = $state<Crate[]>([]);
  setlists = $state<SetlistSummary[]>([]);
  profiles = $state<Profile[]>([]);
  settings = $state<Settings | null>(null);
  decks = $state<[DeckState, DeckState]>([emptyDeck(0), emptyDeck(1)]);
  levels = $state<Levels>({ master: 0, cue: 0, masterVolume: 1, cueVolume: 0.8 });
  perform = $state<PerformState | null>(null);
  /** The setlist being performed / edited in the set panel. */
  current = $state<Setlist | null>(null);
  /** A setlist being browsed in the library (not the current one). */
  browsed = $state<Setlist | null>(null);
  scan = $state<ScanProgress | null>(null);
  spotify = $state<SpotifyStatus | null>(null);
  outputs = $state<OutputStatus | null>(null);
  library = $state<LibraryInfo | null>(null);
  toasts = $state<Toast[]>([]);

  // UI state
  view = $state<SourceView>({ kind: "all" });
  search = $state("");
  /** Selected library rows (version ids). */
  selected = $state<Id[]>([]);
  /** Selected set entries. */
  selectedEntries = $state<Id[]>([]);
  /** Track shown in the inspector. */
  inspectTrack = $state<Id | null>(null);
  /** Entry shown in the inspector (its per-setlist overrides). */
  inspectEntry = $state<Id | null>(null);
  rightTab = $state<"set" | "checks" | "info">("set");
  settingsOpen = $state<false | string>(false);
  newSetlistOpen = $state(false);
  /** Bumped by MIDI browse actions; the library table reacts. */
  midiNav = $state<{ action: string; delta?: number; n: number }>({ action: "", n: 0 });

  trackMap = $derived(new Map(this.tracks.map((t) => [t.id, t])));
  versionMap = $derived.by(() => {
    const m = new Map<Id, Row>();
    for (const t of this.tracks) for (const v of t.versions) m.set(v.id, { track: t, version: v });
    return m;
  });
  rows = $derived.by(() => {
    const out: Row[] = [];
    for (const t of this.tracks) for (const v of t.versions) out.push({ track: t, version: v });
    return out;
  });
  profile = $derived.by((): Profile | null => {
    const id = this.current?.profileId ?? this.settings?.activeProfileId;
    return this.profiles.find((p) => p.id === id) ?? this.profiles[0] ?? null;
  });
  styleMap = $derived(new Map((this.profile?.styles ?? []).map((s) => [s.key, s])));

  style(key: string | null | undefined): StyleDef | undefined {
    return key ? this.styleMap.get(key) : undefined;
  }

  styleName(key: string | null | undefined): string {
    if (!key) return "";
    return this.styleMap.get(key)?.name ?? key.replace(/-/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
  }

  styleColor(key: string | null | undefined): string {
    return (key && this.styleMap.get(key)?.color) || "#8e8e93";
  }

  fadeSeconds = $derived(this.settings?.fadeOutMs != null ? this.settings.fadeOutMs / 1000 : this.profile?.fadeOutSeconds ?? 5);

  toast(text: string, kind: Toast["kind"] = "error") {
    const id = Date.now() + Math.random();
    this.toasts.push({ id, text, kind });
    setTimeout(() => (this.toasts = this.toasts.filter((t) => t.id !== id)), kind === "error" ? 6000 : 3500);
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

  async init() {
    const [tracks, crates, setlists, profiles, settings, perform, decks, scan, spotify, outputs, library] = await Promise.all([
      api.tracks(), api.crates(), api.setlists(), api.profiles(), api.settings(), api.performState(),
      api.deckStates(), api.scanProgress(), api.spotifyStatus(), api.audioStatus(), api.libraryInfo(),
    ]);
    Object.assign(this, { tracks, crates, setlists, profiles, settings, perform, decks, scan, spotify, outputs, library });
    await this.reloadCurrent();
    if (library.loadErrors.length) this.toast(`${library.loadErrors.length} library file(s) could not be read — see Settings → Library`);

    on("deck-state", (d) => (this.decks = d));
    on("levels", (l) => (this.levels = l));
    on("perform-state", async (p) => {
      const changed = p.setlistId !== this.perform?.setlistId;
      this.perform = p;
      if (changed) await this.reloadCurrent();
    });
    on("library-changed", (ids) => this.reloadTracks(ids));
    on("setlist-changed", async (id) => {
      if (id === this.current?.id) await this.reloadCurrent();
      if (this.view.kind === "setlist" && this.view.id === id) await this.reloadBrowsed();
      this.setlists = await api.setlists();
    });
    on("setlists-changed", async () => (this.setlists = await api.setlists()));
    on("crates-changed", async () => (this.crates = await api.crates()));
    on("profiles-changed", async () => (this.profiles = await api.profiles()));
    on("settings-changed", (s) => (this.settings = s));
    on("scan-progress", (s) => (this.scan = s));
    on("spotify-status", (s) => (this.spotify = s));
    on("output-status", (s) => (this.outputs = s));
    on("toast", (t) => this.toast(t));
    on("midi-ui", (m) => (this.midiNav = { ...m, n: this.midiNav.n + 1 }));
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
      api.createSetlist({ name, mode: "live", profileId: app.settings?.activeProfileId ?? "dancebreak" }),
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
  const entries = versionIds.map((id) => {
    const v = app.versionMap.get(id)?.version;
    return { versionId: id, style: v?.style ? undefined : app.view.kind === "style" ? app.view.key : undefined };
  });
  await app.run(api.insertEntries(set.id, entries, position));
}

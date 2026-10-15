// What styles a song shows: your own, or (in a club's set) the club's.
//
// Songs are tagged with *your* styles (styles.json in the library). Without a
// club they show as they are. In a club's set they show the club's styles
// they match, narrowed by your mapping or chosen per entry (see clubs.ts).
//
// The main window and the projector both use these helpers.

import { clubShown, type Assessment, type Club, type ClubStyle, type Fit, type Range, type SongFacts, type TempoChange } from "./clubs";
import { playedDurationMs, resolveSettings } from "./tempo";
import type { LibraryStyle, ProjEntry, ProjectorPayload, SetlistEntry, StyleDef, StyleLibrary, Track, Version } from "./types";

/** Colour for styles without one (and songs without a style). */
export const NEUTRAL = "#8e8e93";

export function myStyle(lib: StyleLibrary | null, key: string): LibraryStyle | undefined {
  return lib?.styles.find((s) => s.key === key);
}

export function prettify(key: string): string {
  return key.replace(/[-_]+/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
}

/** How a style looks. */
export interface Shown {
  /** Your style key, or the club style id (when `club` is set). */
  key: string;
  name: string;
  short: string;
  color: string;
  /** The club style (null without a club). */
  club: StyleDef | null;
  /** In a club's set: not one of the club's styles (shown in italics). */
  unmapped: boolean;
  /** How the song fits this club style (null without a club or for a slot). */
  fit: Fit | null;
  /** Why the song doesn't fit (the DJ chose it anyway): warnings. */
  problems: string[];
  /** Playback rate the style needs (1 = as is). */
  rate: number;
  /** Shown because the song matches it (no mapping or choice narrowed it down). */
  auto: boolean;
}

function pair(r: Range | null): [number, number] | null {
  if (!r || (r.min == null && r.max == null)) return null;
  return [r.min ?? 0, r.max ?? Infinity];
}

/** A club style with its tempo ranges as [min, max] pairs. */
export function styleDef(id: string, s: ClubStyle): StyleDef {
  return {
    key: id,
    name: s.name,
    short: s.short || s.name,
    color: s.color || NEUTRAL,
    sweet: pair(s.preferred),
    extended: pair(s.tempo),
    note: s.note ?? undefined,
  };
}

/** One of your styles as you name it. */
export function mineShown(lib: StyleLibrary | null, key: string): Shown {
  const mine = myStyle(lib, key);
  return {
    key,
    name: mine?.name ?? prettify(key),
    short: mine?.short || mine?.name || prettify(key),
    color: mine?.color || NEUTRAL,
    club: null,
    unmapped: false,
    fit: null,
    problems: [],
    rate: 1,
    auto: false,
  };
}

/** A club style as shown, with how the song fits it. */
export function clubStyleShown(club: Club, id: string, fit: Assessment | undefined): Shown {
  const s = club.style[id];
  const def = styleDef(id, s);
  return { key: id, name: def.name, short: def.short, color: def.color, club: def, unmapped: false, fit: fit?.fit ?? null, problems: fit?.problems ?? [], rate: fit?.rate ?? 1, auto: false };
}

/** Names are the same when they differ only in case and spacing. */
export const sameName = (a: string, b: string) => a.trim().replace(/\s+/g, " ").toLowerCase() === b.trim().replace(/\s+/g, " ").toLowerCase();

/** The club styles each of your styles may be called at a club: your rules, and those with the same name. */
export function clubNames(club: Club, clubId: string | null | undefined, lib: StyleLibrary | null): Record<string, string[]> {
  const out: Record<string, string[]> = {};
  const add = (mine: string, id: string) => {
    const ids = (out[mine] ??= []);
    if (id in club.style && !ids.includes(id)) ids.push(id);
  };
  for (const r of (clubId && lib?.clubRules?.[clubId]) || []) add(r.mine, r.club);
  for (const mine of lib?.styles ?? []) {
    for (const [id, s] of Object.entries(club.style)) if (sameName(styleDef(id, s).name, mine.name)) add(mine.key, id);
  }
  return out;
}

/** The club a set is planned with, and what matching needs alongside it. */
export interface ClubContext {
  club: Club;
  /** Your styles' names at this club (your style key → club style ids): rules and same names. */
  mapping: Record<string, string[]>;
  tempoChange: TempoChange;
}

export function clubContext(club: Club | null, clubId: string | null | undefined, lib: StyleLibrary | null, defaultChangePct: number): ClubContext | null {
  if (!club) return null;
  return {
    club,
    mapping: clubNames(club, clubId, lib),
    tempoChange: club.set.tempo_change ?? { faster: defaultChangePct, slower: defaultChangePct },
  };
}

/** Your style keys an entry shows: its own selection, else all of the version's. */
export function entryStyleKeys(version: Version | undefined, entry: Partial<SetlistEntry> | null | undefined): string[] {
  return entry?.styles ?? version?.styles ?? [];
}

/** What matching needs to know about a song (null: no song, e.g. a slot). */
export function songFacts(
  track: Track | undefined,
  version: Version | undefined,
  entry: Partial<SetlistEntry> | null | undefined,
  opts: { fadeSeconds?: number; trimSilence?: boolean } = {},
  listed: string[] = [],
): SongFacts | null {
  if (!track) return null;
  return {
    listed,
    bpm: track.bpm,
    target: resolveSettings(version, entry).targetBpm ?? null,
    meter: track.beatsPerBar ?? null,
    meterDetected: track.meterSource !== "user",
    lengthSecs: track.durationMs ? playedDurationMs(track, version, entry ?? null, 1, opts) / 1000 : null,
  };
}

/** The styles an entry (or a version) shows: the club's in a club's set, else yours. */
export function shownStyles(
  lib: StyleLibrary | null,
  ctx: ClubContext | null,
  facts: SongFacts | null,
  version: Version | undefined,
  entry: Partial<SetlistEntry> | null | undefined,
): Shown[] {
  const keys = entryStyleKeys(version, entry);
  if (!ctx) return keys.map((k) => mineShown(lib, k));
  const r = clubShown(ctx.club, facts, ctx.tempoChange, entry?.clubStyles, keys, ctx.mapping);
  return r.ids.map((id) => ({ ...clubStyleShown(ctx.club, id, r.fits[id]), auto: r.source === "matches" }));
}

/** A custom label replacing style names: the entry's, else the version's display name. */
export function customLabel(version: Version | undefined, entry: Partial<SetlistEntry> | null | undefined): string | null {
  return entry?.label || version?.displayName || null;
}

/** The text dancers see: a custom label, else the shown styles joined. */
export function labelOf(shown: Shown[], custom: string | null, sep = " / "): string {
  return custom || shown.map((s) => s.name).join(sep);
}

/** What the projector shows for an entry: the club's style names (or the entry's custom label). */
export function projStyle(p: Pick<ProjectorPayload, "club" | "clubId" | "styleLibrary" | "tempoRangePct">, e: ProjEntry): { name: string; short: string; color: string } {
  const ctx = clubContext(p.club, p.clubId, p.styleLibrary, p.tempoRangePct);
  const facts: SongFacts | null = e.trackId ? { bpm: e.bpm, target: null, meter: e.meter, meterDetected: true, lengthSecs: e.lengthSecs, listed: e.clubListed } : null;
  const shown = shownStyles(p.styleLibrary, ctx, facts, undefined, { styles: e.styles, clubStyles: e.clubStyles });
  return {
    name: e.label || shown.map((s) => s.name).join(" / "),
    short: e.label || shown.map((s) => s.short).join(" / "),
    color: shown[0]?.color ?? NEUTRAL,
  };
}

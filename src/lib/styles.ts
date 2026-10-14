// Your dance styles vs. a club's styles.
//
// Songs are tagged with *your* styles (styles.json in the library). A club
// profile has its own style list with tempo ranges; for each club, ordered
// rules map your styles (optionally by tempo) onto the club's. The first rule
// that matches wins; without a matching rule a club style with the same key or
// name is used. The "no club" profile (no styles) shows your styles as they are.
//
// This module is the only implementation of the mapping: the main window and
// the projector both use it.

import { foldBpm } from "./tempo";

/** Colour for styles without one (and songs without a style). */
export const NEUTRAL = "#8e8e93";
import type { LibraryStyle, Profile, ProjEntry, ProjectorPayload, SetlistEntry, StyleDef, StyleLibrary, StyleMappingRule, Version } from "./types";

/** Club style for one of your styles at a given (planned) tempo; `null` = not a style at this club. */
export function clubStyle(lib: StyleLibrary | null, profile: Profile | null, key: string, bpm: number | null): StyleDef | null {
  if (!profile || profile.styles.length === 0) return null;
  for (const r of lib?.mappings[profile.id] ?? []) {
    if (!ruleMatches(r, key, bpm)) continue;
    return r.to ? (profile.styles.find((s) => s.key === r.to) ?? null) : null;
  }
  const byKey = profile.styles.find((s) => s.key === key);
  if (byKey) return byKey;
  const name = myStyle(lib, key)?.name.toLowerCase();
  return (name && profile.styles.find((s) => s.name.toLowerCase() === name)) || null;
}

/** Whether a rule applies. A range with both ends is matched in its own counting (half / double time folded). */
export function ruleMatches(r: StyleMappingRule, key: string, bpm: number | null): boolean {
  if (r.style !== key) return false;
  if (r.minBpm == null && r.maxBpm == null) return true;
  if (bpm == null) return false;
  const b = r.minBpm != null && r.maxBpm != null ? foldBpm(bpm, (r.minBpm + r.maxBpm) / 2) : bpm;
  return (r.minBpm == null || b >= r.minBpm) && (r.maxBpm == null || b <= r.maxBpm);
}

export function myStyle(lib: StyleLibrary | null, key: string): LibraryStyle | undefined {
  return lib?.styles.find((s) => s.key === key);
}

export function prettify(key: string): string {
  return key.replace(/[-_]+/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
}

/** How a style looks: the club's naming when it maps, else your own. */
export interface Shown {
  key: string;
  name: string;
  short: string;
  color: string;
  /** The club style it maps to (null under "no club" or when not a club style). */
  club: StyleDef | null;
  /** True when a club is selected but this style isn't one of its styles. */
  unmapped: boolean;
}

export function showStyle(lib: StyleLibrary | null, profile: Profile | null, key: string, bpm: number | null): Shown {
  const club = clubStyle(lib, profile, key, bpm);
  const mine = myStyle(lib, key);
  const own = { name: mine?.name ?? prettify(key), short: mine?.short || mine?.name || prettify(key), color: mine?.color ?? NEUTRAL };
  return club
    ? { key, name: club.name, short: club.short || club.name, color: club.color, club, unmapped: false }
    : { key, ...own, club: null, unmapped: !!profile && profile.styles.length > 0 };
}

/** Your style keys an entry shows: its own selection, else all of the version's. */
export function entryStyleKeys(version: Version | undefined, entry: Partial<SetlistEntry> | null | undefined): string[] {
  return entry?.styles ?? version?.styles ?? [];
}

/** Styles as shown to dancers, deduplicated by club style (e.g. two of your waltzes both → "Waltz"). */
export function showStyles(lib: StyleLibrary | null, profile: Profile | null, keys: string[], bpm: number | null): Shown[] {
  const out: Shown[] = [];
  for (const k of keys) {
    const s = showStyle(lib, profile, k, bpm);
    const id = s.club ? `club:${s.club.key}` : `mine:${k}`;
    if (!out.some((o) => (o.club ? `club:${o.club.key}` : `mine:${o.key}`) === id)) out.push(s);
  }
  return out;
}

/** A custom label replacing style names: the entry's, else the version's display name. */
export function customLabel(version: Version | undefined, entry: Partial<SetlistEntry> | null | undefined): string | null {
  return entry?.label || version?.displayName || null;
}

/** The text dancers see for an entry: a custom label, else the shown styles joined. */
export function entryLabel(lib: StyleLibrary | null, profile: Profile | null, version: Version | undefined, entry: Partial<SetlistEntry> | null | undefined, bpm: number | null): string {
  const custom = customLabel(version, entry);
  if (custom) return custom;
  return showStyles(lib, profile, entryStyleKeys(version, entry), bpm).map((s) => s.name).join(" / ");
}

/** An entry's dance as spoken in announcements: like the projector (the
 * club's names; styles it doesn't dance left out), alternatives joined by "or". */
export function spokenName(lib: StyleLibrary | null, profile: Profile | null, version: Version | undefined, entry: Partial<SetlistEntry> | null | undefined, bpm: number | null): string {
  const custom = customLabel(version, entry);
  if (custom) return custom;
  const all = showStyles(lib, profile, entryStyleKeys(version, entry), bpm);
  const shown = all.some((s) => !s.unmapped) ? all.filter((s) => !s.unmapped) : all;
  return shown.map((s) => s.name).join(" or ");
}

/** The first club style an entry / version is danced as at `bpm` (for tempo ranges). */
export function clubStyleOfEntry(lib: StyleLibrary | null, profile: Profile | null, version: Version | undefined, entry: Partial<SetlistEntry> | null | undefined, bpm: number | null): StyleDef | null {
  for (const k of entryStyleKeys(version, entry)) {
    const c = clubStyle(lib, profile, k, bpm);
    if (c) return c;
  }
  return null;
}

/** Every club style one of your styles can count as (any tempo), in rule order. */
export function clubTargets(lib: StyleLibrary | null, profile: Profile | null, key: string): StyleDef[] {
  if (!profile || profile.styles.length === 0) return [];
  const out: StyleDef[] = [];
  const add = (s: StyleDef | null | undefined) => s && !out.includes(s) && out.push(s);
  const rules = (lib?.mappings[profile.id] ?? []).filter((r) => r.style === key);
  for (const r of rules) add(profile.styles.find((s) => s.key === r.to));
  // The fallback applies unless an unconditional rule always catches this style first.
  if (!rules.some((r) => r.minBpm == null && r.maxBpm == null)) add(clubStyle(lib, profile, key, null));
  return out;
}

/** What the projector shows for an entry: the club's style names (or the entry's custom label). */
export function projStyle(p: Pick<ProjectorPayload, "profile" | "styleLibrary">, e: ProjEntry): { name: string; short: string; color: string } {
  const all = showStyles(p.styleLibrary, p.profile, e.styles, e.bpm);
  // Styles the club doesn't dance are left out, unless nothing maps.
  const shown = all.some((s) => !s.unmapped) ? all.filter((s) => !s.unmapped) : all;
  return {
    name: e.label || shown.map((s) => s.name).join(" / "),
    short: e.label || shown.map((s) => s.short).join(" / "),
    color: shown[0]?.color ?? NEUTRAL,
  };
}

// Club guidelines: which of a club's styles a song is, what a song in a
// club's set shows, and whether a set follows the club's guidelines.
//
// A club defines its styles by meter, tempo and length (see `club.rs` and
// examples/clubs/). A song *matches* a style when its meter fits and it can be
// played within the style's tempo (directly, at half or double time, or by
// changing its tempo as much as the club allows) at a fitting length.
//
// What a song shows in a club's set, in order of precedence:
// 1. the club styles chosen for that entry (the DJ's choice: any style, even
//    one the song doesn't match, with a warning);
// 2. the styles the club suggests this very song for (its listed songs, found
//    by fingerprint, Spotify track or file hash);
// 3. the club styles it matches. When it matches several, your styles pick
//    which of them show by default: those your styles on the song are called
//    at the club (your rules in Settings → Styles & Tags, or the same name),
//    or all of them if none is. This never adds a style the song doesn't
//    match, and doesn't matter when only one matches.
//
// This module is the only implementation: the main window and the projector
// both use it.

import { foldBpm } from "./tempo";

export interface Range {
  min: number | null;
  max: number | null;
}

export interface ClubStyle {
  name: string;
  short?: string | null;
  color?: string | null;
  meter: number[];
  tempo: Range | null;
  preferred: Range | null;
  length: Range | null;
  count: Range | null;
  share: Range | null;
  when: Range[];
  first: boolean;
  last: boolean;
  max_in_a_row?: number | null;
  note?: string | null;
  /** Songs the club suggests for this style (often particular edits), written in the club file. */
  song?: ClubSong[];
  /** Ids of songs suggested from the club's song libraries … */
  suggested?: string[];
  /** … and those songs, as fetched with this version. */
  library_songs?: ClubSong[];
}

/** All the songs a style suggests: its own, then those from song libraries (the backend's order). */
export function allSongs(s: ClubStyle): ClubSong[] {
  return [...(s.song ?? []), ...(s.library_songs ?? [])];
}

/** A song a club suggests; any of fingerprint, spotify and hash identifies it. */
export interface ClubSong {
  title?: string | null;
  artist?: string | null;
  length?: number | null;
  fingerprint?: string | null;
  spotify?: string | null;
  hash?: string | null;
  download?: string[];
  note?: string | null;
  /** Its id in a song library (library songs only). */
  id?: string | null;
}

/** The style says what fits it (meter, tempo or length); without, only its suggested songs do (e.g. a choreography). */
export function hasCriteria(s: ClubStyle): boolean {
  return s.meter.length > 0 || !!s.tempo || !!s.preferred || !!s.length;
}

export interface ClubGroup {
  name: string;
  styles: string[];
  count: Range | null;
  share: Range | null;
  when: Range[];
  max_in_a_row?: number | null;
  note?: string | null;
}

export interface ClubRule {
  name: string;
  songs: Range | null;
  when: Range[];
  length: Range | null;
  styles: string[];
  count: Range | null;
  note?: string | null;
}

export interface ClubTag {
  name: string;
  color?: string | null;
  announce: boolean;
  note?: string | null;
}

export interface ClubSet {
  minutes: Range | null;
  songs: Range | null;
  pause: { target: number | null; min: number | null; max: number | null };
  fade_out: number | null;
  tempo_change: { faster: number; slower: number } | null;
  auto_advance: boolean | null;
  max_in_a_row: number | null;
}

/** One version of a club's guidelines (the file's own keys). */
export interface Club {
  name: string;
  description: string;
  website?: string | null;
  url?: string | null;
  set: ClubSet;
  style: Record<string, ClubStyle>;
  group: Record<string, ClubGroup>;
  rule: Record<string, ClubRule>;
  tag: Record<string, ClubTag>;
  announcements: string[];
  /** Song libraries its styles suggest songs from (paths or web addresses). */
  song_library?: string[];
}

/** A new, empty club (for the editor). */
export function emptyClub(name: string): Club {
  return {
    name,
    description: "",
    set: { minutes: null, songs: null, pause: { target: null, min: null, max: null }, fade_out: null, tempo_change: null, auto_advance: null, max_in_a_row: null },
    style: {},
    group: {},
    rule: {},
    tag: {},
    announcements: [],
  };
}

// ---------------------------------------------------------------- ranges

export function inRange(v: number, r: Range | null | undefined, slack = 0): boolean {
  if (!r) return true;
  return (r.min == null || v >= r.min - slack) && (r.max == null || v <= r.max + slack);
}

/** "120", "28–34", "at least 3", "at most 4:30" (with `fmt` for the numbers). */
export function rangeText(r: Range | null | undefined, fmt: (v: number) => string = (v) => String(round1(v))): string {
  if (!r) return "";
  if (r.min != null && r.max != null) return r.min === r.max ? fmt(r.min) : `${fmt(r.min)}–${fmt(r.max)}`;
  if (r.min != null) return `at least ${fmt(r.min)}`;
  if (r.max != null) return `at most ${fmt(r.max)}`;
  return "";
}

function round1(v: number): number {
  return Math.round(v * 10) / 10;
}

function mid(r: Range): number | null {
  if (r.min != null && r.max != null) return (r.min + r.max) / 2;
  return r.min ?? r.max;
}

export function fmtSecs(s: number): string {
  const t = Math.round(s);
  return `${Math.floor(t / 60)}:${String(t % 60).padStart(2, "0")}`;
}

const pct = (v: number) => `${round1(v)}%`;

// ---------------------------------------------------------------- matching

/** What matching needs to know about a song. */
export interface SongFacts {
  /** Tempo as recorded (in the song's own counting). */
  bpm: number | null;
  /** Tempo the DJ set for it (then it isn't changed further). */
  target: number | null;
  /** Beats per bar, and whether it was detected (vs set by hand). */
  meter: number | null;
  meterDetected: boolean;
  /** Playing length at its own tempo (seconds). */
  lengthSecs: number | null;
  /** Club styles that suggest this song (it's one of their listed songs). */
  listed?: string[];
}

/** How much the club allows a song's tempo to change (percent). */
export interface TempoChange {
  faster: number;
  slower: number;
}

/** listed: a song the club suggests for the style; preferred: in the
 * preferred tempo; range: in the allowed tempo; stretch: needs a tempo change
 * (within what the club allows); unchecked: the style has no tempo, or the
 * song's tempo is unknown. */
export type Fit = "listed" | "preferred" | "range" | "stretch" | "unchecked";

export interface Assessment {
  id: string;
  /** The song fits the style (possibly with a tempo change). */
  ok: boolean;
  fit: Fit;
  /** Playback rate needed (1 = as is). */
  rate: number;
  /** Tempo it would be played at, in the style's counting. */
  bpm: number | null;
  /** Why it doesn't fit (or what to watch for): shown as warnings. */
  problems: string[];
}

/** The detector tells only triple (3) from duple (4): a detected 4 may be 2/4. */
function meterFits(style: ClubStyle, f: SongFacts): boolean | null {
  if (!style.meter.length) return true;
  if (f.meter == null) return null;
  if (style.meter.includes(f.meter)) return true;
  return f.meterDetected && f.meter === 4 && style.meter.includes(2);
}

const meterName = (m: number) => (m === 3 ? "triple (3)" : m === 2 ? "2/4" : m === 4 ? "4/4" : `${m} beats a bar`);

export function assess(id: string, style: ClubStyle, f: SongFacts, tc: TempoChange): Assessment {
  // The club suggests this song for the style: it fits, whatever its tempo.
  if (f.listed?.includes(id)) return { id, ok: true, fit: "listed", rate: 1, bpm: null, problems: [] };
  // A style danced to particular songs only (no meter, tempo or length to go by).
  if (allSongs(style).length && !hasCriteria(style)) {
    return { id, ok: false, fit: "unchecked", rate: 1, bpm: null, problems: [`not one of the songs suggested for ${style.name}`] };
  }
  const problems: string[] = [];
  let fit: Fit = "unchecked";
  let rate = 1;
  let bpm: number | null = null;

  const m = meterFits(style, f);
  if (m === false) problems.push(`${meterName(f.meter!)} meter; ${style.name} is ${style.meter.map(meterName).join(" or ")}`);

  const allowed = style.tempo ?? style.preferred;
  const center = allowed ? mid(allowed) : null;
  const own = f.target ?? f.bpm;
  if (allowed && center != null && own != null) {
    const base = foldBpm(own, center);
    bpm = round1(base);
    if (style.preferred && inRange(base, style.preferred, 0.5)) fit = "preferred";
    else if (inRange(base, allowed, 0.5)) fit = "range";
    else {
      const edge = allowed.min != null && base < allowed.min ? allowed.min : (allowed.max ?? base);
      const need = edge / base;
      const change = (need - 1) * 100;
      const limit = change > 0 ? tc.faster : tc.slower;
      if (f.target == null && Math.abs(change) <= limit + 0.05) {
        fit = "stretch";
        rate = need;
        bpm = round1(edge);
      } else {
        problems.push(
          f.target != null
            ? `set to ${round1(base)} BPM; ${style.name} is ${rangeText(allowed)} BPM`
            : `${round1(base)} BPM: needs ${change > 0 ? "+" : "−"}${Math.abs(round1(change))}% for ${style.name} (${rangeText(allowed)}), more than the ${limit}% allowed`,
        );
      }
    }
  }

  if (style.length && f.lengthSecs != null) {
    const len = f.lengthSecs / rate;
    if (!inRange(len, style.length, 1)) problems.push(`${fmtSecs(len)} long; ${style.name}: ${rangeText(style.length, fmtSecs)}`);
  }
  return { id, ok: problems.length === 0, fit, rate, bpm, problems };
}

function score(a: Assessment, f: SongFacts, style: ClubStyle): number {
  const base = { listed: -1, preferred: 0, range: 1, unchecked: 1.5, stretch: 2 }[a.fit];
  const meterUnknown = style.meter.length > 0 && f.meter == null ? 0.5 : 0;
  return base + Math.abs(a.rate - 1) * 10 + meterUnknown;
}

/** Club styles a song matches, best fit first: those that suggest it, and
 * those whose tempo it fits. Other styles without a tempo (e.g. a Mixer) match
 * only by your mapping or choice: they'd otherwise match every song. */
export function matches(club: Club, f: SongFacts, tc: TempoChange): Assessment[] {
  return Object.entries(club.style)
    .filter(([id, s]) => f.listed?.includes(id) || s.tempo || s.preferred)
    .map(([id, s]) => ({ a: assess(id, s, f, tc), s }))
    .filter(({ a }) => a.ok)
    .sort((x, y) => score(x.a, f, x.s) - score(y.a, f, y.s))
    .map(({ a }) => a);
}

export type ShownSource = "chosen" | "listed" | "mapping" | "matches" | "none";

export interface ClubShown {
  /** Club style ids to show, in order. */
  ids: string[];
  /** How each shown style fits (problems = warnings). */
  fits: Record<string, Assessment>;
  source: ShownSource;
}

/**
 * The club styles a song (or slot) shows.
 * @param chosen the entry's chosen club styles (`clubStyles`), if any
 * @param myKeys your styles on the song
 * @param mapping what your styles are called at this club: your style key → club style ids
 */
export function clubShown(
  club: Club,
  f: SongFacts | null,
  tc: TempoChange,
  chosen: string[] | null | undefined,
  myKeys: string[],
  mapping: Record<string, string[]> | undefined,
): ClubShown {
  const known = (ids: string[]) => ids.filter((id) => id in club.style);
  const fitsOf = (ids: string[]) => Object.fromEntries(ids.map((id) => [id, f ? assess(id, club.style[id], f, tc) : unchecked(id)]));
  if (chosen && chosen.length) {
    const ids = known(chosen);
    return { ids, fits: fitsOf(ids), source: "chosen" };
  }
  if (!f) return { ids: [], fits: {}, source: "none" };
  // The club lists this song for some styles: it is those (e.g. a line
  // dance's edit), even if its tempo would fit others too.
  const listed = known(f.listed ?? []);
  if (listed.length) return { ids: listed, fits: fitsOf(listed), source: "listed" };
  const all = matches(club, f, tc);
  const ticked = known([...new Set(myKeys.flatMap((k) => mapping?.[k] ?? []))]);
  const hit = all.length > 1 ? all.filter((a) => ticked.includes(a.id)) : [];
  const shown = hit.length ? hit : all;
  return {
    ids: shown.map((a) => a.id),
    fits: Object.fromEntries(shown.map((a) => [a.id, a])),
    source: !shown.length ? "none" : hit.length ? "mapping" : "matches",
  };
}

function unchecked(id: string): Assessment {
  return { id, ok: true, fit: "unchecked", rate: 1, bpm: null, problems: [] };
}

// ---------------------------------------------------------------- set check

/** One song of a set, as the check sees it. */
export interface SetItem {
  entryId: string;
  /** For messages ("Polka" / the song title). */
  title: string;
  /** Club styles it shows. */
  ids: string[];
  fits: Record<string, Assessment>;
  /** Playing length (seconds) at its playing tempo, if known. */
  lengthSecs: number | null;
  /** Pause before it (seconds). */
  pauseSecs: number;
}

export interface Issue {
  /** warn: against the guidelines; note: worth a look (e.g. not finished yet). */
  level: "warn" | "note";
  text: string;
  entries: string[];
}

/** A count the guidelines ask for, and how the set stands. */
export interface Tally {
  key: string;
  name: string;
  have: number;
  want: string;
  ok: boolean;
}

export interface SetCheck {
  issues: Issue[];
  tallies: Tally[];
  songs: number;
  minutes: number;
}

/** Checks a set (songs in order; pauses and skipped songs left out) against a club's guidelines. */
export function checkSet(club: Club, items: SetItem[], opts: { complete?: boolean } = {}): SetCheck {
  const issues: Issue[] = [];
  const tallies: Tally[] = [];
  const n = items.length;
  const name = (id: string) => club.style[id]?.name ?? id;
  const where = (i: number) => (n > 1 ? (i / (n - 1)) * 100 : 0);
  const totalSecs = items.reduce((t, it) => t + (it.lengthSecs ?? 0) + it.pauseSecs, 0);
  const minutes = totalSecs / 60;

  // Songs the guidelines can't place, and fits the DJ overrode.
  for (const it of items) {
    if (!it.ids.length) issues.push({ level: "warn", text: `${it.title}: not one of ${club.name}'s styles`, entries: [it.entryId] });
    for (const id of it.ids) for (const p of it.fits[id]?.problems ?? []) issues.push({ level: "warn", text: `${it.title}: ${p}`, entries: [it.entryId] });
  }

  const tally = (key: string, label: string, have: number, r: Range | null, fmt?: (v: number) => string) => {
    if (!r) return;
    const ok = inRange(have, r);
    tallies.push({ key, name: label, have, want: rangeText(r, fmt), ok });
    // Too few is only a note while the set may still grow.
    if (!ok) issues.push({ level: r.max != null && have > r.max ? "warn" : opts.complete ? "warn" : "note", text: `${label}: ${fmt ? fmt(have) : have}, ${club.name} expects ${rangeText(r, fmt)}`, entries: [] });
  };
  tally("songs", "Songs", n, club.set.songs);
  tally("minutes", "Minutes", Math.round(minutes), club.set.minutes);

  const shows = (it: SetItem, ids: string[]) => it.ids.some((id) => ids.includes(id));
  const counted = (ids: string[], label: string, key: string, count: Range | null, share: Range | null) => {
    const hit = items.filter((it) => shows(it, ids));
    tally(key, label, hit.length, count);
    if (share && n) tally(`${key}:share`, `${label} share`, Math.round((hit.length / n) * 100), share, pct);
  };
  for (const [id, s] of Object.entries(club.style)) counted([id], s.name, `style:${id}`, s.count, s.share);
  for (const [id, g] of Object.entries(club.group)) counted(g.styles, g.name, `group:${id}`, g.count, g.share);

  // Where in the set.
  const positions = (ids: string[], when: Range[], label: string) => {
    if (!when.length) return;
    items.forEach((it, i) => {
      if (!shows(it, ids)) return;
      const p = where(i);
      if (!when.some((r) => inRange(p, r, 0.5))) {
        issues.push({ level: "warn", text: `${it.title}: ${label} at song ${i + 1} (${Math.round(p)}% through); ${club.name}: ${when.map((r) => rangeText(r, pct)).join(" or ")}`, entries: [it.entryId] });
      }
    });
  };
  for (const [id, s] of Object.entries(club.style)) positions([id], s.when, s.name);
  for (const g of Object.values(club.group)) positions(g.styles, g.when, g.name);

  // First and last.
  for (const [id, s] of Object.entries(club.style)) {
    if (s.first && n && !items[0].ids.includes(id)) issues.push({ level: "warn", text: `The set should start with a ${s.name}`, entries: [items[0].entryId] });
    if (s.last && n && !items[n - 1].ids.includes(id)) {
      issues.push({ level: opts.complete ? "warn" : "note", text: `The set should end with a ${s.name}`, entries: [items[n - 1].entryId] });
    }
  }

  // In a row.
  const runs = (ids: string[], max: number | null | undefined, label: string) => {
    if (!max) return;
    let run: SetItem[] = [];
    const flush = () => {
      if (run.length > max) issues.push({ level: "warn", text: `${run.length} ${label} in a row (at most ${max})`, entries: run.map((r) => r.entryId) });
      run = [];
    };
    for (const it of items) {
      if (shows(it, ids)) run.push(it);
      else flush();
    }
    flush();
  };
  for (const [id, s] of Object.entries(club.style)) runs([id], s.max_in_a_row ?? club.set.max_in_a_row, s.name);
  for (const g of Object.values(club.group)) runs(g.styles, g.max_in_a_row, g.name);

  // Rules for part of the set.
  for (const r of Object.values(club.rule)) {
    const part = items.filter((_, i) => (r.songs ? inRange(i + 1, r.songs) : true) && (r.when.length ? r.when.some((w) => inRange(where(i), w, 0.5)) : true));
    if (r.length) {
      for (const it of part) {
        if (it.lengthSecs != null && !inRange(it.lengthSecs, r.length, 1)) {
          issues.push({ level: "warn", text: `${it.title}: ${fmtSecs(it.lengthSecs)} long; ${r.name}: ${rangeText(r.length, fmtSecs)}`, entries: [it.entryId] });
        }
      }
    }
    if (r.count) {
      const have = r.styles.length ? part.filter((it) => shows(it, r.styles)).length : part.length;
      const label = r.styles.length ? `${r.styles.map(name).join(" / ")} in ${r.name.toLowerCase()}` : r.name;
      tally(`rule:${r.name}`, label, have, r.count);
    }
  }
  return { issues, tallies, songs: n, minutes };
}

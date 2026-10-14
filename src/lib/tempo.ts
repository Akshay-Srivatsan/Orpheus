// Tempo and duration helpers shared by the UI and the setlist rules.

import type { Overrides, Profile, SetlistEntry, StyleDef, Track, Version } from "./types";

/** Folds `bpm` by octaves so it is as close as possible to `target`. */
export function foldBpm(bpm: number, target: number): number {
  if (!(bpm > 0) || !(target > 0)) return bpm;
  let b = bpm;
  while (b < target / Math.SQRT2) b *= 2;
  while (b > target * Math.SQRT2) b /= 2;
  return b;
}

/** Centre of a style's preferred range. */
export function styleCenter(s: StyleDef | undefined): number | null {
  const r = s?.sweet ?? s?.extended;
  return r ? (r[0] + r[1]) / 2 : null;
}

/** A track's BPM expressed in the counting convention of a style. */
export function bpmForStyle(trackBpm: number | null, style: StyleDef | undefined): number | null {
  if (trackBpm == null) return null;
  const c = styleCenter(style);
  return c ? foldBpm(trackBpm, c) : trackBpm;
}

/** Effective settings: entry overrides → version (track defaults are applied by callers). */
export function resolveSettings(version: Version | undefined, entry: Partial<SetlistEntry> | null | undefined): Overrides {
  const e = entry?.overrides ?? {};
  const v = version ?? ({} as Partial<Version>);
  return {
    targetBpm: e.targetBpm ?? v.targetBpm ?? null,
    cueInMs: e.cueInMs ?? v.cueInMs ?? null,
    cueOutMs: e.cueOutMs ?? v.cueOutMs ?? null,
    fadeOutMs: e.fadeOutMs ?? v.fadeOutMs ?? null,
    gainDb: e.gainDb ?? v.gainDb ?? null,
    stems: e.stems ?? v.stems ?? null,
  };
}

/** Style of an entry (slot style or override) or version. */
export function styleOf(version: Version | undefined, entry?: Partial<SetlistEntry> | null): string | null {
  return entry?.style ?? version?.style ?? null;
}

export interface Playback {
  /** Original tempo in the style's counting. */
  base: number | null;
  /** Tempo it will be played at. */
  target: number | null;
  /** Playback rate (target / base). */
  rate: number;
}

/** How a version will be played (optionally within a setlist entry). Mirrors the backend's load_request. */
export function playback(track: Track | undefined, version: Version | undefined, entry: Partial<SetlistEntry> | null, profile: Profile | null): Playback {
  const style = profile?.styles.find((s) => s.key === styleOf(version, entry));
  const target = resolveSettings(version, entry).targetBpm ?? null;
  if (!track?.bpm) return { base: null, target, rate: 1 };
  if (target) {
    const base = foldBpm(track.bpm, target);
    return { base, target, rate: Math.min(2, Math.max(0.5, target / base)) };
  }
  const base = bpmForStyle(track.bpm, style);
  return { base, target: base, rate: 1 };
}

/** Audible duration in ms (cue points, fade-out, silence trimming and tempo applied). */
export function playedDurationMs(
  track: Track | undefined,
  version: Version | undefined,
  entry: Partial<SetlistEntry> | null,
  rate: number,
  opts: { fadeSeconds?: number; trimSilence?: boolean } = {},
): number {
  if (!track) return 0;
  const r = resolveSettings(version, entry);
  const trim = opts.trimSilence ?? true;
  const start = r.cueInMs ?? (trim ? track.audioStartMs : null) ?? 0;
  let end = r.cueOutMs ?? (trim ? track.audioEndMs : null) ?? track.durationMs;
  if (r.fadeOutMs != null) end = Math.min(end, r.fadeOutMs + (opts.fadeSeconds ?? 5) * 1000);
  return Math.max(0, end - start) / (rate || 1);
}

export type TempoFit = "sweet" | "extended" | "outside" | "unknown";

export function tempoFit(bpm: number | null, style: StyleDef | undefined): TempoFit {
  if (bpm == null || !style || (!style.sweet && !style.extended)) return "unknown";
  if (style.sweet && bpm >= style.sweet[0] - 0.5 && bpm <= style.sweet[1] + 0.5) return "sweet";
  if (style.extended && bpm >= style.extended[0] - 0.5 && bpm <= style.extended[1] + 0.5) return "extended";
  if (!style.extended && style.sweet && bpm >= style.sweet[0] * 0.9 && bpm <= style.sweet[1] * 1.1) return "extended";
  return "outside";
}

/** Rate needed to bring `bpm` into the style's sweet range (1 if already inside). */
export function rateToFit(bpm: number, style: StyleDef | undefined): number {
  const r = style?.sweet ?? style?.extended;
  if (!r) return 1;
  if (bpm < r[0]) return r[0] / bpm;
  if (bpm > r[1]) return r[1] / bpm;
  return 1;
}

export function fmtTime(ms: number | null | undefined, opts: { tenths?: boolean } = {}): string {
  if (ms == null || !isFinite(ms)) return "–:––";
  const neg = ms < 0;
  const total = Math.abs(ms) / 1000;
  const m = Math.floor(total / 60);
  const s = Math.floor(total % 60);
  let out = `${m}:${s.toString().padStart(2, "0")}`;
  if (opts.tenths) out += `.${Math.floor((total * 10) % 10)}`;
  return (neg ? "−" : "") + out;
}

export function fmtBpm(b: number | null | undefined): string {
  if (b == null) return "–";
  return Math.abs(b - Math.round(b)) < 0.05 ? Math.round(b).toString() : b.toFixed(1);
}

export function fmtPct(rate: number): string {
  const p = (rate - 1) * 100;
  if (Math.abs(p) < 0.05) return "0.0%";
  return `${p > 0 ? "+" : "−"}${Math.abs(p).toFixed(1)}%`;
}

/** Modifier-key label for shortcut hints (⌘ on macOS, Ctrl elsewhere). */
export const MOD = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform) ? "⌘" : "Ctrl+";

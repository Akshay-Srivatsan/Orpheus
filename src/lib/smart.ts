// Smart crates: rule-based collections evaluated against the library.
// Pure functions (no Tauri imports) so they're unit-testable.

import { clubStyle, clubStyleOfEntry, myStyle, prettify } from "./styles";
import { playback, playedDurationMs, tempoFit, bpmForStyle } from "./tempo";
import type { Crate, Id, Profile, Row, StyleLibrary } from "./types";

export type SmartField =
  | "style" | "bpm" | "tempoFit" | "duration" | "energy" | "rhythm" | "rating" | "text" | "source"
  | "familiar" | "plays" | "lastPlayed" | "added" | "crate" | "tag";

export interface SmartCondition {
  field: SmartField;
  op: string;
  /** Style key, number, [min, max], text, source kind, crate id or tag id, depending on the op. */
  value?: string | number | [number, number] | null;
}

export interface SmartRules {
  match: "all" | "any";
  conditions: SmartCondition[];
}

/** How an op's value is edited in the UI. */
export type SmartInput = "none" | "style" | "number" | "range" | "text" | "source" | "crate" | "tag" | "days";

export interface SmartOpDef {
  op: string;
  label: string;
  input: SmartInput;
}

export interface SmartFieldDef {
  field: SmartField;
  label: string;
  ops: SmartOpDef[];
}

const num = (op: string, label: string): SmartOpDef => ({ op, label, input: "number" });

export const SMART_FIELDS: SmartFieldDef[] = [
  {
    field: "style",
    label: "Dance style",
    ops: [
      { op: "is", label: "is", input: "style" },
      { op: "isNot", label: "is not", input: "style" },
      { op: "isSet", label: "is set", input: "none" },
      { op: "isEmpty", label: "is not set", input: "none" },
    ],
  },
  {
    field: "bpm",
    label: "Tempo",
    ops: [
      { op: "fitsStyle", label: "fits the range of", input: "style" },
      { op: "between", label: "is between (BPM)", input: "range" },
      num("atLeast", "is at least (BPM)"),
      num("atMost", "is at most (BPM)"),
      { op: "isEmpty", label: "is unknown", input: "none" },
    ],
  },
  {
    field: "tempoFit",
    label: "Tempo for its own style",
    ops: [
      { op: "sweet", label: "is in the sweet spot", input: "none" },
      { op: "ok", label: "is acceptable", input: "none" },
      { op: "outside", label: "is outside the range", input: "none" },
    ],
  },
  {
    field: "duration",
    label: "Length",
    ops: [
      { op: "atMost", label: "is at most (seconds)", input: "number" },
      { op: "atLeast", label: "is at least (seconds)", input: "number" },
    ],
  },
  { field: "energy", label: "Energy", ops: [num("atLeast", "is at least"), num("atMost", "is at most"), { op: "isEmpty", label: "is not rated", input: "none" }] },
  { field: "rhythm", label: "Beat clarity", ops: [num("atLeast", "is at least"), num("atMost", "is at most"), { op: "isEmpty", label: "is not rated", input: "none" }] },
  { field: "rating", label: "Rating", ops: [num("atLeast", "is at least"), num("atMost", "is at most"), { op: "isEmpty", label: "is not rated", input: "none" }] },
  {
    field: "text",
    label: "Title, artist, album or notes",
    ops: [
      { op: "contains", label: "contains", input: "text" },
      { op: "notContains", label: "does not contain", input: "text" },
    ],
  },
  { field: "source", label: "Source", ops: [{ op: "is", label: "is", input: "source" }, { op: "isNot", label: "is not", input: "source" }] },
  { field: "familiar", label: "Familiar / classic", ops: [{ op: "isTrue", label: "yes", input: "none" }, { op: "isFalse", label: "no", input: "none" }] },
  { field: "plays", label: "Times played", ops: [num("atLeast", "is at least"), num("atMost", "is at most")] },
  {
    field: "lastPlayed",
    label: "Last played",
    ops: [
      { op: "withinDays", label: "in the last (days)", input: "days" },
      { op: "notWithinDays", label: "not in the last (days)", input: "days" },
      { op: "never", label: "never", input: "none" },
    ],
  },
  { field: "added", label: "Added", ops: [{ op: "withinDays", label: "in the last (days)", input: "days" }] },
  { field: "crate", label: "Crate", ops: [{ op: "in", label: "contains it", input: "crate" }, { op: "notIn", label: "doesn't contain it", input: "crate" }] },
  { field: "tag", label: "Version tag", ops: [{ op: "has", label: "includes", input: "tag" }, { op: "lacks", label: "doesn't include", input: "tag" }] },
];

export function fieldDef(f: SmartField): SmartFieldDef {
  return SMART_FIELDS.find((d) => d.field === f) ?? SMART_FIELDS[0];
}

export function defaultCondition(field: SmartField = "style"): SmartCondition {
  const op = fieldDef(field).ops[0];
  const value = op.input === "range" ? ([100, 120] as [number, number]) : op.input === "number" || op.input === "days" ? (field === "duration" ? 210 : 30) : op.input === "source" ? "file" : null;
  return { field, op: op.op, value };
}

export interface SmartContext {
  /** The club whose tempo ranges "fits the range of" uses. */
  profile: Profile | null;
  /** Your styles and their club mappings. */
  styles: StyleLibrary | null;
  crates: Crate[];
  /** Rows of the whole library (needed when a rule refers to another smart crate). */
  library?: Row[];
  now?: number;
  fadeSeconds?: number;
  trimSilence?: boolean;
}

const DAY = 86_400_000;

function cmp(v: number | null | undefined, op: string, x: unknown): boolean {
  if (op === "isEmpty") return v == null;
  if (v == null || typeof x !== "number") return false;
  if (op === "atLeast") return v >= x;
  if (op === "atMost") return v <= x;
  return false;
}

/** Members of a crate (hand-picked or smart) as a set of version ids. */
export function crateMembers(c: Crate, ctx: SmartContext, depth = 0): Set<Id> {
  if (c.smart && depth < 4) return new Set(evaluate(c.smart, ctx.library ?? [], ctx, depth + 1).map((r) => r.version.id));
  return new Set(c.versionIds);
}

function test(cond: SmartCondition, row: Row, ctx: SmartContext, depth: number): boolean {
  const { track: t, version: v } = row;
  const own = v.styles ?? [];
  const clubOf = (k: string, bpm: number | null) => clubStyle(ctx.styles, ctx.profile, k, bpm) ?? undefined;
  const pb = playback(t, v, null, (ver, e, bpm) => clubStyleOfEntry(ctx.styles, ctx.profile, ver, e, bpm));
  switch (cond.field) {
    case "style":
      switch (cond.op) {
        case "is": return own.includes(String(cond.value));
        case "isNot": return !own.includes(String(cond.value));
        case "isSet": return own.length > 0;
        case "isEmpty": return own.length === 0;
      }
      return false;
    case "bpm": {
      const bpm = pb.target ?? t.bpm;
      if (cond.op === "isEmpty") return bpm == null;
      if (bpm == null) return false;
      if (cond.op === "fitsStyle") {
        const st = clubOf(String(cond.value ?? ""), bpm);
        return !!st && ["sweet", "extended"].includes(tempoFit(bpmForStyle(bpm, st), st));
      }
      if (cond.op === "between" && Array.isArray(cond.value)) return bpm >= cond.value[0] && bpm <= cond.value[1];
      return cmp(bpm, cond.op, cond.value);
    }
    case "tempoFit": {
      const st = clubStyleOfEntry(ctx.styles, ctx.profile, v, null, pb.target ?? t.bpm) ?? undefined;
      const fit = tempoFit(pb.target ?? (t.bpm ? bpmForStyle(t.bpm, st) : null), st);
      if (cond.op === "sweet") return fit === "sweet";
      if (cond.op === "ok") return fit === "sweet" || fit === "extended";
      if (cond.op === "outside") return fit === "outside";
      return false;
    }
    case "duration": {
      const secs = playedDurationMs(t, v, null, pb.rate, { fadeSeconds: ctx.fadeSeconds, trimSilence: ctx.trimSilence }) / 1000;
      return cmp(secs, cond.op, cond.value);
    }
    case "energy": return cmp(t.energy, cond.op, cond.value);
    case "rhythm": return cmp(t.rhythm, cond.op, cond.value);
    case "rating": return cmp(t.rating, cond.op, cond.value);
    case "plays": return cmp(t.playCount, cond.op, cond.value);
    case "text": {
      const q = String(cond.value ?? "").toLowerCase().trim();
      if (!q) return true;
      const hay = `${t.title} ${t.artist} ${t.album} ${t.genre ?? ""} ${t.notes ?? ""} ${v.name} ${v.comment ?? ""}`.toLowerCase();
      return cond.op === "notContains" ? !hay.includes(q) : hay.includes(q);
    }
    case "source": {
      const has = t.sources.some((s) => s.kind === cond.value);
      return cond.op === "isNot" ? !has : has;
    }
    case "familiar": return cond.op === "isFalse" ? !t.familiar : !!t.familiar;
    case "lastPlayed": {
      const now = ctx.now ?? Date.now();
      const last = t.lastPlayedAt;
      if (cond.op === "never") return !last;
      const days = typeof cond.value === "number" ? cond.value : 0;
      const recent = !!last && now - last <= days * DAY;
      return cond.op === "notWithinDays" ? !recent : recent;
    }
    case "added": {
      const days = typeof cond.value === "number" ? cond.value : 0;
      return (ctx.now ?? Date.now()) - t.addedAt <= days * DAY;
    }
    case "crate": {
      const c = ctx.crates.find((x) => x.id === cond.value);
      const inIt = !!c && crateMembers(c, ctx, depth).has(v.id);
      return cond.op === "notIn" ? !inIt : inIt;
    }
    case "tag": {
      const has = (v.tags ?? []).includes(String(cond.value ?? ""));
      return cond.op === "lacks" ? !has : has;
    }
  }
  return false;
}

/** Rows matching the rules (in input order). No conditions = no songs. */
export function evaluate(rules: SmartRules, rows: Row[], ctx: SmartContext, depth = 0): Row[] {
  const conds = rules.conditions ?? [];
  if (!conds.length) return [];
  const all = rules.match !== "any";
  return rows.filter((r) => (all ? conds.every((c) => test(c, r, ctx, depth)) : conds.some((c) => test(c, r, ctx, depth))));
}

/** Short human description of a condition, for tooltips. */
export function describe(c: SmartCondition, ctx: Pick<SmartContext, "styles" | "crates">): string {
  const f = fieldDef(c.field);
  const op = f.ops.find((o) => o.op === c.op);
  let val = "";
  switch (op?.input) {
    case "style": val = myStyle(ctx.styles, String(c.value ?? ""))?.name ?? prettify(String(c.value ?? "")); break;
    case "crate": val = ctx.crates.find((x) => x.id === c.value)?.name ?? "?"; break;
    case "range": val = Array.isArray(c.value) ? `${c.value[0]}–${c.value[1]}` : ""; break;
    case "none": break;
    default: val = String(c.value ?? "");
  }
  return `${f.label} ${op?.label ?? c.op}${val ? " " + val : ""}`;
}

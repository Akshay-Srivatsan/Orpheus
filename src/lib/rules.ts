// Setlist rule checking against a club profile. Pure functions (no Tauri), unit-tested in rules.test.ts.

import type { CountSpec, Id, Profile, Rule, Setlist, SetlistEntry, Severity, StyleDef, Track, Version } from "./types";
import { fmtTime, playback, playedDurationMs, tempoFit } from "./tempo";

export interface CheckInput {
  profile: Profile;
  setlist: Setlist;
  tracks: Map<Id, Track>;
  versions: Map<Id, { version: Version; track: Track }>;
  fadeSeconds?: number;
  trimSilence?: boolean;
}

export interface RuleResult {
  /** Index of the rule in profile.rules (-1 for synthetic results). */
  index: number;
  rule: Rule;
  label: string;
  severity: Severity;
  ok: boolean;
  detail: string;
  entryIds: Id[];
}

export interface StyleCount {
  key: string;
  name: string;
  color: string;
  count: number;
  min?: number;
  max?: number;
}

export interface CheckResult {
  results: RuleResult[];
  counts: StyleCount[];
  totalMs: number;
  unknownDurations: number;
  issuesByEntry: Map<Id, RuleResult[]>;
}

/** A non-skipped entry with everything the rules need resolved. */
interface Item {
  entry: SetlistEntry;
  /** 1-based position among non-skipped entries. */
  pos: number;
  style: string | null;
  styleDef: StyleDef | undefined;
  track: Track | undefined;
  version: Version | undefined;
  /** Tempo it will be played at, or null if unknown. */
  bpm: number | null;
  /** Audible duration in ms, or null if unknown (slots, missing tracks). */
  durationMs: number | null;
  tags: string[];
}

interface Ctx {
  profile: Profile;
  items: Item[];
  lessonStyle: string | null;
  lessonCategory: string;
}

const songs = (n: number) => `${n} song${n === 1 ? "" : "s"}`;

function listPositions(items: Item[], max = 6): string {
  const p = items.map((i) => i.pos);
  const shown = p.slice(0, max).join(", ");
  return p.length > max ? `${shown}, …` : shown;
}

/** Does a style key match a rule target (`key`, `group:<id>`, `@lesson`)? */
function matches(ctx: Ctx, target: string, style: string | null): boolean {
  if (!style) return false;
  if (target === "@lesson") return ctx.lessonStyle != null && style === ctx.lessonStyle;
  if (target.startsWith("group:")) {
    const g = target.slice(6);
    return ctx.profile.styles.find((s) => s.key === style)?.group === g;
  }
  return style === target;
}

function targetName(ctx: Ctx, target: string): string {
  if (target === "@lesson") {
    const s = ctx.profile.styles.find((x) => x.key === ctx.lessonStyle);
    return s ? `${s.name} (lesson)` : "Lesson style";
  }
  if (target.startsWith("group:")) {
    const g = target.slice(6);
    return ctx.profile.groups?.find((x) => x.id === g)?.name ?? g;
  }
  return ctx.profile.styles.find((s) => s.key === target)?.name ?? target;
}

function targetColor(ctx: Ctx, target: string): string {
  if (target === "@lesson") return ctx.profile.styles.find((x) => x.key === ctx.lessonStyle)?.color ?? "#9ca3af";
  if (target.startsWith("group:")) {
    const g = target.slice(6);
    return ctx.profile.styles.find((s) => s.group === g)?.color ?? "#9ca3af";
  }
  return ctx.profile.styles.find((s) => s.key === target)?.color ?? "#9ca3af";
}

function countFor(ctx: Ctx, spec: CountSpec | undefined): number | undefined {
  if (spec == null) return undefined;
  if (typeof spec === "number") return spec;
  return spec[ctx.lessonCategory] ?? spec["other"];
}

/** Effective [min, max] tempo range for a rule ("extended" falls back to sweet ±10%). */
function rangeFor(style: StyleDef, which: "sweet" | "extended"): [number, number] | null {
  if (which === "sweet") return style.sweet ?? style.extended ?? null;
  if (style.extended) return style.extended;
  if (style.sweet) return [style.sweet[0] * 0.9, style.sweet[1] * 1.1];
  return null;
}

function outsideRange(item: Item, which: "sweet" | "extended"): boolean {
  if (item.bpm == null || !item.styleDef) return false;
  if (which === "extended" && !item.styleDef.extended) {
    const r = rangeFor(item.styleDef, which);
    return r != null && (item.bpm < r[0] - 0.5 || item.bpm > r[1] + 0.5);
  }
  if (which === "sweet") {
    const fit = tempoFit(item.bpm, { ...item.styleDef, extended: null });
    return fit === "outside";
  }
  return tempoFit(item.bpm, item.styleDef) === "outside";
}

function defaultLabel(ctx: Ctx, rule: Rule): string {
  switch (rule.type) {
    case "songCount":
      return rule.min != null && rule.max != null
        ? `${rule.min}–${rule.max} songs`
        : rule.min != null
          ? `At least ${rule.min} songs`
          : `At most ${rule.max} songs`;
    case "maxDuration": {
      const range = rule.to ? `Songs ${rule.from ?? 1}–${rule.to}` : rule.from && rule.from > 1 ? `Songs from #${rule.from}` : "All songs";
      return `${range} under ${fmtTime(rule.seconds * 1000)}`;
    }
    case "styleCount":
      return targetName(ctx, rule.target);
    case "styleInFirst":
      return `At least ${rule.min} of the first ${rule.n} songs: ${targetName(ctx, rule.target)}`;
    case "tagCount": {
      const tag = ctx.profile.tags.find((t) => t.id === rule.tag)?.name ?? rule.tag;
      const half = rule.half ? ` in the ${rule.half} half` : "";
      return `${tag}${rule.target ? ` (${targetName(ctx, rule.target)})` : ""}${half}`;
    }
    case "tempoRange":
      return `Tempo within ${rule.range} range`;
    case "endingSequence":
      return `Ends with ${rule.sequence.map((t) => targetName(ctx, t)).join(", ")}`;
    case "noRepeat":
      return "No song played twice";
    case "noConsecutiveSame":
      return "No two consecutive songs of the same style and tempo";
    case "maxConsecutive":
      return `No more than ${rule.count} ${rule.energy} songs in a row`;
  }
}

type Outcome = { ok: boolean; detail: string; entryIds?: Id[]; severity?: Severity };

function evaluate(ctx: Ctx, rule: Rule): Outcome {
  const items = ctx.items;
  const needsLesson =
    ("target" in rule && rule.target === "@lesson") ||
    (rule.type === "endingSequence" && [...rule.sequence, ...(rule.allowAfter ?? [])].includes("@lesson"));
  if (needsLesson && !ctx.lessonStyle) {
    return { ok: false, detail: "Set a lesson style for this setlist", severity: "warning" };
  }

  switch (rule.type) {
    case "songCount": {
      const n = items.length;
      if (rule.min != null && n < rule.min) return { ok: false, detail: `${songs(n)} — need ${rule.min - n} more` };
      if (rule.max != null && n > rule.max) return { ok: false, detail: `${songs(n)} — ${n - rule.max} too many` };
      return { ok: true, detail: songs(n) };
    }

    case "maxDuration": {
      const from = rule.from ?? 1;
      const to = rule.to ?? Infinity;
      const scope = items.filter((i) => i.pos >= from && i.pos <= to);
      const over = scope.filter((i) => i.durationMs != null && i.durationMs > rule.seconds * 1000 + 500);
      if (over.length === 0) {
        const unknown = scope.filter((i) => i.durationMs == null).length;
        return { ok: true, detail: unknown ? `OK (${unknown} unknown)` : "OK" };
      }
      return {
        ok: false,
        detail: `Song${over.length > 1 ? "s" : ""} ${listPositions(over)} over ${fmtTime(rule.seconds * 1000)}`,
        entryIds: over.map((i) => i.entry.id),
      };
    }

    case "styleCount": {
      const hits = items.filter((i) => matches(ctx, rule.target, i.style));
      const n = hits.length;
      const min = countFor(ctx, rule.min);
      const max = countFor(ctx, rule.max);
      if (min != null && n < min) return { ok: false, detail: `${n} of ${min} — need ${min - n} more` };
      if (max != null && n > max)
        return { ok: false, detail: `${n} — max ${max} (${n - max} too many)`, entryIds: hits.map((i) => i.entry.id) };
      const parts = [min != null ? `${n} of ${min}` : `${n}`];
      if (max != null) parts.push(`max ${max}`);
      return { ok: true, detail: parts.join(", ") };
    }

    case "styleInFirst": {
      const first = items.filter((i) => i.pos <= rule.n);
      const n = first.filter((i) => matches(ctx, rule.target, i.style)).length;
      return n >= rule.min
        ? { ok: true, detail: `${n} in the first ${rule.n}` }
        : { ok: false, detail: `${n} of ${rule.min} in the first ${rule.n}`, entryIds: first.map((i) => i.entry.id) };
    }

    case "tagCount": {
      const half = Math.ceil(items.length / 2);
      const hits = items.filter((i) => {
        if (!i.tags.includes(rule.tag)) return false;
        if (rule.target && !matches(ctx, rule.target, i.style)) return false;
        if (rule.half === "first" && i.pos > half) return false;
        if (rule.half === "second" && i.pos <= half) return false;
        return true;
      });
      const n = hits.length;
      // Tagged entries that miss the required style/half, to point the DJ at them.
      const misplaced = items.filter((i) => i.tags.includes(rule.tag) && !hits.includes(i));
      if (rule.min != null && n < rule.min) {
        const extra = misplaced.length ? ` (${misplaced.length} tagged elsewhere)` : "";
        return { ok: false, detail: `${n} of ${rule.min}${extra}`, entryIds: misplaced.map((i) => i.entry.id) };
      }
      if (rule.max != null && n > rule.max)
        return { ok: false, detail: `${n} — max ${rule.max}`, entryIds: hits.map((i) => i.entry.id) };
      return { ok: true, detail: `${n}` };
    }

    case "tempoRange": {
      const exceptions = items.filter((i) => outsideRange(i, rule.range));
      const problems: string[] = [];
      const bad = new Set<Item>();
      if (rule.notInFirst) {
        const early = exceptions.filter((i) => i.pos <= rule.notInFirst!);
        if (early.length) {
          problems.push(`songs ${listPositions(early)} out of range in the first ${rule.notInFirst}`);
          early.forEach((i) => bad.add(i));
        }
      }
      if (rule.noConsecutive) {
        const consec: Item[] = [];
        for (let k = 1; k < exceptions.length; k++) {
          if (exceptions[k].pos === exceptions[k - 1].pos + 1) {
            consec.push(exceptions[k - 1], exceptions[k]);
          }
        }
        const uniq = [...new Set(consec)];
        if (uniq.length) {
          problems.push(`consecutive exceptions at ${listPositions(uniq)}`);
          uniq.forEach((i) => bad.add(i));
        }
      }
      const max = rule.maxExceptions ?? 0;
      if (exceptions.length > max) {
        problems.push(`${exceptions.length} out of range (max ${max})`);
        exceptions.forEach((i) => bad.add(i));
      }
      if (problems.length) {
        const text = problems.join("; ");
        return {
          ok: false,
          detail: text.charAt(0).toUpperCase() + text.slice(1),
          entryIds: [...bad].sort((a, b) => a.pos - b.pos).map((i) => i.entry.id),
        };
      }
      return {
        ok: true,
        detail: exceptions.length ? `${exceptions.length} exception${exceptions.length > 1 ? "s" : ""} (max ${max})` : "All in range",
      };
    }

    case "endingSequence": {
      let end = items.length;
      const allow = rule.allowAfter ?? [];
      while (end > 0 && allow.some((t) => matches(ctx, t, items[end - 1].style))) end--;
      const need = rule.sequence.length;
      if (end < need) return { ok: false, detail: `Need the last ${need} songs to be ${rule.sequence.map((t) => targetName(ctx, t)).join(", ")}` };
      const tail = items.slice(end - need, end);
      const flex = Math.min(rule.flexibleCount ?? 0, need);
      const wrong: Item[] = [];
      // Flexible part: multiset match (greedy bipartite is enough for distinct targets).
      const remaining = rule.sequence.slice(0, flex);
      for (const it of tail.slice(0, flex)) {
        const k = remaining.findIndex((t) => matches(ctx, t, it.style));
        if (k >= 0) remaining.splice(k, 1);
        else wrong.push(it);
      }
      for (let k = flex; k < need; k++) {
        if (!matches(ctx, rule.sequence[k], tail[k].style)) wrong.push(tail[k]);
      }
      if (wrong.length) {
        const missing = remaining.map((t) => targetName(ctx, t));
        const fixed = rule.sequence.slice(flex).map((t) => targetName(ctx, t));
        const parts = [`Song${wrong.length > 1 ? "s" : ""} ${listPositions(wrong)} out of place`];
        if (missing.length) parts.push(`missing ${missing.join(", ")}`);
        if (fixed.length) parts.push(`last must be ${fixed.join(", ")}`);
        return { ok: false, detail: parts.join("; "), entryIds: wrong.map((i) => i.entry.id) };
      }
      return { ok: true, detail: "OK" };
    }

    case "noRepeat": {
      const seen = new Map<Id, Item>();
      const dupes: Item[] = [];
      for (const i of items) {
        if (!i.track) continue;
        const prev = seen.get(i.track.id);
        if (prev) {
          if (!dupes.includes(prev)) dupes.push(prev);
          dupes.push(i);
        } else seen.set(i.track.id, i);
      }
      return dupes.length
        ? { ok: false, detail: `Repeated: songs ${listPositions(dupes)}`, entryIds: dupes.map((i) => i.entry.id) }
        : { ok: true, detail: "OK" };
    }

    case "noConsecutiveSame": {
      const pct = rule.tempoPct ?? 5;
      const bad: Item[] = [];
      for (let k = 1; k < items.length; k++) {
        const a = items[k - 1];
        const b = items[k];
        if (!a.style || a.style !== b.style) continue;
        if (a.bpm == null || b.bpm == null) continue;
        if (Math.abs(a.bpm - b.bpm) <= (Math.min(a.bpm, b.bpm) * pct) / 100) {
          if (!bad.includes(a)) bad.push(a);
          bad.push(b);
        }
      }
      return bad.length
        ? { ok: false, detail: `Songs ${listPositions(bad)}`, entryIds: bad.map((i) => i.entry.id) }
        : { ok: true, detail: "OK" };
    }

    case "maxConsecutive": {
      const bad: Item[] = [];
      let run: Item[] = [];
      const flush = () => {
        if (run.length > rule.count) bad.push(...run);
        run = [];
      };
      for (const i of items) {
        if (i.styleDef?.energy === rule.energy) run.push(i);
        else flush();
      }
      flush();
      return bad.length
        ? { ok: false, detail: `Run at songs ${listPositions(bad)}`, entryIds: bad.map((i) => i.entry.id) }
        : { ok: true, detail: "OK" };
    }
  }
}

export function checkSetlist(input: CheckInput): CheckResult {
  const { profile, setlist } = input;
  const styleMap = new Map(profile.styles.map((s) => [s.key, s]));
  const items: Item[] = [];
  let totalMs = 0;
  let unknownDurations = 0;
  for (const entry of setlist.entries) {
    if (entry.status === "skipped") continue;
    const vt = entry.versionId ? input.versions.get(entry.versionId) : undefined;
    const version = vt?.version;
    const track = vt?.track ?? undefined;
    const style = entry.style ?? version?.style ?? null;
    let bpm: number | null = null;
    let durationMs: number | null = null;
    if (track && version) {
      const pb = playback(track, version, entry, profile);
      bpm = pb.target;
      durationMs = playedDurationMs(track, version, entry, pb.rate, {
        fadeSeconds: input.fadeSeconds ?? profile.fadeOutSeconds ?? 5,
        trimSilence: input.trimSilence,
      });
      if (!(durationMs > 0)) durationMs = null;
    }
    if (durationMs == null) unknownDurations++;
    else totalMs += durationMs;
    items.push({
      entry,
      pos: items.length + 1,
      style,
      styleDef: style ? styleMap.get(style) : undefined,
      track,
      version,
      bpm,
      durationMs,
      tags: [...new Set([...(entry.tags ?? []), ...(version?.tags ?? [])])],
    });
  }

  const lessonStyle = setlist.lessonStyle ?? null;
  const lessonCategory = (lessonStyle && styleMap.get(lessonStyle)?.lessonCategory) || "other";
  const ctx: Ctx = { profile, items, lessonStyle, lessonCategory };

  const results: RuleResult[] = profile.rules.map((rule, index) => {
    const o = evaluate(ctx, rule);
    return {
      index,
      rule,
      label: rule.label ?? defaultLabel(ctx, rule),
      severity: o.severity ?? rule.severity ?? "error",
      ok: o.ok,
      detail: o.detail,
      entryIds: o.entryIds ?? [],
    };
  });

  const slots = items.filter((i) => !i.entry.versionId);
  if (slots.length) {
    results.push({
      index: -1,
      rule: { type: "songCount", label: "Unfilled slots" },
      label: "Unfilled style slots",
      severity: "info",
      ok: false,
      detail: `${slots.length} slot${slots.length > 1 ? "s" : ""} still need a song`,
      entryIds: slots.map((i) => i.entry.id),
    });
  }

  // Style counts: every styleCount target plus every style present.
  const counts: StyleCount[] = [];
  const seenKeys = new Set<string>();
  for (const rule of profile.rules) {
    if (rule.type !== "styleCount" || seenKeys.has(rule.target)) continue;
    if (rule.target === "@lesson" && !lessonStyle) continue;
    seenKeys.add(rule.target);
    counts.push({
      key: rule.target,
      name: targetName(ctx, rule.target),
      color: targetColor(ctx, rule.target),
      count: items.filter((i) => matches(ctx, rule.target, i.style)).length,
      min: countFor(ctx, rule.min),
      max: countFor(ctx, rule.max),
    });
  }
  const present = new Map<string, number>();
  for (const i of items) if (i.style) present.set(i.style, (present.get(i.style) ?? 0) + 1);
  for (const [key, count] of present) {
    if (seenKeys.has(key)) continue;
    seenKeys.add(key);
    const s = styleMap.get(key);
    counts.push({ key, name: s?.name ?? key, color: s?.color ?? "#9ca3af", count });
  }

  const issuesByEntry = new Map<Id, RuleResult[]>();
  for (const r of results) {
    if (r.ok) continue;
    for (const id of r.entryIds) {
      const list = issuesByEntry.get(id) ?? [];
      list.push(r);
      issuesByEntry.set(id, list);
    }
  }

  return { results, counts, totalMs, unknownDurations, issuesByEntry };
}

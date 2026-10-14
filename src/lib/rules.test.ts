import { describe, expect, it } from "vitest";
import { checkSetlist, type CheckResult, type RuleResult } from "./rules";
import type { Profile, Rule, Setlist, SetlistEntry, Track, Version } from "./types";

const profileModules = import.meta.glob("../../src-tauri/profiles/*.json", { eager: true, import: "default" });
const builtin = (id: string) =>
  structuredClone(Object.entries(profileModules).find(([p]) => p.endsWith(`/${id}.json`))![1]) as Profile;
const dancebreak = () => builtin("dancebreak");
const powers = () => builtin("powers");

let seq = 0;

/** A song: one track with one version of the given style. */
interface Song {
  track: Track;
  version: Version;
}

function song(style: string | null, bpm: number | null, durationSec = 180, extra: Partial<Version> = {}): Song {
  const id = `t${++seq}`;
  const track: Track = {
    id,
    title: id,
    artist: "",
    album: "",
    durationMs: durationSec * 1000,
    bpm,
    bpmSource: "user",
    energy: null,
    rhythm: null,
    rating: null,
    loudnessDb: null,
    audioStartMs: null,
    audioEndMs: null,
    analyzed: true,
    addedAt: 0,
    sources: [{ kind: "file", key: id }],
    versions: [],
    files: ["/x"],
    missing: false,
    playCount: 0,
    lastPlayedAt: null,
  };
  const version: Version = { id: `v${seq}`, name: "Original", style, ...extra };
  track.versions = [version];
  return { track, version };
}

interface Opts {
  lesson?: string | null;
  /** Per-entry tweaks by index. */
  entry?: Record<number, Partial<SetlistEntry>>;
  slots?: { at: number; style: string }[];
}

function run(profile: Profile, songs: Song[], opts: Opts = {}): CheckResult {
  const entries: SetlistEntry[] = songs.map((s, i) => ({
    id: `e${i + 1}`,
    versionId: s.version.id,
    status: "pending",
    ...(opts.entry?.[i] ?? {}),
  }));
  for (const sl of opts.slots ?? []) {
    entries.splice(sl.at, 0, { id: `slot${sl.at}`, style: sl.style, status: "pending" });
  }
  const setlist: Setlist = {
    id: "s",
    name: "Test",
    eventDate: null,
    profileId: profile.id,
    lessonStyle: opts.lesson === undefined ? null : opts.lesson,
    notes: "",
    mode: "preset",
    createdAt: 0,
    updatedAt: 0,
    entries,
  };
  const tracks = new Map(songs.map((s) => [s.track.id, s.track]));
  const versions = new Map(songs.map((s) => [s.version.id, { version: s.version, track: s.track }]));
  return checkSetlist({ profile, setlist, tracks, versions, trimSilence: false });
}

/** A profile with only the given rules. */
function only(base: Profile, ...rules: Rule[]): Profile {
  return { ...base, rules };
}

const result = (r: CheckResult, type: Rule["type"], n = 0): RuleResult => r.results.filter((x) => x.rule.type === type)[n];
const byLabel = (r: CheckResult, label: string): RuleResult => {
  const x = r.results.find((x) => x.label === label);
  if (!x) throw new Error(`no rule "${label}" in ${r.results.map((x) => x.label).join(" | ")}`);
  return x;
};

describe("songCount", () => {
  it("checks min/max", () => {
    const p = only(dancebreak(), { type: "songCount", min: 3, max: 4 });
    expect(result(run(p, [song("polka", 115), song("polka", 115)]), "songCount").ok).toBe(false);
    expect(result(run(p, [song("polka", 115), song("polka", 115), song("polka", 115)]), "songCount").ok).toBe(true);
    const five = Array.from({ length: 5 }, () => song("polka", 115));
    const r = result(run(p, five), "songCount");
    expect(r.ok).toBe(false);
    expect(r.detail).toContain("1 too many");
  });

  it("ignores skipped entries", () => {
    const p = only(dancebreak(), { type: "songCount", max: 2 });
    const songs = [song("polka", 115), song("polka", 115), song("polka", 115)];
    expect(result(run(p, songs), "songCount").ok).toBe(false);
    expect(result(run(p, songs, { entry: { 1: { status: "skipped" } } }), "songCount").ok).toBe(true);
  });
});

describe("maxDuration", () => {
  it("applies to a position range and accounts for tempo", () => {
    const p = only(dancebreak(), { type: "maxDuration", seconds: 210, from: 1, to: 2 });
    // 220 s at original tempo is too long...
    const long = song("cross-step-waltz", 115, 220);
    let r = result(run(p, [song("polka", 115), long]), "maxDuration");
    expect(r.ok).toBe(false);
    expect(r.entryIds).toEqual(["e2"]);
    expect(r.detail).toContain("Song 2");
    // ...but sped up 10% it fits (220 / 1.1 = 200 s).
    const fast = song("cross-step-waltz", 110, 220, { targetBpm: 121 });
    r = result(run(p, [song("polka", 115), fast]), "maxDuration");
    expect(r.ok).toBe(true);
    // Out of range positions are ignored.
    r = result(run(p, [song("polka", 115), song("polka", 115), long]), "maxDuration");
    expect(r.ok).toBe(true);
  });

  it("respects cue-out overrides", () => {
    const p = only(dancebreak(), { type: "maxDuration", seconds: 210 });
    const s = song("polka", 115, 300);
    expect(result(run(p, [s]), "maxDuration").ok).toBe(false);
    expect(result(run(p, [s], { entry: { 0: { overrides: { cueOutMs: 200_000 } } } }), "maxDuration").ok).toBe(true);
  });
});

describe("styleCount", () => {
  it("counts styles and groups", () => {
    const p = only(
      dancebreak(),
      { type: "styleCount", target: "group:rotary", min: 2 },
      { type: "styleCount", target: "fast-waltz", max: 1 },
    );
    const r = run(p, [song("rotary-waltz", 150), song("fast-waltz", 180), song("fast-waltz", 185)]);
    expect(r.results[0].ok).toBe(true);
    expect(r.results[1].ok).toBe(false);
    expect(r.results[1].entryIds).toEqual(["e2", "e3"]);
  });

  it("uses lesson categories for @lesson and per-style minimums", () => {
    const p = only(
      dancebreak(),
      { type: "styleCount", target: "@lesson", min: { waltz: 3, swing: 3, other: 2 } },
      { type: "styleCount", target: "lindy-hop", min: { waltz: 2, swing: 2, other: 1 } },
    );
    const songs = [song("polka", 115), song("polka", 115), song("lindy-hop", 140)];
    // Polka is an "other" lesson: 2 polkas and 1 lindy are enough.
    let r = run(p, songs, { lesson: "polka" });
    expect(r.results.map((x) => x.ok)).toEqual([true, true]);
    // Lindy is a "swing" lesson: need 3 lindys.
    r = run(p, songs, { lesson: "lindy-hop" });
    expect(r.results.map((x) => x.ok)).toEqual([false, false]);
    expect(r.results[0].detail).toBe("1 of 3 — need 2 more");
  });

  it("asks for a lesson style when @lesson is used without one", () => {
    const p = only(dancebreak(), { type: "styleCount", target: "@lesson", min: 5 });
    const r = run(p, [song("polka", 115)]);
    expect(r.results[0].ok).toBe(false);
    expect(r.results[0].severity).toBe("warning");
    expect(r.results[0].detail).toMatch(/lesson style/i);
  });

  it("counts unfilled slots by their style", () => {
    const p = only(dancebreak(), { type: "styleCount", target: "cross-step-waltz", min: 2 });
    const r = run(p, [song("cross-step-waltz", 115)], { slots: [{ at: 1, style: "cross-step-waltz" }] });
    expect(r.results[0].ok).toBe(true);
    const slots = r.results.find((x) => x.label === "Unfilled style slots")!;
    expect(slots.severity).toBe("info");
    expect(slots.entryIds).toEqual(["slot1"]);
    expect(r.unknownDurations).toBe(1);
  });
});

describe("styleInFirst", () => {
  it("looks only at the first n songs", () => {
    const p = only(dancebreak(), { type: "styleInFirst", target: "@lesson", n: 3, min: 2 });
    const a = [song("polka", 115), song("lindy-hop", 140), song("polka", 115)];
    expect(run(p, a, { lesson: "polka" }).results[0].ok).toBe(true);
    const b = [song("polka", 115), song("lindy-hop", 140), song("lindy-hop", 140), song("polka", 115)];
    expect(run(p, b, { lesson: "polka" }).results[0].ok).toBe(false);
  });
});

describe("tagCount", () => {
  it("checks tag, target style and half", () => {
    const p = only(dancebreak(), { type: "tagCount", tag: "jam", target: "cross-step-waltz", half: "second", min: 1 });
    const songs = [song("cross-step-waltz", 115), song("polka", 115), song("cross-step-waltz", 115), song("polka", 115)];
    // Jam in the first half: not enough, and the misplaced entry is pointed out.
    let r = run(p, songs, { entry: { 0: { tags: ["jam"] } } });
    expect(r.results[0].ok).toBe(false);
    expect(r.results[0].entryIds).toEqual(["e1"]);
    // Jam on a cross-step in the second half.
    r = run(p, songs, { entry: { 2: { tags: ["jam"] } } });
    expect(r.results[0].ok).toBe(true);
    // Tags on the version count too.
    const tagged = song("cross-step-waltz", 115, 180, { tags: ["jam"] });
    r = run(p, [songs[0], songs[1], tagged, songs[3]]);
    expect(r.results[0].ok).toBe(true);
  });
});

describe("tempoRange", () => {
  const rule: Rule = { type: "tempoRange", range: "extended", maxExceptions: 2, notInFirst: 2, noConsecutive: true };
  // Cross-step extended range is 105–125.
  const ok = () => song("cross-step-waltz", 115);
  const slow = () => song("cross-step-waltz", 95);

  it("allows a limited number of non-consecutive exceptions after the opening", () => {
    const p = only(dancebreak(), rule);
    expect(run(p, [ok(), ok(), slow(), ok(), slow()]).results[0].ok).toBe(true);
  });

  it("flags exceptions in the opening songs", () => {
    const p = only(dancebreak(), rule);
    const r = run(p, [ok(), slow(), ok()]);
    expect(r.results[0].ok).toBe(false);
    expect(r.results[0].entryIds).toEqual(["e2"]);
  });

  it("flags consecutive exceptions and too many exceptions", () => {
    const p = only(dancebreak(), rule);
    let r = run(p, [ok(), ok(), slow(), slow()]);
    expect(r.results[0].ok).toBe(false);
    expect(r.results[0].detail).toMatch(/consecutive/i);
    r = run(p, [ok(), ok(), slow(), ok(), slow(), ok(), slow()]);
    expect(r.results[0].ok).toBe(false);
    expect(r.results[0].detail).toMatch(/3 out of range \(max 2\)/);
  });

  it("uses the target tempo, not the original", () => {
    const p = only(dancebreak(), rule);
    const fixed = song("cross-step-waltz", 95, 180, { targetBpm: 110 });
    expect(run(p, [ok(), fixed]).results[0].ok).toBe(true);
  });

  it("falls back to sweet ±10% when a style has no extended range", () => {
    const p = only(dancebreak(), { type: "tempoRange", range: "extended" });
    // Fast waltz: sweet 170–200, no extended range.
    expect(run(p, [song("fast-waltz", 160)]).results[0].ok).toBe(true);
    expect(run(p, [song("fast-waltz", 150)]).results[0].ok).toBe(false);
  });

  it("ignores skipped entries and unknown tempos", () => {
    const p = only(dancebreak(), { type: "tempoRange", range: "extended" });
    expect(run(p, [ok(), slow()], { entry: { 1: { status: "skipped" } } }).results[0].ok).toBe(true);
    expect(run(p, [song("cross-step-waltz", null)]).results[0].ok).toBe(true);
  });
});

describe("endingSequence (Dancebreak)", () => {
  const ending = () => dancebreak().rules.find((r) => r.type === "endingSequence")!;
  const p = () => only(dancebreak(), ending());
  const csw = () => song("cross-step-waltz", 115);
  const wcs = () => song("west-coast-swing", 100);
  const lh = () => song("lindy-hop", 140);
  const rw = () => song("rotary-waltz", 150);

  it("accepts the first three in any order with the rotary last", () => {
    expect(run(p(), [csw(), wcs(), lh(), csw(), rw()]).results[0].ok).toBe(true);
    expect(run(p(), [csw(), csw(), lh(), wcs(), rw()]).results[0].ok).toBe(true);
  });

  it("allows an optional solo jazz routine after the last rotary", () => {
    expect(run(p(), [lh(), csw(), wcs(), rw(), song("solo-jazz", null)]).results[0].ok).toBe(true);
  });

  it("requires the rotary waltz to be last", () => {
    const r = run(p(), [wcs(), lh(), rw(), csw()]);
    expect(r.results[0].ok).toBe(false);
    expect(r.results[0].detail).toMatch(/Rotary Waltz/);
  });

  it("reports missing styles", () => {
    const r = run(p(), [wcs(), wcs(), csw(), rw()]);
    expect(r.results[0].ok).toBe(false);
    expect(r.results[0].detail).toMatch(/missing Lindy Hop/);
  });

  it("ignores a skipped final song", () => {
    const r = run(p(), [wcs(), lh(), csw(), rw(), wcs()], { entry: { 4: { status: "skipped" } } });
    expect(r.results[0].ok).toBe(true);
  });

  it("fails on a too-short set", () => {
    expect(run(p(), [rw()]).results[0].ok).toBe(false);
  });
});

describe("noRepeat / noConsecutiveSame / maxConsecutive", () => {
  it("finds repeated tracks across versions", () => {
    const p = only(powers(), { type: "noRepeat" });
    const a = song("waltz", 144);
    const other: Version = { id: "v-other", name: "Viennese", style: "viennese-waltz", targetBpm: 168 };
    a.track.versions.push(other);
    const b: Song = { track: a.track, version: other };
    const r = run(p, [a, song("polka", 114), b]);
    expect(r.results[0].ok).toBe(false);
    expect(r.results[0].entryIds).toEqual(["e1", "e3"]);
  });

  it("flags adjacent same style at similar tempo", () => {
    const p = only(powers(), { type: "noConsecutiveSame", tempoPct: 5 });
    expect(run(p, [song("waltz", 144), song("waltz", 146)]).results[0].ok).toBe(false);
    expect(run(p, [song("waltz", 140), song("waltz", 156)]).results[0].ok).toBe(true);
    expect(run(p, [song("waltz", 144), song("polka", 114), song("waltz", 144)]).results[0].ok).toBe(true);
  });

  it("limits runs of fast/slow styles", () => {
    const p = only(powers(), { type: "maxConsecutive", energy: "fast", count: 2 });
    // polka, salsa, quickstep are "fast" in the Powers profile.
    let r = run(p, [song("polka", 114), song("salsa", 94), song("quickstep", 200), song("waltz", 144)]);
    expect(r.results[0].ok).toBe(false);
    expect(r.results[0].entryIds).toEqual(["e1", "e2", "e3"]);
    r = run(p, [song("polka", 114), song("salsa", 94), song("waltz", 144), song("quickstep", 200)]);
    expect(r.results[0].ok).toBe(true);
  });
});

describe("full profiles", () => {
  it("runs every built-in rule without throwing and reports counts", () => {
    for (const prof of [dancebreak(), powers()]) {
      const songs = prof.styles.map((s) => song(s.key, s.sweet ? (s.sweet[0] + s.sweet[1]) / 2 : null));
      const r = run(prof, songs, { lesson: prof.styles[0].key });
      expect(r.results.length).toBeGreaterThanOrEqual(prof.rules.length);
      for (const x of r.results) {
        expect(x.label.length).toBeGreaterThan(0);
        expect(x.detail.length).toBeGreaterThan(0);
      }
      for (const s of prof.styles) expect(r.counts.some((c) => c.key === s.key)).toBe(true);
      expect(r.totalMs).toBe(songs.length * 180_000);
    }
  });

  it("indexes issues by entry and includes Dancebreak's labelled rules", () => {
    const r = run(dancebreak(), [song("rotary-waltz", 150, 300)], { lesson: "cross-step-waltz" });
    const first10 = byLabel(r, "First 10 songs under 3:30");
    expect(first10.ok).toBe(false);
    expect(r.issuesByEntry.get("e1")).toContain(first10);
    const counts = r.counts.find((c) => c.key === "cross-step-waltz")!;
    expect(counts.min).toBe(9);
    expect(r.counts.find((c) => c.key === "@lesson")!.min).toBe(7);
  });
});

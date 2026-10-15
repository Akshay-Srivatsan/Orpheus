import { describe, expect, it } from "vitest";
import { evaluate, type SmartContext, type SmartRules } from "./smart";
import { emptyClub, type Club, type ClubStyle } from "./clubs";
import type { ClubContext } from "./styles";
import type { Crate, Row, Track, Version } from "./types";

const style = (name: string, tempo: [number, number], preferred: [number, number]): ClubStyle => ({
  name, meter: [3], tempo: { min: tempo[0], max: tempo[1] }, preferred: { min: preferred[0], max: preferred[1] },
  length: null, count: null, share: null, when: [], first: false, last: false,
});
// Waltzes like the Dancebreak example's, and your styles mapped onto them.
const club: Club = {
  ...emptyClub("Club"),
  style: { rotary: style("Rotary Waltz", [135, 160], [140, 155]), "cross-step": style("Cross-Step Waltz", [105, 125], [110, 120]) },
};
const clubCtx: ClubContext = {
  club,
  mapping: { "rotary-waltz": ["rotary"], "cross-step-waltz": ["cross-step"] },
  tempoChange: { faster: 10, slower: 8 },
};

let seq = 0;
function row(style: string | null, bpm: number | null, extra: Partial<Track> = {}, vextra: Partial<Version> = {}): Row {
  const id = `t${++seq}`;
  const track: Track = {
    id, title: id, artist: "", album: "", durationMs: 180_000, bpm, bpmSource: "user",
    rating: null, loudnessDb: null, audioStartMs: null, audioEndMs: null, analyzed: true, addedAt: 0,
    sources: [{ kind: "file", key: id }], versions: [], files: ["/x"], missing: false, playCount: 0, lastPlayedAt: null,
    ...extra,
  };
  const version: Version = { id: `v${seq}`, name: "Original", styles: style ? [style] : [], ...vextra };
  track.versions = [version];
  return { track, version };
}

const ids = (rs: Row[]) => rs.map((r) => r.version.id);

describe("smart crates", () => {
  // Rotary sweet 140–155 (ok 135–160); cross-step 110–120 (ok 105–125).
  const rotary = row("rotary-waltz", 150);
  const csw = row("cross-step-waltz", 116);
  const unlabelledRotaryTempo = row(null, 148);
  const unlabelledSlow = row(null, 112, { beatsPerBar: 3 }, { tags: ["classic"] });
  const spotify = row(null, 96, { sources: [{ kind: "spotify", key: "spotify:track:x" }], durationMs: 300_000 });
  const rows = [rotary, csw, unlabelledRotaryTempo, unlabelledSlow, spotify];
  const waltzes: Crate = { id: "w", name: "Waltzes", position: 1, versionIds: [rotary.version.id, csw.version.id, unlabelledRotaryTempo.version.id, unlabelledSlow.version.id] };
  const ctx: SmartContext = { club: clubCtx, styles: null, crates: [waltzes], library: rows, now: 10 * 86_400_000, trimSilence: false };
  const run = (rules: SmartRules) => ids(evaluate(rules, rows, ctx));

  it("filters by style label", () => {
    expect(run({ match: "all", conditions: [{ field: "style", op: "is", value: "rotary-waltz" }] })).toEqual([rotary.version.id]);
    expect(run({ match: "all", conditions: [{ field: "style", op: "isEmpty" }] })).toHaveLength(3);
  });

  it("disambiguates a mixed waltz crate by tempo", () => {
    const rotaryish: SmartRules = {
      match: "all",
      conditions: [
        { field: "crate", op: "in", value: "w" },
        { field: "bpm", op: "fitsStyle", value: "rotary-waltz" },
      ],
    };
    expect(run(rotaryish)).toEqual([rotary.version.id, unlabelledRotaryTempo.version.id]);
    const cross: SmartRules = { ...rotaryish, conditions: [rotaryish.conditions[0], { field: "bpm", op: "fitsStyle", value: "cross-step-waltz" }] };
    expect(run(cross)).toEqual([csw.version.id, unlabelledSlow.version.id]);
  });

  it("supports numeric, text, source and 'any' matching", () => {
    expect(run({ match: "all", conditions: [{ field: "bpm", op: "between", value: [110, 120] }] })).toEqual([csw.version.id, unlabelledSlow.version.id]);
    expect(run({ match: "all", conditions: [{ field: "duration", op: "atLeast", value: 240 }] })).toEqual([spotify.version.id]);
    expect(run({ match: "all", conditions: [{ field: "source", op: "is", value: "spotify" }] })).toEqual([spotify.version.id]);
    expect(run({ match: "all", conditions: [{ field: "tag", op: "has", value: "classic" }] })).toEqual([unlabelledSlow.version.id]);
    expect(run({ match: "all", conditions: [{ field: "meter", op: "triple" }] })).toEqual([unlabelledSlow.version.id]);
    expect(run({ match: "any", conditions: [{ field: "style", op: "is", value: "rotary-waltz" }, { field: "source", op: "is", value: "spotify" }] })).toEqual([rotary.version.id, spotify.version.id]);
    expect(run({ match: "all", conditions: [{ field: "lastPlayed", op: "never" }] })).toHaveLength(5);
    expect(run({ match: "all", conditions: [] })).toEqual([]);
  });

  it("can refer to another smart crate without looping forever", () => {
    const a: Crate = { id: "a", name: "A", position: 2, versionIds: [], smart: { match: "all", conditions: [{ field: "crate", op: "in", value: "b" }] } };
    const b: Crate = { id: "b", name: "B", position: 3, versionIds: [], smart: { match: "all", conditions: [{ field: "crate", op: "in", value: "a" }] } };
    const c2: SmartContext = { ...ctx, crates: [a, b] };
    expect(evaluate(a.smart!, rows, c2)).toEqual([]);
  });
});

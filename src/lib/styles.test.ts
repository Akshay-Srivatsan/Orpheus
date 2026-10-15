import { describe, expect, it } from "vitest";
import { emptyClub, type Club, type ClubStyle } from "./clubs";
import { clubContext, projStyle, sameName, shownStyles, songFacts } from "./styles";
import type { ProjEntry, StyleLibrary, Track, Version } from "./types";

const style = (name: string, meter: number[], tempo: [number, number] | null): ClubStyle => ({
  name, short: name.slice(0, 3).toUpperCase(), color: "#123456", meter, tempo: tempo && { min: tempo[0], max: tempo[1] }, preferred: null,
  length: null, count: null, share: null, when: [], first: false, last: false,
});
const club: Club = {
  ...emptyClub("Club"),
  set: { ...emptyClub("").set, tempo_change: { faster: 5, slower: 5 } },
  style: { rotary: style("Rotary Waltz", [3], [135, 160]), "cross-step": style("Cross-Step Waltz", [3], [105, 125]), polka: style("Polka", [2], [105, 130]) },
};
const lib: StyleLibrary = {
  styles: [
    { key: "waltz", name: "Waltz", short: "W", color: "#111111" },
    { key: "kizomba", name: "Kizomba", short: "K", color: "#222222" },
    { key: "my-polka", name: "polka", short: "", color: "#333333" },
  ],
  clubRules: { club: [{ mine: "waltz", club: "rotary" }, { mine: "waltz", club: "cross-step" }] },
};
const track = (bpm: number, meter: number): Track => ({
  id: "t", title: "Song", artist: "", album: "", durationMs: 180_000, bpm, bpmSource: "detected", beatsPerBar: meter, meterSource: "detected",
  rating: null, loudnessDb: null, audioStartMs: null, audioEndMs: null, analyzed: true, addedAt: 0, sources: [], versions: [],
  files: [], missing: false, playCount: 0, lastPlayedAt: null,
});
const version = (styles: string[]): Version => ({ id: "v", name: "Original", styles });

describe("what songs show", () => {
  const ctx = clubContext(club, "club", lib, 10);
  const names = (t: Track, v: Version) => shownStyles(lib, ctx, songFacts(t, v, null), v, null).map((s) => s.name);

  it("your own styles without a club", () => {
    expect(shownStyles(lib, null, null, version(["waltz"]), null).map((s) => s.name)).toEqual(["Waltz"]);
  });

  it("your mapping, narrowed by tempo", () => {
    expect(names(track(145, 3), version(["waltz"]))).toEqual(["Rotary Waltz"]);
    expect(names(track(115, 3), version(["waltz"]))).toEqual(["Cross-Step Waltz"]);
  });

  it("unmapped styles show every match", () => {
    expect(names(track(118, 4), version(["kizomba"]))).toEqual(["Polka"]);
    expect(names(track(118, 4), version([]))).toEqual(["Polka"]);
  });

  it("same names need no rule", () => {
    expect(ctx?.mapping["my-polka"]).toEqual(["polka"]);
    expect(ctx?.mapping.waltz).toEqual(["rotary", "cross-step"]);
    expect(ctx?.mapping.kizomba).toBeUndefined();
  });

  it("names match whatever their spacing and hyphens", () => {
    for (const n of ["crossstep waltz", "Cross-Step Waltz", "cross step  waltz", "Cross–step waltz"]) expect(sameName(n, "Cross-step Waltz")).toBe(true);
    expect(sameName("Night Club Two-Step", "Nightclub Two Step")).toBe(true);
    expect(sameName("Rotary Waltz", "Waltz")).toBe(false);
  });

  it("a song needing a tempo change it isn't given warns in a set", () => {
    const v = version(["waltz"]);
    const t = track(133, 3);
    // Rotary needs +1.5%; Cross-Step would need −6%, more than the club's 5%.
    const inSet = shownStyles(lib, ctx, songFacts(t, v, {}), v, {});
    expect(inSet.map((s) => [s.name, s.fit, s.bpm])).toEqual([["Rotary Waltz", "stretch", 135]]);
    expect(inSet[0].problems).toEqual(["plays at 133 BPM; Rotary Waltz is 135–160 BPM (+1.5% would fit)"]);
    // In the library, it's just a fit.
    expect(shownStyles(lib, ctx, songFacts(t, v, null), v, null)[0].problems).toEqual([]);
    // Given the tempo, it fits.
    const e = { overrides: { targetBpm: 135 } };
    expect(shownStyles(lib, ctx, songFacts(t, v, e), v, e)[0].problems).toEqual([]);
  });

  it("a slot shows its chosen style", () => {
    expect(shownStyles(lib, ctx, null, undefined, { clubStyles: ["polka"] }).map((s) => s.name)).toEqual(["Polka"]);
  });

  it("the projector resolves the same way", () => {
    const e: ProjEntry = { id: "e", trackId: "t", title: "", artist: "", styles: ["waltz"], clubStyles: null, meter: 3, lengthSecs: 180, clubListed: [], label: null, tags: [], status: "pending", bpm: 147 };
    expect(projStyle({ club, clubId: "club", styleLibrary: lib, tempoRangePct: 10 }, e).name).toBe("Rotary Waltz");
    expect(projStyle({ club, clubId: "club", styleLibrary: lib, tempoRangePct: 10 }, { ...e, label: "Mixer" }).name).toBe("Mixer");
  });
});

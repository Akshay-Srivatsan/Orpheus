import { describe, expect, it } from "vitest";
import { clubStyle, clubTargets, entryLabel, projStyle, showStyles } from "./styles";
import type { Profile, StyleLibrary, Version } from "./types";

const profileModules = import.meta.glob("../../src-tauri/profiles/*.json", { eager: true, import: "default" });
const builtin = (id: string) =>
  structuredClone(Object.entries(profileModules).find(([p]) => p.endsWith(`/${id}.json`))![1]) as Profile;

const club = builtin("dancebreak");
const none = builtin("none");

// Your styles: one "waltz" covering what the club splits by tempo, plus a few others.
const lib: StyleLibrary = {
  styles: [
    { key: "waltz", name: "Waltz", short: "W", color: "#111111" },
    { key: "lindy", name: "Lindy Hop", short: "Lindy", color: "#222222" },
    { key: "my-polka", name: "Polka", short: "P", color: "#333333" },
    { key: "kizomba", name: "Kizomba", short: "K", color: "#444444" },
  ],
  mappings: {
    dancebreak: [
      { style: "waltz", minBpm: 100, maxBpm: 128, to: "cross-step-waltz" },
      { style: "waltz", minBpm: 129, maxBpm: 165, to: "rotary-waltz" },
      { style: "waltz", to: "" },
      { style: "lindy", to: "lindy-hop" },
    ],
  },
};

describe("club styles", () => {
  it("maps your styles by tempo, first rule first", () => {
    expect(clubStyle(lib, club, "waltz", 115)?.key).toBe("cross-step-waltz");
    expect(clubStyle(lib, club, "waltz", 150)?.key).toBe("rotary-waltz");
    // Half time counts in the range's own counting.
    expect(clubStyle(lib, club, "waltz", 75)?.key).toBe("rotary-waltz");
    // Outside every range: the catch-all rule says not danced here.
    expect(clubStyle(lib, club, "waltz", 190)).toBeNull();
    expect(clubStyle(lib, club, "waltz", null)).toBeNull();
  });

  it("falls back to the same key, then the same name", () => {
    expect(clubStyle(lib, club, "lindy", 140)?.key).toBe("lindy-hop");
    expect(clubStyle(lib, club, "my-polka", null)?.name).toBe("Polka");
    expect(clubStyle(lib, club, "kizomba", null)).toBeNull();
  });

  it("shows your own styles under No club", () => {
    expect(clubStyle(lib, none, "waltz", 150)).toBeNull();
    const shown = showStyles(lib, none, ["waltz", "lindy"], 150);
    expect(shown.map((s) => s.name)).toEqual(["Waltz", "Lindy Hop"]);
    expect(shown.every((s) => !s.unmapped)).toBe(true);
  });

  it("deduplicates styles that land on the same club style, and uses labels", () => {
    const l2: StyleLibrary = { ...lib, mappings: { dancebreak: [{ style: "my-polka", to: "lindy-hop" }] } };
    expect(showStyles(l2, club, ["lindy", "my-polka"], 140).map((s) => s.key)).toEqual(["lindy"]);
    const v: Version = { id: "v", name: "Original", styles: ["waltz", "lindy"] };
    expect(entryLabel(lib, club, v, null, 150)).toBe("Rotary Waltz / Lindy Hop");
    expect(entryLabel(lib, club, v, { styles: ["lindy"] }, 150)).toBe("Lindy Hop");
    expect(entryLabel(lib, club, v, { label: "Mixer" }, 150)).toBe("Mixer");
  });

  it("lists every club style a style can become", () => {
    expect(clubTargets(lib, club, "waltz").map((s) => s.key)).toEqual(["cross-step-waltz", "rotary-waltz"]);
    expect(clubTargets(lib, none, "waltz")).toEqual([]);
  });

  it("leaves styles the club doesn't dance off the projector", () => {
    const p = { profile: club, styleLibrary: lib };
    const e = { id: "e", trackId: null, title: "", artist: "", styles: ["kizomba", "lindy"], label: null, tags: [], status: "pending" as const, bpm: 140 };
    expect(projStyle(p, e).name).toBe("Lindy Hop");
    expect(projStyle(p, { ...e, styles: ["kizomba"] }).name).toBe("Kizomba");
  });
});

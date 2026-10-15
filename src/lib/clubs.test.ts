import { describe, expect, it } from "vitest";
import { assess, checkSet, clubShown, emptyClub, matches, type Club, type ClubStyle, type SetItem, type SongFacts } from "./clubs";

const R = (min: number | null, max: number | null) => ({ min, max });
const style = (s: Partial<ClubStyle> & { name: string }): ClubStyle => ({
  meter: [],
  tempo: null,
  preferred: null,
  length: null,
  count: null,
  share: null,
  when: [],
  first: false,
  last: false,
  ...s,
});

// A small club like the Dancebreak example.
const club: Club = {
  ...emptyClub("Club"),
  set: { ...emptyClub("").set, songs: R(4, 6), tempo_change: { faster: 10, slower: 8 }, max_in_a_row: 2 },
  style: {
    "cross-step": style({ name: "Cross-Step Waltz", meter: [3], tempo: R(105, 125), preferred: R(110, 120), count: R(2, null) }),
    rotary: style({ name: "Rotary Waltz", meter: [3], tempo: R(135, 160), preferred: R(140, 155), last: true }),
    polka: style({ name: "Polka", meter: [2], tempo: R(105, 130), when: [R(25, 75)] }),
    lindy: style({ name: "Lindy Hop", meter: [4], tempo: R(125, 160), length: R(null, 210) }),
    mixer: style({ name: "Mixer" }),
  },
};
const tc = club.set.tempo_change!;
const song = (bpm: number | null, meter: number | null, extra: Partial<SongFacts> = {}): SongFacts => ({
  bpm,
  target: null,
  meter,
  meterDetected: true,
  lengthSecs: 180,
  ...extra,
});

describe("matching", () => {
  it("matches by meter and tempo, best fit first", () => {
    expect(matches(club, song(115, 3), tc).map((a) => a.id)).toEqual(["cross-step"]);
    expect(matches(club, song(148, 3), tc)[0].id).toBe("rotary");
    // Duple songs aren't waltzes; Lindy Hop needs +8.7 % (allowed), so it comes last.
    // A style without a tempo (Mixer) isn't a match by itself.
    expect(matches(club, song(115, 4), tc).map((a) => a.id)).toEqual(["polka", "lindy"]);
  });

  it("counts half and double time", () => {
    expect(matches(club, song(74, 3), tc)[0].id).toBe("rotary"); // 148 in half time
  });

  it("reaches a range by changing tempo, within what the club allows", () => {
    const a = assess("rotary", club.style.rotary, song(128, 3), tc); // +5.5% to 135
    expect(a.ok).toBe(true);
    expect(a.fit).toBe("stretch");
    expect(a.rate).toBeCloseTo(135 / 128, 3);
    const far = assess("rotary", club.style.rotary, song(122, 3), tc); // +10.7%: too much
    expect(far.ok).toBe(false);
    expect(far.problems[0]).toMatch(/needs \+10\.7%/);
    // Slowing down is limited separately (8 %).
    expect(assess("rotary", club.style.rotary, song(172, 3), tc).ok).toBe(true); // −7 %
    expect(assess("rotary", club.style.rotary, song(176, 3), tc).ok).toBe(false); // −9.1 %
  });

  it("a set tempo isn't changed further", () => {
    expect(assess("rotary", club.style.rotary, song(128, 3, { target: 128 }), tc).ok).toBe(false);
    expect(assess("rotary", club.style.rotary, song(128, 3, { target: 145 }), tc).fit).toBe("preferred");
  });

  it("checks length at the playing tempo", () => {
    expect(assess("lindy", club.style.lindy, song(140, 4, { lengthSecs: 240 }), tc).problems[0]).toMatch(/4:00 long/);
    expect(assess("lindy", club.style.lindy, song(140, 4, { lengthSecs: 200 }), tc).ok).toBe(true);
  });

  it("a detected 4 may be 2/4; a meter set by hand must match", () => {
    expect(assess("polka", club.style.polka, song(115, 4), tc).ok).toBe(true);
    expect(assess("polka", club.style.polka, song(115, 4, { meterDetected: false }), tc).ok).toBe(false);
    // Unknown meter: not ruled out.
    expect(assess("polka", club.style.polka, song(115, null), tc).ok).toBe(true);
  });
});

describe("suggested songs", () => {
  const withSongs: Club = {
    ...club,
    style: {
      ...club.style,
      // A choreography: no meter, tempo or length; only its suggested songs.
      bnp: style({ name: "Bohemian National Polka", song: [{ title: "BNP", hash: "a0" }] }),
      // A style with criteria and suggestions.
      polka: { ...club.style.polka, song: [{ title: "Slow polka", hash: "a1" }] },
    },
  };

  it("a choreography matches only its suggested songs", () => {
    expect(matches(withSongs, song(118, 4), tc).map((a) => a.id)).not.toContain("bnp");
    expect(matches(withSongs, song(118, 4, { listed: ["bnp"] }), tc)[0].id).toBe("bnp");
    expect(assess("bnp", withSongs.style.bnp, song(118, 4), tc).problems[0]).toMatch(/not one of the songs suggested/);
  });

  it("a listed song shows only the styles that list it", () => {
    // 118 BPM duple would also fit Polka by tempo; the club says it's a BNP.
    const shown = clubShown(withSongs, song(118, 4, { listed: ["bnp"] }), tc, null, [], undefined);
    expect(shown).toMatchObject({ ids: ["bnp"], source: "listed" });
    // The DJ's choice still wins.
    expect(clubShown(withSongs, song(118, 4, { listed: ["bnp"] }), tc, ["polka"], [], undefined).ids).toEqual(["polka"]);
  });

  it("other styles take suggested songs, whatever their tempo, and matching songs from the library", () => {
    const slow = song(90, 4, { meterDetected: false, listed: ["polka"] }); // a polka at 90 BPM, which the club suggests
    expect(assess("polka", withSongs.style.polka, slow, tc)).toMatchObject({ ok: true, fit: "listed" });
    expect(matches(withSongs, song(115, 4), tc).map((a) => a.id)).toContain("polka");
  });
});

describe("what a song shows", () => {
  const mapping = { waltz: ["cross-step", "rotary"], "my-polka": ["polka"] };

  it("your mapping picks among the styles the song matches", () => {
    expect(clubShown(club, song(115, 3), tc, null, ["waltz"], mapping).ids).toEqual(["cross-step"]);
    expect(clubShown(club, song(148, 3), tc, null, ["waltz"], mapping).ids).toEqual(["rotary"]);
  });

  it("a mapped song that fits none of its styles still shows them, with the reason", () => {
    const s = clubShown(club, song(95, 4, { meterDetected: false }), tc, null, ["my-polka"], mapping);
    expect(s.ids).toEqual(["polka"]);
    expect(s.fits.polka.problems.length).toBeGreaterThan(0);
  });

  it("without a mapping, every match", () => {
    expect(clubShown(club, song(115, 3), tc, null, ["kizomba"], mapping).ids).toEqual(["cross-step"]);
  });

  it("the DJ's choice wins, even a style the song doesn't match", () => {
    const s = clubShown(club, song(115, 3), tc, ["rotary"], ["waltz"], mapping);
    expect(s.source).toBe("chosen");
    expect(s.ids).toEqual(["rotary"]);
    expect(s.fits.rotary.ok).toBe(false);
  });
});

describe("set check", () => {
  const item = (title: string, ids: string[], lengthSecs = 180): SetItem => ({ entryId: title, title, ids, fits: {}, lengthSecs, pauseSecs: 8 });

  it("counts, positions, in a row, and the last song", () => {
    const c = checkSet(club, [item("a", ["polka"]), item("b", ["cross-step"]), item("c", ["mixer"]), item("d", ["mixer"]), item("e", ["mixer"])]);
    const text = c.issues.map((i) => i.text);
    expect(text.some((t) => /a: Polka at song 1 \(0% through\)/.test(t))).toBe(true);
    expect(text).toContain("3 Mixer in a row (at most 2)");
    expect(text).toContain("The set should end with a Rotary Waltz");
    // One cross-step of the two expected: a note while the set isn't finished.
    const cs = c.issues.find((i) => i.text.startsWith("Cross-Step Waltz"));
    expect(cs?.level).toBe("note");
    expect(checkSet(club, [item("x", ["cross-step"])], { complete: true }).issues.find((i) => i.text.startsWith("Cross-Step"))?.level).toBe("warn");
    expect(c.tallies.find((t) => t.key === "songs")).toMatchObject({ have: 5, ok: true });
  });

  it("rules for part of the set", () => {
    const withRule: Club = { ...club, rule: { early: { name: "First 2 songs", songs: R(1, 2), when: [], length: R(null, 200), styles: [], count: null } } };
    const c = checkSet(withRule, [item("long", ["mixer"], 240), item("ok", ["mixer"]), item("late-long", ["rotary"], 240)]);
    const text = c.issues.map((i) => i.text);
    expect(text).toContain("long: 4:00 long; First 2 songs: at most 3:20");
    expect(text.some((t) => t.startsWith("late-long: 4:00"))).toBe(false);
  });

  it("songs without a club style are flagged", () => {
    const c = checkSet(club, [item("odd", [])]);
    expect(c.issues[0].text).toBe("odd: not one of Club's styles");
  });
});

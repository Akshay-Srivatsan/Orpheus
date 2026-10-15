// Mock projector data for designing the projector page in a plain browser.
// URL params: ?mode=live|preset&gap=1&message=...&theme=light&scale=1.2&idle=1

import { emptyClub, type Club } from "./clubs";
import type { DeckState, ProjEntry, ProjectorPayload, ProjectorSettings } from "./types";

const STYLES: Record<string, [string, string, string]> = {
  csw: ["Cross-Step Waltz", "CSW", "#a78bfa"],
  rw: ["Rotary Waltz", "RW", "#f472b6"],
  lh: ["Lindy Hop", "LH", "#fbbf24"],
  wcs: ["West Coast Swing", "WCS", "#34d399"],
  pk: ["Polka", "PK", "#facc15"],
  fus: ["Fusion", "FUS", "#818cf8"],
  sal: ["Salsa", "SAL", "#f97316"],
  line: ["Line Dance", "LINE", "#2dd4bf"],
  mix: ["Mixer", "MIX", "#4ade80"],
  chor: ["Choreography (BNP etc.)", "CHOR", "#38bdf8"],
  fw: ["Fast Waltz", "FW", "#fb7185"],
};

const SONGS: [string, string, string, string[]?][] = [
  ["csw", "A Thousand Years", "Christina Perri"],
  ["lh", "Cantina Band", "John Williams"],
  ["csw", "Rainbow Connection", "Kermit"],
  ["wcs", "Dazed & Confused", "Ruel"],
  ["rw", "Once Upon a December", "Christy Altomare"],
  ["csw", "Leaves of October", "Andrea Perry", ["dance-with-stranger"]],
  ["pk", "Ievan Polkka (Remastered 2024)", "Hatsune Miku"],
  ["lh", "Shivers", "Ed Sheeran"],
  ["rw", "Le Festin", "Camille"],
  ["fus", "Diamonds Are Forever", "Sabrina Carpenter"],
  ["csw", "Songs I'll Never Write", "Narrowhaven"],
  ["wcs", "Spinnin", "Madison Beer"],
  ["line", "Barbie Line Dance", "Dua Lipa"],
  ["rw", "Isabella's Lullaby", "Takahiro Obata"],
  ["csw", "Stargazing", "Myles Smith"],
  ["lh", "Why Don't You Love Me", "Hot Chelle Rae"],
  ["sal", "Wavin' Flag", "K'naan"],
  ["rw", "Illumielle", "Jo Blankenburg"],
  ["csw", "Nice To Meet You", "Myles Smith", ["jam"]],
  ["wcs", "Never Love You Again", "Post Malone"],
  ["mix", "Ring Ding (A Scotsman's Story)", "Nathan Evans"],
  ["rw", "Amas Veritas", "Alan Silvestri"],
  ["csw", "Forever Like That", "Ben Rector"],
  ["lh", "Try Everything", "Shakira"],
  ["chor", "Bohemian National Polka", "Cincinnati Pops"],
  ["fw", "Lullaby of Flame (Accelerating)", "DDRKirby(ISQ)"],
  ["csw", "Crazier", "Taylor Swift"],
  ["rw", "Merry-Go-Round of Life", "Joe Hisaishi"],
  ["csw", "You And Me", "Lifehouse"],
  ["wcs", "There Goes My Baby", "d4vd", ["last"]],
  ["lh", "Keep Lying", "Donna Missal", ["last"]],
  ["csw", "The Vow", "Ed Sheeran", ["last"]],
  ["rw", "Erin Shore", "The Corrs", ["last"]],
];

const TAGS: Record<string, [string, string]> = {
  "dance-with-stranger": ["Dance with a Stranger", "#f0abfc"],
  jam: ["Jam", "#fde047"],
  last: ["Last of style", "#fca5a5"],
};

/** A club whose style ids are the mock's (entries choose them directly). */
function mockClub(): Club {
  const style: Club["style"] = {};
  for (const [id, [name, short, color]] of Object.entries(STYLES)) {
    style[id] = { name, short, color, meter: [], tempo: null, preferred: null, length: null, count: null, share: null, when: [], first: false, last: false };
  }
  return { ...emptyClub("Mock club"), style };
}

export function mockSettings(p: URLSearchParams): ProjectorSettings {
  return {
    title: "Dancebreak — Fall Social",
    displayMode: "auto",
    historyCount: 5,
    upcomingCount: Number(p.get("upcoming") ?? 0),
    showNext: true,
    revealNextInGap: true,
    showArtist: true,
    showBpm: p.has("bpm"),
    showClock: true,
    showArtwork: true,
    fontScale: Number(p.get("scale") ?? 1),
    theme: p.get("theme") === "light" ? "light" : "dark",
  };
}

export function mockPayload(p: URLSearchParams): ProjectorPayload {
  const mode = p.get("mode") === "preset" ? "preset" : "live";
  const gap = p.has("gap");
  const idle = p.has("idle");
  const cur = Number(p.get("index") ?? 11);
  // ?n=60: a longer set (songs repeat).
  const count = Number(p.get("n") ?? SONGS.length);
  const songs = Array.from({ length: count }, (_, i) => SONGS[i % SONGS.length]);
  const entries: ProjEntry[] = songs.map(([st, title, artist, tags], i) => {
    return {
      id: `e${i}`,
      trackId: null,
      title,
      artist,
      styles: [],
      clubStyles: [st],
      meter: null,
      lengthSecs: null,
      clubListed: [],
      label: null,
      tags: (tags ?? []).map((t) => ({ id: t, name: TAGS[t][0], color: TAGS[t][1] })),
      status: i < cur ? (i === 3 ? "skipped" : "played") : i === cur && !gap ? "playing" : "pending",
      bpm: 100 + ((i * 17) % 60),
    };
  });
  if (idle) {
    return {
      title: mockSettings(p).title,
      mode,
      settings: mockSettings(p),
      current: null,
      next: null,
      history: [],
      entries: [],
      currentIndex: null,
      inGap: false,
      playing: false,
      message: p.get("message"),
      liveDeck: 0,
      club: mockClub(),
      clubId: "mock",
      tempoRangePct: 10,
      styleLibrary: { styles: [] },
    };
  }
  const currentIndex = gap ? cur - 1 : cur;
  const current = entries[currentIndex] ?? null;
  const next = entries.slice(currentIndex + 1).find((e) => e.status === "pending") ?? null;
  const history = entries
    .slice(0, currentIndex)
    .filter((e) => e.status === "played")
    .reverse()
    .slice(0, 5);
  return {
    title: mockSettings(p).title,
    mode,
    settings: mockSettings(p),
    current,
    next,
    history,
    entries,
    currentIndex,
    inGap: gap,
    playing: !gap,
    message: p.get("message"),
    liveDeck: 0,
    club: mockClub(),
    clubId: "mock",
    tempoRangePct: 10,
    styleLibrary: { styles: [] },
  };
}

export function mockDeck(): DeckState {
  return {
    deck: 0,
    status: "playing",
    trackId: "t1",
    recordingId: "t1",
    versionId: "v1",
    entryId: "e11",
    positionMs: 74000,
    durationMs: 166916,
    startMs: 230,
    endMs: 163530,
    fadeAtMs: null,
    cueMs: 230,
    onAir: true,
    stems: null,
    fading: false,
    tempo: 1,
    keylock: true,
    gainDb: 0,
    trimDb: 0,
    pfl: false,
    error: null,
  };
}

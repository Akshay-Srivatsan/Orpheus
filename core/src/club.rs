//! Club guidelines: what a club's sets should contain. A club defines its
//! dance styles by meter, tempo and length, and can say how many of each it
//! expects, how many songs in all, and where in the set a style belongs.
//!
//! Clubs publish their guidelines as a human-editable TOML file (on their
//! website, or shared as a file); see `examples/clubs/`. Orpheus keeps every
//! version it has imported, exactly as received: a set stays with the
//! guidelines it was planned with until the DJ moves it to a newer version.
//!
//! On disk: `<library>/clubs/<club>/<version>.toml`, plus `versions.toml`
//! naming each version, when it was imported and where from.
//!
//! Matching songs to styles and checking sets happen in the interface
//! (`src/lib/clubs.ts`); this module reads, writes and validates the files.

use anyhow::{anyhow, bail, Context, Result};
use indexmap::IndexMap;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------- definition

/// A club's guidelines (one version of its file).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Club {
    pub name: String,
    pub description: String,
    /// The club's page about its guidelines, for people.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,
    /// Where the club publishes this file (for "Check for updates").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub set: SetRules,
    /// `[style.<id>]`: by id (sets and your mappings refer to it; the name
    /// shown is `name`), in file order.
    #[serde(rename = "style")]
    pub styles: IndexMap<String, Style>,
    /// `[tag.<id>]`
    #[serde(rename = "tag")]
    pub tags: IndexMap<String, Tag>,
    /// `[group.<id>]`: counts across several styles ("at least one Latin").
    #[serde(rename = "group")]
    pub groups: IndexMap<String, Group>,
    /// `[rule.<id>]`: rules for part of the set ("the first 10 songs under 3:30").
    #[serde(rename = "rule")]
    pub rules: IndexMap<String, Rule>,
    /// Reminders the DJ can announce (Announce menu).
    pub announcements: Vec<String>,
    /// Song libraries the club's styles suggest songs from (`suggested`):
    /// files next to the club file, or web addresses. Several clubs can share one.
    #[serde(deserialize_with = "strings", skip_serializing_if = "Vec::is_empty")]
    pub song_library: Vec<String>,
    /// Those libraries' songs, as fetched with this version (by id).
    #[serde(skip)]
    pub library: IndexMap<String, ClubSong>,
    /// Fingerprints songs refer to by file or web address, as fetched when
    /// the version was imported (reference as written → fingerprint).
    #[serde(skip)]
    pub fingerprints: HashMap<String, String>,
}

impl Club {
    /// A song's fingerprint text: written in the file, or fetched from where it points.
    pub fn fingerprint_of<'a>(&'a self, song: &'a ClubSong) -> Option<&'a str> {
        let f = song.fingerprint.as_deref()?;
        if is_fingerprint_ref(f) {
            self.fingerprints.get(f).map(|s| s.as_str())
        } else {
            Some(f)
        }
    }

    /// Fills each style's `library_songs` from `library` (by `suggested` id).
    pub fn attach_library(&mut self) {
        for st in self.styles.values_mut() {
            st.library_songs = st
                .suggested
                .iter()
                .filter_map(|id| self.library.get(id).map(|s| ClubSong { id: Some(id.clone()), ..s.clone() }))
                .collect();
        }
    }

    /// Suggested song ids no library has.
    pub fn missing_songs(&self) -> Vec<String> {
        self.styles.values().flat_map(|s| &s.suggested).filter(|id| !self.library.contains_key(*id)).cloned().collect()
    }

    /// Fingerprint references (file paths or web addresses) its own songs use.
    pub fn fingerprint_refs(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .styles
            .values()
            .flat_map(|s| &s.songs)
            .filter_map(|s| s.fingerprint.clone())
            .filter(|f| is_fingerprint_ref(f))
            .collect();
        out.sort();
        out.dedup();
        out
    }
}

/// The club as one self-contained file (to publish, or to embed in a set
/// file): the song libraries' suggested songs written into their styles, and
/// fingerprint references replaced by the fingerprints.
pub fn self_contained(club: &Club) -> Club {
    let mut c = club.clone();
    for st in c.styles.values_mut() {
        let from_library: Vec<ClubSong> = std::mem::take(&mut st.library_songs).into_iter().map(|s| ClubSong { id: None, ..s }).collect();
        st.songs.extend(from_library);
        st.suggested.clear();
        for s in st.songs.iter_mut() {
            if let Some(f) = s.fingerprint.clone().filter(|f| is_fingerprint_ref(f)) {
                s.fingerprint = club.fingerprints.get(&f).cloned();
            }
        }
    }
    c.song_library.clear();
    c.library.clear();
    c.fingerprints.clear();
    c
}

/// A fingerprint value that points elsewhere (fingerprint text itself is
/// letters, digits, `-` and `_` only).
pub fn is_fingerprint_ref(s: &str) -> bool {
    s.contains(['/', ':', '.'])
}

/// The fingerprint in a referenced file: just the fingerprint, or `fpcalc`'s
/// output (`FINGERPRINT=…`).
pub fn read_fingerprint_file(text: &str) -> Result<String> {
    let fp = text
        .lines()
        .find_map(|l| l.trim().strip_prefix("FINGERPRINT="))
        .unwrap_or(text)
        .trim()
        .to_string();
    crate::fingerprint::decode(&fp)?;
    Ok(fp)
}

/// A fingerprint file bundled with the example clubs.
pub fn bundled_fingerprint(r: &str) -> Result<String> {
    EXAMPLE_FINGERPRINTS
        .iter()
        .find(|(name, _)| r.trim_start_matches("./") == *name)
        .map(|(_, t)| t.to_string())
        .ok_or_else(|| anyhow!("no bundled file \"{r}\""))
}

/// Where a reference points: a web address as is; a relative one against
/// where the club file came from (its web address or its folder).
pub fn resolve_ref(base: &str, r: &str) -> String {
    if r.starts_with("https://") || r.starts_with("http://") {
        return r.to_string();
    }
    if base.starts_with("https://") || base.starts_with("http://") {
        if let Ok(u) = reqwest::Url::parse(base).and_then(|b| b.join(r)) {
            return u.to_string();
        }
    }
    let p = Path::new(r);
    if p.is_absolute() {
        return r.to_string();
    }
    Path::new(base).parent().map(|d| d.join(p).to_string_lossy().into_owned()).unwrap_or_else(|| r.to_string())
}

/// Fetches the fingerprints a club refers to (those not already in `known`).
pub async fn fetch_fingerprints(club: &Club, base: &str, known: &HashMap<String, String>) -> Result<HashMap<String, String>> {
    let mut out = HashMap::new();
    for r in club.fingerprint_refs() {
        if let Some(k) = known.get(&r) {
            out.insert(r, k.clone());
            continue;
        }
        let (text, _) = fetch_text(base, &r).await?;
        let fp = read_fingerprint_file(&text).with_context(|| format!("fingerprint \"{r}\""))?;
        out.insert(r, fp);
    }
    Ok(out)
}

/// A file a club refers to (a fingerprint or song library): bundled with the
/// examples, from the web, or from disk; and where it was (for its own references).
async fn fetch_text(base: &str, r: &str) -> Result<(String, String)> {
    if base == EXAMPLE_BASE {
        return Ok((bundled_fingerprint(r)?, EXAMPLE_BASE.to_string()));
    }
    let at = resolve_ref(base, r);
    let text = if at.starts_with("https://") || at.starts_with("http://") {
        let resp = reqwest::get(&at).await.with_context(|| format!("fetching {at}"))?;
        if !resp.status().is_success() {
            bail!("{at}: {}", resp.status());
        }
        resp.text().await?
    } else {
        std::fs::read_to_string(&at).with_context(|| format!("reading {at}"))?
    };
    Ok((text, at))
}

/// Fetches the club's song libraries, with their songs' fingerprints written
/// in (references resolve against each library's own location).
pub async fn fetch_song_libraries(club: &Club, base: &str) -> Result<IndexMap<String, ClubSong>> {
    let mut out: IndexMap<String, ClubSong> = IndexMap::new();
    for loc in &club.song_library {
        let (text, at) = fetch_text(base, loc).await?;
        let lib = parse_song_library(&text).with_context(|| format!("song library \"{loc}\""))?;
        for (id, mut song) in lib.songs {
            if let Some(f) = song.fingerprint.clone().filter(|f| is_fingerprint_ref(f)) {
                let (t, _) = fetch_text(&at, &f).await?;
                song.fingerprint = Some(read_fingerprint_file(&t).with_context(|| format!("song {id}: fingerprint \"{f}\""))?);
            }
            if out.insert(id.clone(), song).is_some() {
                bail!("song \"{id}\" is in two song libraries");
            }
        }
    }
    let mut c = club.clone();
    c.library = out.clone();
    if let Some(id) = c.missing_songs().first() {
        bail!("no song \"{id}\" in the club's song libraries");
    }
    Ok(out)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct SetRules {
    /// Set length in minutes.
    #[serde(deserialize_with = "range_opt")]
    pub minutes: Option<Range>,
    /// Number of songs in the set.
    #[serde(deserialize_with = "range_opt")]
    pub songs: Option<Range>,
    pub pause: Pause,
    /// Fade-out length (seconds) when a song is cut short.
    pub fade_out: Option<f64>,
    /// How much a song may be sped up or slowed down (percent).
    pub tempo_change: Option<TempoChange>,
    /// Whether auto mode is the club's default.
    pub auto_advance: Option<bool>,
    /// At most this many songs of the same style in a row (any style).
    pub max_in_a_row: Option<u32>,
}

/// Pause between songs (seconds): `pause = 8` or `{ target = 8, min = 5, max = 10 }`.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Pause {
    pub target: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TempoChange {
    #[serde(default)]
    pub faster: f64,
    #[serde(default)]
    pub slower: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Style {
    /// Display name (default: from the id).
    pub name: String,
    /// Abbreviation for tight spaces.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Beats per bar: `meter = 3` or `meter = [2, 4]`; empty = any.
    #[serde(deserialize_with = "one_or_many")]
    pub meter: Vec<u8>,
    /// Beats per minute a song may be played at.
    #[serde(deserialize_with = "range_opt")]
    pub tempo: Option<Range>,
    /// The ideal part of `tempo`.
    #[serde(deserialize_with = "range_opt")]
    pub preferred: Option<Range>,
    /// Song length (seconds; `"3:30"` in the file).
    #[serde(deserialize_with = "duration_range_opt")]
    pub length: Option<Range>,
    /// How many songs of this style a set should have.
    #[serde(deserialize_with = "range_opt")]
    pub count: Option<Range>,
    /// Share of the set's songs (percent; `"50%"` in the file).
    #[serde(deserialize_with = "percent_range_opt")]
    pub share: Option<Range>,
    /// Where in the set (percent of the way through; `"75%"` in the file),
    /// one range or several: `when = ["0%", "50%"]`.
    #[serde(deserialize_with = "percent_ranges")]
    pub when: Vec<Range>,
    /// The set should start or end with this style.
    pub first: bool,
    pub last: bool,
    /// At most this many of this style in a row.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_in_a_row: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// `[[style.<id>.song]]`: the songs (often a particular edit) this style
    /// is danced to, e.g. a choreography's or a line dance's. A style with
    /// songs matches only those songs.
    #[serde(rename = "song", skip_serializing_if = "Vec::is_empty")]
    pub songs: Vec<ClubSong>,
    /// Songs from the club's song libraries (`song_library`), by id.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub suggested: Vec<String>,
    /// Those songs, resolved (for matching and the interface; not in the file).
    #[serde(skip_deserializing, skip_serializing_if = "Vec::is_empty")]
    pub library_songs: Vec<ClubSong>,
}

impl Style {
    /// Its own songs, then those it suggests from song libraries.
    pub fn all_songs(&self) -> impl Iterator<Item = &ClubSong> {
        self.songs.iter().chain(&self.library_songs)
    }
}

/// A song library (shared by clubs): `[song.<id>]` entries like a style's songs.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct SongLibrary {
    pub name: String,
    pub description: String,
    #[serde(rename = "song")]
    pub songs: IndexMap<String, ClubSong>,
}

pub fn parse_song_library(text: &str) -> Result<SongLibrary> {
    let lib: SongLibrary = toml::from_str(text).map_err(|e| anyhow!("{e}"))?;
    for (id, song) in &lib.songs {
        if !valid_id(id) {
            bail!("Song id \"{id}\": use lowercase letters, digits and dashes");
        }
        check_song(song, &format!("song {id}"))?;
    }
    Ok(lib)
}

/// A song a club accepts for a style. Any one of `fingerprint`, `spotify` and
/// `hash` identifies it; the rest is for people (and redundancy).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ClubSong {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    /// Length (seconds; `"2:35"` in the file).
    #[serde(deserialize_with = "duration_opt", skip_serializing_if = "Option::is_none")]
    pub length: Option<f64>,
    /// Chromaprint fingerprint (as Orpheus copies it, or `fpcalc` prints it),
    /// or where to find it: a file next to the club file or a web address.
    /// Recognises this version in any audio format, and tells edits apart.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    /// A Spotify track (`spotify:track:…`, a link, or its id).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spotify: Option<String>,
    /// Orpheus's hash of a particular audio file (exactly that file).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    /// Where to download the file (several links allowed; needs `hash`, which
    /// the download must match).
    #[serde(deserialize_with = "strings", skip_serializing_if = "Vec::is_empty")]
    pub download: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Its id in a song library (not in the file).
    #[serde(skip_deserializing, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

impl ClubSong {
    /// The Spotify track id, from a URI, a link or a bare id.
    pub fn spotify_id(&self) -> Option<String> {
        spotify_id(self.spotify.as_deref()?)
    }
}

/// Orpheus's file hashes (`decode::content_hash`): `a` or `f` and 32 hex digits.
pub fn valid_hash(h: &str) -> bool {
    let mut c = h.chars();
    matches!(c.next(), Some('a' | 'f')) && h.len() == 33 && c.all(|c| c.is_ascii_hexdigit())
}

/// `spotify:track:ID`, `https://open.spotify.com/track/ID?si=…` or `ID` → `ID`.
pub fn spotify_id(s: &str) -> Option<String> {
    let s = s.trim();
    let id = if let Some(rest) = s.strip_prefix("spotify:track:") {
        rest
    } else if let Some(i) = s.find("/track/") {
        s[i + 7..].split(['?', '/', '#']).next().unwrap_or("")
    } else {
        s
    };
    (id.len() == 22 && id.chars().all(|c| c.is_ascii_alphanumeric())).then(|| id.to_string())
}

/// Several styles counted together.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Group {
    pub name: String,
    /// Style ids.
    pub styles: Vec<String>,
    #[serde(deserialize_with = "range_opt")]
    pub count: Option<Range>,
    #[serde(deserialize_with = "percent_range_opt")]
    pub share: Option<Range>,
    #[serde(deserialize_with = "percent_ranges")]
    pub when: Vec<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_in_a_row: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// A rule for part of the set: which songs (by number, or how far through),
/// then what they must be: a length, and/or how many of some styles.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Rule {
    pub name: String,
    /// Song numbers (1 = first): `songs = [1, 10]`, `songs = { min = 11 }`.
    #[serde(deserialize_with = "range_opt")]
    pub songs: Option<Range>,
    /// Or how far through the set (percent).
    #[serde(deserialize_with = "percent_ranges")]
    pub when: Vec<Range>,
    /// Each of these songs' length.
    #[serde(deserialize_with = "duration_range_opt")]
    pub length: Option<Range>,
    /// The styles `count` counts (style ids; empty = all songs).
    pub styles: Vec<String>,
    #[serde(deserialize_with = "range_opt")]
    pub count: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// A club's song tag (e.g. "Snowball").
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Tag {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Shown on the projector.
    pub announce: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Inclusive range; either end may be open.
#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq)]
pub struct Range {
    pub min: Option<f64>,
    pub max: Option<f64>,
}

impl Range {
    fn check(&self, what: &str) -> Result<()> {
        if let (Some(a), Some(b)) = (self.min, self.max) {
            if a > b {
                bail!("{what}: {a} is more than {b}");
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- flexible value forms

/// A number, `[min, max]`, or `{ min, max }`.
#[derive(Deserialize)]
#[serde(untagged)]
enum RawRange<T> {
    One(T),
    Pair([T; 2]),
    Table { min: Option<T>, max: Option<T> },
}

/// A number or a string like `"3:30"` / `"75%"`.
#[derive(Deserialize)]
#[serde(untagged)]
enum NumOrText {
    Num(f64),
    Text(String),
}

fn raw_to_range<T>(r: RawRange<T>, conv: impl Fn(T) -> Result<f64, String>) -> Result<Range, String> {
    Ok(match r {
        RawRange::One(v) => {
            let v = conv(v)?;
            Range { min: Some(v), max: Some(v) }
        }
        RawRange::Pair([a, b]) => Range { min: Some(conv(a)?), max: Some(conv(b)?) },
        RawRange::Table { min, max } => Range { min: min.map(&conv).transpose()?, max: max.map(&conv).transpose()? },
    })
}

fn range_opt<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Range>, D::Error> {
    let r = RawRange::<f64>::deserialize(d)?;
    raw_to_range(r, Ok).map(Some).map_err(serde::de::Error::custom)
}

/// Seconds from `"m:ss"`, `"h:mm:ss"` or a number of seconds.
pub fn parse_duration(v: NumOrTextPub) -> Result<f64, String> {
    match v.0 {
        NumOrText::Num(n) => Ok(n),
        NumOrText::Text(s) => {
            let mut secs = 0.0;
            for part in s.trim().split(':') {
                let n: f64 = part.trim().parse().map_err(|_| format!("\"{s}\" isn't a length like \"3:30\""))?;
                secs = secs * 60.0 + n;
            }
            Ok(secs)
        }
    }
}

/// Wrapper so `parse_duration` can be shared by tests.
pub struct NumOrTextPub(NumOrText);

fn duration_range_opt<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Range>, D::Error> {
    let r = RawRange::<NumOrText>::deserialize(d)?;
    raw_to_range(r, |v| parse_duration(NumOrTextPub(v))).map(Some).map_err(serde::de::Error::custom)
}

/// Percent from `"75%"` or a number (already percent).
fn parse_percent(v: NumOrText) -> Result<f64, String> {
    match v {
        NumOrText::Num(n) => Ok(n),
        NumOrText::Text(s) => s
            .trim()
            .trim_end_matches('%')
            .trim()
            .parse()
            .map_err(|_| format!("\"{s}\" isn't a percentage like \"75%\"")),
    }
}

/// One range or a list of ranges of percentages.
fn percent_ranges<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Range>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        Many(Vec<RawRange<NumOrText>>),
        One(RawRange<NumOrText>),
    }
    let conv = |r| raw_to_range(r, parse_percent).map_err(serde::de::Error::custom);
    match OneOrMany::deserialize(d)? {
        // `["0%", "50%"]` is one range (a pair), not two single points.
        OneOrMany::Many(v) if v.len() == 2 && v.iter().all(|r| matches!(r, RawRange::One(_))) => {
            let mut it = v.into_iter();
            let (Some(RawRange::One(a)), Some(RawRange::One(b))) = (it.next(), it.next()) else { unreachable!() };
            Ok(vec![Range {
                min: Some(parse_percent(a).map_err(serde::de::Error::custom)?),
                max: Some(parse_percent(b).map_err(serde::de::Error::custom)?),
            }])
        }
        OneOrMany::Many(v) => v.into_iter().map(conv).collect(),
        OneOrMany::One(r) => Ok(vec![conv(r)?]),
    }
}

fn duration_opt<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    parse_duration(NumOrTextPub(NumOrText::deserialize(d)?)).map(Some).map_err(serde::de::Error::custom)
}

/// One string or a list of them.
fn strings<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum S {
        One(String),
        Many(Vec<String>),
    }
    Ok(match S::deserialize(d)? {
        S::One(v) => vec![v],
        S::Many(v) => v,
    })
}

fn percent_range_opt<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Range>, D::Error> {
    let r = RawRange::<NumOrText>::deserialize(d)?;
    raw_to_range(r, parse_percent).map(Some).map_err(serde::de::Error::custom)
}

fn one_or_many<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum M {
        One(u8),
        Many(Vec<u8>),
    }
    Ok(match M::deserialize(d)? {
        M::One(v) => vec![v],
        M::Many(v) => v,
    })
}

impl<'de> Deserialize<'de> for Pause {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum P {
            Secs(f64),
            Table {
                #[serde(default)]
                target: Option<f64>,
                #[serde(default)]
                min: Option<f64>,
                #[serde(default)]
                max: Option<f64>,
            },
        }
        Ok(match P::deserialize(d)? {
            P::Secs(s) => Pause { target: Some(s), min: None, max: None },
            P::Table { target, min, max } => Pause { target, min, max },
        })
    }
}

impl<'de> Deserialize<'de> for Range {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        raw_to_range(RawRange::<f64>::deserialize(d)?, Ok).map_err(serde::de::Error::custom)
    }
}

// ---------------------------------------------------------------- parse, check, write

/// Ids (style and tag keys, club and version ids): lowercase words and dashes.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// An id from a name ("Rotary Waltz" → "rotary-waltz").
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars().flat_map(|c| c.to_lowercase()) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() {
        "club".into()
    } else {
        out
    }
}

fn prettify(id: &str) -> String {
    id.split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Largest meter accepted (beats per bar).
const MAX_METER: u8 = 12;

/// Parses and checks a club file. Errors name the line (from the TOML
/// parser) or the style and field.
pub fn parse(text: &str) -> Result<Club> {
    let mut club: Club = toml::from_str(text).map_err(|e| anyhow!("{e}"))?;
    if club.name.trim().is_empty() {
        bail!("The club needs a name: name = \"…\"");
    }
    for (id, s) in club.styles.iter_mut() {
        if !valid_id(id) {
            bail!("Style id \"{id}\": use lowercase letters, digits and dashes (e.g. rotary-waltz)");
        }
        if s.name.trim().is_empty() {
            s.name = prettify(id);
        }
        for &m in &s.meter {
            if m == 0 || m > MAX_METER {
                bail!("{}: meter {m} isn't 1 to {MAX_METER} beats per bar", s.name);
            }
        }
        for (r, what) in [(&s.tempo, "tempo"), (&s.preferred, "preferred"), (&s.length, "length"), (&s.count, "count")] {
            if let Some(r) = r {
                r.check(&format!("{}: {what}", s.name))?;
            }
        }
        check_percents(&s.when, s.share.as_ref(), &s.name)?;
        for (i, song) in s.songs.iter().enumerate() {
            check_song(song, &format!("{}: song {} ({})", s.name, i + 1, song.title.as_deref().unwrap_or("untitled")))?;
        }
        for id in &s.suggested {
            if !valid_id(id) {
                bail!("{}: suggested song \"{id}\": use a song's id from the song library", s.name);
            }
        }
        if let Some(c) = &s.color {
            check_color(c).with_context(|| format!("{}: color", s.name))?;
        }
    }
    for (id, t) in club.tags.iter_mut() {
        if !valid_id(id) {
            bail!("Tag id \"{id}\": use lowercase letters, digits and dashes");
        }
        if t.name.trim().is_empty() {
            t.name = prettify(id);
        }
        if let Some(c) = &t.color {
            check_color(c).with_context(|| format!("tag {}: color", t.name))?;
        }
    }
    for (id, g) in club.groups.iter_mut() {
        if !valid_id(id) {
            bail!("Group id \"{id}\": use lowercase letters, digits and dashes");
        }
        if g.name.trim().is_empty() {
            g.name = prettify(id);
        }
        if g.styles.is_empty() {
            bail!("{}: list its styles: styles = [\"…\"]", g.name);
        }
        if let Some(r) = &g.count {
            r.check(&format!("{}: count", g.name))?;
        }
        check_percents(&g.when, g.share.as_ref(), &g.name)?;
    }
    for (id, r) in club.rules.iter_mut() {
        if !valid_id(id) {
            bail!("Rule id \"{id}\": use lowercase letters, digits and dashes");
        }
        if r.name.trim().is_empty() {
            r.name = prettify(id);
        }
        for (x, what) in [(&r.songs, "songs"), (&r.length, "length"), (&r.count, "count")] {
            if let Some(x) = x {
                x.check(&format!("{}: {what}", r.name))?;
            }
        }
        check_percents(&r.when, None, &r.name)?;
        if r.length.is_none() && r.count.is_none() {
            bail!("{}: say what the songs must be (length = … or count = …)", r.name);
        }
    }
    // References to styles must name styles of this club.
    let refs = club.groups.values().flat_map(|g| g.styles.iter().map(move |s| (&g.name, s)));
    let refs = refs.chain(club.rules.values().flat_map(|r| r.styles.iter().map(move |s| (&r.name, s))));
    for (owner, s) in refs {
        if !club.styles.contains_key(s) {
            bail!("{owner}: no style \"{s}\" (use a style's id, the name after [style.])");
        }
    }
    for (r, what) in [(&club.set.minutes, "set minutes"), (&club.set.songs, "set songs")] {
        if let Some(r) = r {
            r.check(what)?;
        }
    }
    Ok(club)
}

/// A song entry: identified, with well-formed identifiers.
fn check_song(song: &ClubSong, what: &str) -> Result<()> {
    if song.fingerprint.is_none() && song.spotify.is_none() && song.hash.is_none() {
        bail!("{what}: give a fingerprint, a Spotify track or a file hash to say which song it is");
    }
    if let Some(f) = song.fingerprint.as_deref().filter(|f| !is_fingerprint_ref(f)) {
        crate::fingerprint::decode(f).with_context(|| format!("{what}: fingerprint"))?;
    }
    if !song.download.is_empty() && song.hash.is_none() {
        bail!("{what}: a download needs the file's hash, so Orpheus can check it got the right file");
    }
    if let Some(h) = &song.hash {
        if !valid_hash(h) {
            bail!("{what}: \"{h}\" isn't an Orpheus file hash (copy it from the song's Info)");
        }
    }
    if let Some(sp) = &song.spotify {
        if spotify_id(sp).is_none() {
            bail!("{what}: \"{sp}\" isn't a Spotify track (spotify:track:…, or its link)");
        }
    }
    Ok(())
}

/// Positions and shares are percentages from 0 to 100.
fn check_percents(when: &[Range], share: Option<&Range>, owner: &str) -> Result<()> {
    for (r, what) in when.iter().map(|r| (r, "when")).chain(share.map(|r| (r, "share"))) {
        r.check(&format!("{owner}: {what}"))?;
        for v in [r.min, r.max].into_iter().flatten() {
            if !(0.0..=100.0).contains(&v) {
                bail!("{owner}: {what} {v}% isn't between 0% and 100%");
            }
        }
    }
    Ok(())
}

fn check_color(c: &str) -> Result<()> {
    let hex = c.strip_prefix('#').unwrap_or("");
    if !(hex.len() == 6 || hex.len() == 3) || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("\"{c}\" isn't a colour like \"#e5484d\"");
    }
    Ok(())
}

fn num(v: f64) -> toml::Value {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        toml::Value::Integer(v as i64)
    } else {
        toml::Value::Float(v)
    }
}

/// A range in its most compact form: `3`, `[3, 6]`, `{ min = 3 }`.
fn range_value(r: &Range, fmt: &dyn Fn(f64) -> toml::Value) -> Option<toml::Value> {
    Some(match (r.min, r.max) {
        (Some(a), Some(b)) if a == b => fmt(a),
        (Some(a), Some(b)) => toml::Value::Array(vec![fmt(a), fmt(b)]),
        (Some(a), None) => toml::Value::Table([("min".to_string(), fmt(a))].into_iter().collect()),
        (None, Some(b)) => toml::Value::Table([("max".to_string(), fmt(b))].into_iter().collect()),
        (None, None) => return None,
    })
}

fn duration_text(secs: f64) -> toml::Value {
    let s = secs.round() as i64;
    toml::Value::String(format!("{}:{:02}", s / 60, s % 60))
}

fn percent_text(p: f64) -> toml::Value {
    toml::Value::String(format!("{}%", (p * 10.0).round() / 10.0))
}

/// Writes a club as a tidy TOML file (the form the editor saves).
pub fn to_toml(club: &Club) -> String {
    use toml::{Table, Value};
    let mut root = Table::new();
    root.insert("name".into(), Value::String(club.name.clone()));
    if !club.description.is_empty() {
        root.insert("description".into(), Value::String(club.description.clone()));
    }
    if let Some(u) = &club.website {
        root.insert("website".into(), Value::String(u.clone()));
    }
    if let Some(u) = &club.url {
        root.insert("url".into(), Value::String(u.clone()));
    }
    match club.song_library.as_slice() {
        [] => {}
        [one] => {
            root.insert("song_library".into(), Value::String(one.clone()));
        }
        many => {
            root.insert("song_library".into(), Value::Array(many.iter().cloned().map(Value::String).collect()));
        }
    }
    if !club.announcements.is_empty() {
        root.insert("announcements".into(), Value::Array(club.announcements.iter().cloned().map(Value::String).collect()));
    }
    let s = &club.set;
    let mut set = Table::new();
    if let Some(v) = s.minutes.as_ref().and_then(|r| range_value(r, &num)) {
        set.insert("minutes".into(), v);
    }
    if let Some(v) = s.songs.as_ref().and_then(|r| range_value(r, &num)) {
        set.insert("songs".into(), v);
    }
    match (s.pause.target, s.pause.min, s.pause.max) {
        (None, None, None) => {}
        (Some(t), None, None) => {
            set.insert("pause".into(), num(t));
        }
        (t, a, b) => {
            let mut p = Table::new();
            for (k, v) in [("target", t), ("min", a), ("max", b)] {
                if let Some(v) = v {
                    p.insert(k.into(), num(v));
                }
            }
            set.insert("pause".into(), Value::Table(p));
        }
    }
    if let Some(f) = s.fade_out {
        set.insert("fade_out".into(), num(f));
    }
    if let Some(t) = s.tempo_change {
        set.insert(
            "tempo_change".into(),
            Value::Table([("faster".to_string(), num(t.faster)), ("slower".to_string(), num(t.slower))].into_iter().collect()),
        );
    }
    if let Some(a) = s.auto_advance {
        set.insert("auto_advance".into(), Value::Boolean(a));
    }
    if let Some(n) = s.max_in_a_row {
        set.insert("max_in_a_row".into(), Value::Integer(n as i64));
    }
    if !set.is_empty() {
        root.insert("set".into(), Value::Table(set));
    }
    // Helpers for the per-id tables.
    let put_range = |t: &mut Table, k: &str, r: &Option<Range>, f: &dyn Fn(f64) -> Value| {
        if let Some(v) = r.as_ref().and_then(|r| range_value(r, f)) {
            t.insert(k.into(), v);
        }
    };
    let put_when = |t: &mut Table, when: &[Range]| match when {
        [] => {}
        [r] => {
            if let Some(v) = range_value(r, &percent_text) {
                t.insert("when".into(), v);
            }
        }
        rs => {
            t.insert("when".into(), Value::Array(rs.iter().filter_map(|r| range_value(r, &percent_text)).collect()));
        }
    };
    let put_text = |t: &mut Table, k: &str, v: &Option<String>| {
        if let Some(v) = v {
            t.insert(k.into(), Value::String(v.clone()));
        }
    };
    let ids = |v: &[String]| Value::Array(v.iter().cloned().map(Value::String).collect());

    let mut styles = Table::new();
    for (id, st) in &club.styles {
        let mut t = Table::new();
        t.insert("name".into(), Value::String(st.name.clone()));
        put_text(&mut t, "short", &st.short);
        put_text(&mut t, "color", &st.color);
        match st.meter.as_slice() {
            [] => {}
            [m] => {
                t.insert("meter".into(), Value::Integer(*m as i64));
            }
            ms => {
                t.insert("meter".into(), Value::Array(ms.iter().map(|m| Value::Integer(*m as i64)).collect()));
            }
        }
        put_range(&mut t, "tempo", &st.tempo, &num);
        put_range(&mut t, "preferred", &st.preferred, &num);
        put_range(&mut t, "length", &st.length, &duration_text);
        put_range(&mut t, "count", &st.count, &num);
        put_range(&mut t, "share", &st.share, &percent_text);
        put_when(&mut t, &st.when);
        if st.first {
            t.insert("first".into(), Value::Boolean(true));
        }
        if st.last {
            t.insert("last".into(), Value::Boolean(true));
        }
        if let Some(n) = st.max_in_a_row {
            t.insert("max_in_a_row".into(), Value::Integer(n as i64));
        }
        put_text(&mut t, "note", &st.note);
        if !st.suggested.is_empty() {
            t.insert("suggested".into(), ids(&st.suggested));
        }
        if !st.songs.is_empty() {
            let songs = st.songs.iter().map(|song| Value::Table(song_table(song))).collect();
            t.insert("song".into(), Value::Array(songs));
        }
        styles.insert(id.clone(), Value::Table(t));
    }
    let mut groups = Table::new();
    for (id, g) in &club.groups {
        let mut t = Table::new();
        t.insert("name".into(), Value::String(g.name.clone()));
        t.insert("styles".into(), ids(&g.styles));
        put_range(&mut t, "count", &g.count, &num);
        put_range(&mut t, "share", &g.share, &percent_text);
        put_when(&mut t, &g.when);
        if let Some(n) = g.max_in_a_row {
            t.insert("max_in_a_row".into(), Value::Integer(n as i64));
        }
        put_text(&mut t, "note", &g.note);
        groups.insert(id.clone(), Value::Table(t));
    }
    let mut rules = Table::new();
    for (id, r) in &club.rules {
        let mut t = Table::new();
        t.insert("name".into(), Value::String(r.name.clone()));
        put_range(&mut t, "songs", &r.songs, &num);
        put_when(&mut t, &r.when);
        put_range(&mut t, "length", &r.length, &duration_text);
        if !r.styles.is_empty() {
            t.insert("styles".into(), ids(&r.styles));
        }
        put_range(&mut t, "count", &r.count, &num);
        put_text(&mut t, "note", &r.note);
        rules.insert(id.clone(), Value::Table(t));
    }
    let mut tags = Table::new();
    for (id, tg) in &club.tags {
        let mut t = Table::new();
        t.insert("name".into(), Value::String(tg.name.clone()));
        put_text(&mut t, "color", &tg.color);
        if tg.announce {
            t.insert("announce".into(), Value::Boolean(true));
        }
        put_text(&mut t, "note", &tg.note);
        tags.insert(id.clone(), Value::Table(t));
    }
    for (k, t) in [("style", styles), ("group", groups), ("rule", rules), ("tag", tags)] {
        if !t.is_empty() {
            root.insert(k.into(), Value::Table(t));
        }
    }
    tidy(&toml::to_string(&root).unwrap_or_default())
}

/// A song entry's fields, in the order people read them (the long
/// fingerprint last).
fn song_table(song: &ClubSong) -> toml::Table {
    use toml::Value;
    let mut x = toml::Table::new();
    let mut text = |k: &str, v: &Option<String>| {
        if let Some(v) = v {
            x.insert(k.into(), Value::String(v.clone()));
        }
    };
    text("title", &song.title);
    text("artist", &song.artist);
    if let Some(l) = song.length {
        x.insert("length".into(), duration_text(l));
    }
    let mut text = |k: &str, v: &Option<String>| {
        if let Some(v) = v {
            x.insert(k.into(), Value::String(v.clone()));
        }
    };
    text("spotify", &song.spotify);
    text("hash", &song.hash);
    text("note", &song.note);
    if !song.download.is_empty() {
        x.insert("download".into(), Value::Array(song.download.iter().cloned().map(Value::String).collect()));
    }
    if let Some(f) = &song.fingerprint {
        x.insert("fingerprint".into(), Value::String(f.clone()));
    }
    x
}

/// One song entry as club-file text, to paste under a style.
pub fn song_toml(song: &ClubSong) -> String {
    let mut style = toml::Table::new();
    style.insert("song".into(), toml::Value::Array(vec![toml::Value::Table(song_table(song))]));
    let mut styles = toml::Table::new();
    styles.insert("your-style-id".into(), toml::Value::Table(style));
    let mut root = toml::Table::new();
    root.insert("style".into(), toml::Value::Table(styles));
    tidy(&toml::to_string(&root).unwrap_or_default())
}

/// Hand-written style: small tables (`pause`, `count = { min = 3 }`) inline,
/// arrays on one line, and each style and tag in its own section.
fn tidy(text: &str) -> String {
    use toml_edit::{DocumentMut, Item};
    let Ok(mut doc) = text.parse::<DocumentMut>() else { return text.to_string() };
    fn inline_children(t: &mut toml_edit::Table) {
        for (_, item) in t.iter_mut() {
            if let Item::Table(sub) = item {
                let inl = std::mem::take(sub).into_inline_table();
                *item = Item::Value(toml_edit::Value::InlineTable(inl));
            }
        }
    }
    if let Some(Item::Table(set)) = doc.get_mut("set") {
        inline_children(set);
    }
    for group in ["style", "group", "rule", "tag"] {
        if let Some(Item::Table(t)) = doc.get_mut(group) {
            t.set_implicit(true);
            for (_, item) in t.iter_mut() {
                if let Item::Table(entry) = item {
                    inline_children(entry);
                }
            }
        }
    }
    doc.to_string()
}

// ---------------------------------------------------------------- versions on disk

/// One stored version of a club's guidelines.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VersionMeta {
    /// Name the DJ gave it ("" = by its import date).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// When it was imported or saved (epoch ms).
    pub imported: i64,
    /// Where it came from: a URL, a file path, or "edited".
    #[serde(skip_serializing_if = "String::is_empty")]
    pub from: String,
    /// What fingerprint references resolve against: the club file's web
    /// address or path (edited versions keep their original's).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub base: String,
}

/// Fingerprints fetched for a version (`<version>.fingerprints.toml`).
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct FingerprintsFile {
    fingerprints: IndexMap<String, String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct VersionsFile {
    /// By version id, oldest first.
    versions: IndexMap<String, VersionMeta>,
}

const DAY_FORMAT: &str = "%b %-d, %Y";
const DAY_TIME_FORMAT: &str = "%b %-d, %Y, %-I:%M %p";

/// An epoch-ms time in this computer's time zone.
fn local_time(ms: i64, format: &str) -> String {
    jiff::Timestamp::from_millisecond(ms).map(|t| t.to_zoned(jiff::tz::TimeZone::system()).strftime(format).to_string()).unwrap_or_default()
}

/// A club and its versions, for lists.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClubInfo {
    pub id: String,
    /// The latest version's name for the club.
    pub name: String,
    pub description: String,
    pub url: Option<String>,
    /// Newest first.
    pub versions: Vec<VersionInfo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    pub id: String,
    pub name: String,
    /// How it's called in lists: its name, else when it was imported.
    pub label: String,
    pub imported: i64,
    pub from: String,
    /// The file couldn't be read; the error.
    pub error: Option<String>,
}

/// One version: the file's text, and its parsed definition.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClubVersion {
    pub club_id: String,
    pub version_id: String,
    pub text: String,
    pub club: Option<Club>,
    pub error: Option<String>,
}

/// The clubs folder of a library.
pub struct Clubs {
    dir: PathBuf,
}

/// How an import went.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Imported {
    pub club_id: String,
    pub version_id: String,
    /// The file was identical to the latest version: nothing new was stored.
    pub unchanged: bool,
}

impl Clubs {
    pub fn new(library: &Path) -> Self {
        Self { dir: library.join("clubs") }
    }

    fn club_dir(&self, id: &str) -> Result<PathBuf> {
        if !valid_id(id) {
            bail!("invalid club id \"{id}\"");
        }
        Ok(self.dir.join(id))
    }

    fn versions(&self, id: &str) -> Result<VersionsFile> {
        let p = self.club_dir(id)?.join("versions.toml");
        match std::fs::read_to_string(&p) {
            Ok(t) => toml::from_str(&t).with_context(|| format!("reading {}", p.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(VersionsFile::default()),
            Err(e) => Err(e.into()),
        }
    }

    fn save_versions(&self, id: &str, v: &VersionsFile) -> Result<()> {
        let dir = self.club_dir(id)?;
        std::fs::create_dir_all(&dir)?;
        crate::store::write_atomic(&dir.join("versions.toml"), toml::to_string_pretty(v)?.as_bytes())
    }

    pub fn list(&self) -> Vec<ClubInfo> {
        let mut out = vec![];
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return out };
        let mut ids: Vec<String> = rd
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().to_str().map(String::from))
            .filter(|id| valid_id(id))
            .collect();
        ids.sort();
        for id in ids {
            let Ok(vf) = self.versions(&id) else { continue };
            let mut versions = vec![];
            let mut latest: Option<Club> = None;
            for (vid, meta) in vf.versions.iter().rev() {
                let parsed = self.read_text(&id, vid).and_then(|t| parse(&t));
                let error = parsed.as_ref().err().map(|e| format!("{e:#}"));
                if latest.is_none() {
                    latest = parsed.ok();
                }
                versions.push(VersionInfo { id: vid.clone(), name: meta.name.clone(), label: String::new(), imported: meta.imported, from: meta.from.clone(), error });
            }
            if versions.is_empty() {
                continue;
            }
            // Unnamed versions go by their date (and time, when several share a day).
            let days: Vec<String> = versions.iter().map(|v| local_time(v.imported, DAY_FORMAT)).collect();
            for (i, v) in versions.iter_mut().enumerate() {
                v.label = if !v.name.is_empty() {
                    v.name.clone()
                } else if days.iter().enumerate().any(|(j, d)| j != i && *d == days[i]) {
                    local_time(v.imported, DAY_TIME_FORMAT)
                } else {
                    days[i].clone()
                };
            }
            let l = latest.unwrap_or_default();
            out.push(ClubInfo { name: if l.name.is_empty() { prettify(&id) } else { l.name }, description: l.description, url: l.url, id, versions });
        }
        out
    }

    fn read_text(&self, id: &str, version: &str) -> Result<String> {
        if !valid_id(version) {
            bail!("invalid version id \"{version}\"");
        }
        let p = self.club_dir(id)?.join(format!("{version}.toml"));
        std::fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))
    }

    /// The newest version's id.
    pub fn latest(&self, id: &str) -> Option<String> {
        self.versions(id).ok()?.versions.keys().last().cloned()
    }

    /// The fingerprints fetched for a version.
    pub fn fingerprints(&self, id: &str, version: &str) -> HashMap<String, String> {
        let Ok(dir) = self.club_dir(id) else { return HashMap::new() };
        std::fs::read_to_string(dir.join(format!("{version}.fingerprints.toml")))
            .ok()
            .and_then(|t| toml::from_str::<FingerprintsFile>(&t).ok())
            .map(|f| f.fingerprints.into_iter().collect())
            .unwrap_or_default()
    }

    fn save_fingerprints(&self, id: &str, version: &str, fps: &HashMap<String, String>) -> Result<()> {
        let p = self.club_dir(id)?.join(format!("{version}.fingerprints.toml"));
        if fps.is_empty() {
            let _ = std::fs::remove_file(p);
            return Ok(());
        }
        let mut sorted: Vec<_> = fps.iter().collect();
        sorted.sort();
        let file = FingerprintsFile { fingerprints: sorted.into_iter().map(|(k, v)| (k.clone(), v.clone())).collect() };
        crate::store::write_atomic(&p, toml::to_string_pretty(&file)?.as_bytes())
    }

    /// What a version's fingerprint references resolve against.
    pub fn base(&self, id: &str, version: &str) -> String {
        self.versions(id).ok().and_then(|v| v.versions.get(version).map(|m| m.base.clone())).unwrap_or_default()
    }

    fn parse_version(&self, id: &str, version: &str, text: &str) -> Result<Club> {
        let mut c = parse(text)?;
        c.fingerprints = self.fingerprints(id, version);
        c.library = self.song_library(id, version);
        c.attach_library();
        Ok(c)
    }

    /// The song libraries fetched for a version (their songs, by id).
    pub fn song_library(&self, id: &str, version: &str) -> IndexMap<String, ClubSong> {
        let Ok(dir) = self.club_dir(id) else { return IndexMap::new() };
        std::fs::read_to_string(dir.join(format!("{version}.songs.toml")))
            .ok()
            .and_then(|t| parse_song_library(&t).ok())
            .map(|l| l.songs)
            .unwrap_or_default()
    }

    fn save_song_library(&self, id: &str, version: &str, songs: &IndexMap<String, ClubSong>) -> Result<()> {
        let p = self.club_dir(id)?.join(format!("{version}.songs.toml"));
        if songs.is_empty() {
            let _ = std::fs::remove_file(p);
            return Ok(());
        }
        let lib = SongLibrary { name: String::new(), description: String::new(), songs: songs.clone() };
        crate::store::write_atomic(&p, toml::to_string_pretty(&lib)?.as_bytes())
    }

    pub fn get(&self, id: &str, version: &str) -> Result<ClubVersion> {
        let text = self.read_text(id, version)?;
        let (club, error) = match self.parse_version(id, version, &text) {
            Ok(c) => (Some(c), None),
            Err(e) => (None, Some(format!("{e:#}"))),
        };
        Ok(ClubVersion { club_id: id.into(), version_id: version.into(), text, club, error })
    }

    /// Parsed definition of a version (`None` = latest).
    pub fn club(&self, id: &str, version: Option<&str>) -> Result<Club> {
        let v = match version {
            Some(v) => v.to_string(),
            None => self.latest(id).ok_or_else(|| anyhow!("no club \"{id}\""))?,
        };
        self.parse_version(id, &v, &self.read_text(id, &v)?)
    }

    /// A new version id from the time (unique within the club).
    fn new_version_id(&self, club: &str, now_ms: i64) -> Result<String> {
        let t = jiff::Timestamp::from_millisecond(now_ms).map_err(|e| anyhow!("{e}"))?;
        let base = t.strftime("%Y-%m-%d-%H%M").to_string();
        let vf = self.versions(club)?;
        let mut id = base.clone();
        let mut n = 2;
        while vf.versions.contains_key(&id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        Ok(id)
    }

    /// Stores `text` as a new version (of the club its id or name names, or
    /// `into` if given), with the fingerprints it refers to (fetched from
    /// `base`). Identical to the latest version: nothing is stored.
    pub fn add(
        &self,
        text: &str,
        from: &str,
        base: &str,
        into: Option<&str>,
        fps: &HashMap<String, String>,
        songs: &IndexMap<String, ClubSong>,
        now_ms: i64,
    ) -> Result<Imported> {
        let club = parse(text)?;
        let id = match into {
            Some(i) => i.to_string(),
            None => slug(&club.name),
        };
        let dir = self.club_dir(&id)?;
        if let Some(latest) = self.latest(&id) {
            if self.read_text(&id, &latest).ok().as_deref() == Some(text) {
                return Ok(Imported { club_id: id, version_id: latest, unchanged: true });
            }
        }
        std::fs::create_dir_all(&dir)?;
        let vid = self.new_version_id(&id, now_ms)?;
        crate::store::write_atomic(&dir.join(format!("{vid}.toml")), text.as_bytes())?;
        self.save_fingerprints(&id, &vid, fps)?;
        self.save_song_library(&id, &vid, songs)?;
        let mut vf = self.versions(&id)?;
        vf.versions.insert(vid.clone(), VersionMeta { name: String::new(), imported: now_ms, from: from.into(), base: base.into() });
        self.save_versions(&id, &vf)?;
        Ok(Imported { club_id: id, version_id: vid, unchanged: false })
    }

    /// Saves edits to `version`: in place if `in_place` (a version no set
    /// uses yet), else as a new version.
    #[allow(clippy::too_many_arguments)]
    pub fn save(
        &self,
        id: &str,
        version: &str,
        text: &str,
        in_place: bool,
        fps: &HashMap<String, String>,
        songs: &IndexMap<String, ClubSong>,
        now_ms: i64,
    ) -> Result<Imported> {
        parse(text)?;
        if in_place {
            let dir = self.club_dir(id)?;
            crate::store::write_atomic(&dir.join(format!("{version}.toml")), text.as_bytes())?;
            self.save_fingerprints(id, version, fps)?;
            self.save_song_library(id, version, songs)?;
            return Ok(Imported { club_id: id.into(), version_id: version.into(), unchanged: false });
        }
        let base = self.base(id, version);
        self.add(text, "edited", &base, Some(id), fps, songs, now_ms)
    }

    pub fn rename_version(&self, id: &str, version: &str, name: &str) -> Result<()> {
        let mut vf = self.versions(id)?;
        let m = vf.versions.get_mut(version).ok_or_else(|| anyhow!("no version \"{version}\""))?;
        m.name = name.trim().to_string();
        self.save_versions(id, &vf)
    }

    /// Removes a version (the club, with its last version).
    pub fn delete_version(&self, id: &str, version: &str) -> Result<()> {
        let mut vf = self.versions(id)?;
        vf.versions.shift_remove(version);
        let dir = self.club_dir(id)?;
        for suffix in ["toml", "fingerprints.toml", "songs.toml"] {
            let _ = std::fs::remove_file(dir.join(format!("{version}.{suffix}")));
        }
        if vf.versions.is_empty() {
            return self.delete(id);
        }
        self.save_versions(id, &vf)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let dir = self.club_dir(id)?;
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        Ok(())
    }

    /// Where updates for a club come from: its file's `url`, else the URL it
    /// was imported from.
    pub fn update_url(&self, id: &str) -> Option<String> {
        if let Some(u) = self.club(id, None).ok().and_then(|c| c.url) {
            return Some(u);
        }
        let vf = self.versions(id).ok()?;
        vf.versions.values().rev().map(|m| m.from.clone()).find(|f| f.starts_with("http://") || f.starts_with("https://"))
    }
}

/// `base` of the bundled example clubs: their fingerprints are built in.
pub const EXAMPLE_BASE: &str = "example";

/// Files the example clubs refer to: their song library and its fingerprints
/// (`examples/clubs/`).
pub const EXAMPLE_FINGERPRINTS: &[(&str, &str)] = &include!("../../examples/clubs/fingerprints/list.rs");

/// The example clubs shipped with Orpheus (`examples/clubs/`): (file name, text).
pub const EXAMPLES: &[(&str, &str)] = &[
    ("dancebreak", include_str!("../../examples/clubs/dancebreak.toml")),
    ("jammix", include_str!("../../examples/clubs/jammix.toml")),
    ("friday-night-waltz", include_str!("../../examples/clubs/friday-night-waltz.toml")),
];

/// Fetches a club file from the web.
pub async fn fetch(url: &str) -> Result<String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        bail!("Enter a web address starting with https://");
    }
    let resp = reqwest::get(url).await.with_context(|| format!("fetching {url}"))?;
    if !resp.status().is_success() {
        bail!("{url}: {}", resp.status());
    }
    let text = resp.text().await?;
    // Fail now, with the parse error, rather than storing a web page.
    parse(&text).with_context(|| format!("{url} isn't a club file"))?;
    Ok(text)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_club_exports_as_one_self_contained_file() {
        let text = r#"
name = "Club"
song_library = "songs.toml"

[style.barbie]
name = "Barbie Line Dance"
suggested = ["barbie-rp"]

[[style.barbie.song]]
title = "Own song"
fingerprint = "fingerprints/own.txt"
"#;
        let mut club = parse(text).unwrap();
        club.library.insert("barbie-rp".into(), ClubSong { title: Some("Barbie (RP edit)".into()), hash: Some("a0123456789abcdef0123456789abcdef".into()), ..Default::default() });
        let fp = crate::fingerprint::encode(&(0..200u32).map(|i| i.wrapping_mul(2654435761)).collect::<Vec<_>>());
        club.fingerprints.insert("fingerprints/own.txt".into(), fp.clone());
        club.attach_library();
        let out = to_toml(&self_contained(&club));
        assert!(!out.contains("song_library") && !out.contains("suggested") && !out.contains("fingerprints/"), "{out}");
        let back = parse(&out).unwrap();
        let songs = &back.styles["barbie"].songs;
        assert_eq!(songs.iter().map(|s| s.title.clone().unwrap()).collect::<Vec<_>>(), ["Own song", "Barbie (RP edit)"]);
        assert_eq!(songs[0].fingerprint.as_deref(), Some(fp.as_str()));
    }

    use super::*;

    const SAMPLE: &str = r##"
name = "Test Club"
description = "For tests"
website = "https://example.org/djing"
url = "https://example.org/club.toml"
announcements = ["No street shoes."]

[set]
minutes = 120
songs = [28, 34]
pause = { target = 8, min = 5, max = 10 }
fade_out = 5
tempo_change = { faster = 10, slower = 8 }
max_in_a_row = 2

[style.rotary]
name = "Rotary Waltz"
short = "RW"
color = "#e5484d"
meter = 3
tempo = [135, 160]
preferred = [140, 155]
length = ["2:00", "4:30"]
count = { min = 3 }
when = [["0%", "40%"], ["90%", "100%"]]
last = true

[style.polka]
meter = [2, 4]
tempo = 120
when = ["25%", "75%"]

[[style.polka.song]]
title = "A Polka"
length = "2:30"
spotify = "https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC?si=x"
hash = "a1798743518a3a1c753c8dbe831f48f39"
download = ["https://example.org/a.mp3", "https://mirror.example.org/a.mp3"]

[style.salsa]
meter = 4
tempo = [86, 100]
share = { max = "10%" }

[group.latin]
name = "Latin"
styles = ["salsa", "polka"]
count = { min = 1 }

[rule.short-start]
name = "Short songs first"
songs = [1, 10]
length = { max = "3:30" }

[tag.snowball]
name = "Snowball"
announce = true
"##;

    #[test]
    fn parses_all_forms() {
        let c = parse(SAMPLE).unwrap();
        assert_eq!(c.name, "Test Club");
        assert_eq!(c.set.songs, Some(Range { min: Some(28.0), max: Some(34.0) }));
        assert_eq!(c.set.pause.target, Some(8.0));
        assert_eq!(c.styles.keys().collect::<Vec<_>>(), ["rotary", "polka", "salsa"], "file order kept");
        let rw = &c.styles["rotary"];
        assert_eq!(rw.meter, vec![3]);
        assert_eq!(rw.length, Some(Range { min: Some(120.0), max: Some(270.0) }));
        assert_eq!(rw.count, Some(Range { min: Some(3.0), max: None }));
        assert_eq!(rw.when.len(), 2);
        assert_eq!(rw.when[1], Range { min: Some(90.0), max: Some(100.0) });
        let pk = &c.styles["polka"];
        assert_eq!(pk.name, "Polka", "name from the id");
        assert_eq!(pk.meter, vec![2, 4]);
        assert_eq!(pk.tempo, Some(Range { min: Some(120.0), max: Some(120.0) }));
        assert_eq!(pk.when, vec![Range { min: Some(25.0), max: Some(75.0) }], "a pair is one range");
        assert!(c.tags["snowball"].announce);
        assert_eq!(c.set.max_in_a_row, Some(2));
        let song = &c.styles["polka"].songs[0];
        assert_eq!(song.length, Some(150.0));
        assert_eq!(song.spotify_id().as_deref(), Some("4uLU6hMCjMI75M1A2tKUQC"));
        assert_eq!(song.download.len(), 2);
        assert_eq!(c.styles["salsa"].share, Some(Range { min: None, max: Some(10.0) }));
        assert_eq!(c.groups["latin"].styles, ["salsa", "polka"]);
        let r = &c.rules["short-start"];
        assert_eq!(r.songs, Some(Range { min: Some(1.0), max: Some(10.0) }));
        assert_eq!(r.length, Some(Range { min: None, max: Some(210.0) }));
    }

    #[test]
    fn references_must_exist() {
        let e = parse("name = \"X\"\n[style.a]\n[group.g]\nstyles = [\"b\"]\ncount = 1\n").unwrap_err().to_string();
        assert!(e.contains("no style \"b\""), "{e}");
        let e = parse("name = \"X\"\n[rule.r]\nsongs = [1, 5]\n").unwrap_err().to_string();
        assert!(e.contains("say what the songs must be"), "{e}");
        let e = parse("name = \"X\"\n[style.a]\n[[style.a.song]]\ntitle = \"T\"\n").unwrap_err().to_string();
        assert!(e.contains("fingerprint, a Spotify track or a file hash"), "{e}");
        let e = parse("name = \"X\"\n[style.a]\n[[style.a.song]]\nfingerprint = \"nope\"\n").unwrap_err().to_string();
        assert!(e.contains("fingerprint"), "{e}");
        let e = parse("name = \"X\"\n[style.a]\n[[style.a.song]]\nspotify = \"4uLU6hMCjMI75M1A2tKUQC\"\ndownload = \"https://x.org/a.mp3\"\n").unwrap_err().to_string();
        assert!(e.contains("needs the file's hash"), "{e}");
    }

    #[test]
    fn round_trips_through_the_editor_form() {
        let c = parse(SAMPLE).unwrap();
        let text = to_toml(&c);
        assert_eq!(parse(&text).unwrap(), c, "{text}");
        assert!(text.contains("length = [\"2:00\", \"4:30\"]"), "{text}");
    }

    #[test]
    fn errors_are_specific() {
        let e = parse("name = \"X\"\n[style.a]\ntempo = [160, 120]\n").unwrap_err().to_string();
        assert!(e.contains("160 is more than 120"), "{e}");
        let e = parse("name = \"X\"\n[style.a]\nmeter = 0\n").unwrap_err().to_string();
        assert!(e.contains("meter 0"), "{e}");
        let e = parse("name = \"X\"\n[style.a]\ntempo = [1,\n").unwrap_err().to_string();
        assert!(e.contains("line"), "parse errors name the line: {e}");
        let e = parse("name = \"X\"\n[style.a]\ntempoo = 3\n").unwrap_err().to_string();
        assert!(e.contains("tempoo"), "unknown fields are named: {e}");
        assert!(parse("description = \"no name\"").is_err());
    }

    #[test]
    fn versions_are_kept() {
        let dir = std::env::temp_dir().join(format!("orpheus-clubs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let clubs = Clubs::new(&dir);
        let a = clubs.add(SAMPLE, "file", "", None, &HashMap::new(), &IndexMap::new(), 1_000_000_000_000).unwrap();
        assert_eq!(a.club_id, "test-club");
        assert!(!a.unchanged);
        let same = clubs.add(SAMPLE, "file", "", None, &HashMap::new(), &IndexMap::new(), 1_000_000_060_000).unwrap();
        assert!(same.unchanged, "identical file: no new version");
        let b = clubs.add(&SAMPLE.replace("120\nsongs", "90\nsongs"), "file", "", None, &HashMap::new(), &IndexMap::new(), 1_000_000_060_000).unwrap();
        assert_ne!(a.version_id, b.version_id);
        let list = clubs.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].versions.len(), 2);
        assert_eq!(list[0].versions[0].id, b.version_id, "newest first");
        assert_eq!(clubs.club("test-club", Some(&a.version_id)).unwrap().set.minutes.unwrap().min, Some(120.0));
        assert_eq!(clubs.club("test-club", None).unwrap().set.minutes.unwrap().min, Some(90.0));
        clubs.rename_version("test-club", &a.version_id, "Spring 2026").unwrap();
        assert_eq!(clubs.list()[0].versions[1].name, "Spring 2026");
        clubs.delete_version("test-club", &b.version_id).unwrap();
        assert_eq!(clubs.latest("test-club").as_deref(), Some(a.version_id.as_str()));
        clubs.delete_version("test-club", &a.version_id).unwrap();
        assert!(clubs.list().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The example clubs (examples/clubs/*.toml) parse and survive the editor.
    #[test]
    fn example_clubs_parse() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/clubs");
        let mut n = 0;
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("toml") || p.file_name().is_some_and(|n| n == "songs.toml") {
                continue;
            }
            let text = std::fs::read_to_string(&p).unwrap();
            let c = parse(&text).unwrap_or_else(|e| panic!("{}: {e:#}", p.display()));
            assert!(!c.styles.is_empty(), "{}", p.display());
            // Every fingerprint it refers to is bundled, and reads.
            for r in c.fingerprint_refs() {
                read_fingerprint_file(&bundled_fingerprint(&r).unwrap()).unwrap_or_else(|e| panic!("{r}: {e:#}"));
            }
            // Its song libraries are bundled and have every song it suggests.
            let songs = futures_lite_block_on(fetch_song_libraries(&c, EXAMPLE_BASE)).unwrap_or_else(|e| panic!("{}: {e:#}", p.display()));
            for st in c.styles.values() {
                for id in &st.suggested {
                    assert!(songs[id].fingerprint.as_deref().is_none_or(|f| !is_fingerprint_ref(f)), "{id}: fingerprint read");
                }
            }
            assert_eq!(parse(&to_toml(&c)).unwrap(), c, "{}", p.display());
            n += 1;
        }
        assert_eq!(n, EXAMPLES.len(), "every example club is bundled");
    }

    #[test]
    fn fingerprint_references() {
        assert!(!is_fingerprint_ref("AQADtEmUSEk-_x"));
        assert!(is_fingerprint_ref("fingerprints/bnp.txt"));
        assert!(is_fingerprint_ref("https://club.org/fp/bnp.txt"));
        // Relative to where the club file came from.
        assert_eq!(resolve_ref("https://club.org/dj/club.toml", "fp/bnp.txt"), "https://club.org/dj/fp/bnp.txt");
        assert_eq!(resolve_ref("/home/dj/clubs/club.toml", "fp/bnp.txt"), "/home/dj/clubs/fp/bnp.txt");
        assert_eq!(resolve_ref("/home/dj/club.toml", "https://x.org/a.txt"), "https://x.org/a.txt");
        // A plain fingerprint, or fpcalc's output.
        let (_, fp) = EXAMPLE_FINGERPRINTS.iter().find(|(n, _)| n.starts_with("fingerprints/")).unwrap();
        let fp = fp.trim();
        assert_eq!(read_fingerprint_file(&format!("{fp}\n")).unwrap(), fp);
        assert_eq!(read_fingerprint_file(&format!("DURATION=155\nFINGERPRINT={fp}\n")).unwrap(), fp);
        assert!(read_fingerprint_file("<html>not found</html>").is_err());
    }

    /// Runs a future that does no waiting (bundled files only).
    fn futures_lite_block_on<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(f)
    }

    #[test]
    fn durations() {
        assert_eq!(parse_duration(NumOrTextPub(NumOrText::Text("3:30".into()))).unwrap(), 210.0);
        assert_eq!(parse_duration(NumOrTextPub(NumOrText::Text("1:02:03".into()))).unwrap(), 3723.0);
        assert_eq!(parse_duration(NumOrTextPub(NumOrText::Num(95.0))).unwrap(), 95.0);
        assert_eq!(slug("Stanford (Richard Powers)"), "stanford-richard-powers");
    }
}

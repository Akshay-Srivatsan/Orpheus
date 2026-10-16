//! What the interface shows: songs' styles (yours, or the club's in a club's
//! set), how they'll play (tempo, length), library rows and set entries, ready
//! to display. Interfaces only lay these out.
//!
//! Songs are tagged with *your* styles (`styles.json` in the library). Without
//! a club they show as they are. In a club's set they show the club's styles
//! they match, narrowed by your mapping or chosen per entry (see `matching`).

use crate::app::fold_bpm;
use crate::club::{Club, Range, Style, TempoChange};
use crate::matching::{self, Assessment, Fit, ShownSource, SongFacts};
use crate::model::*;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;

/// Colour for styles without one (and songs without a style).
pub const NEUTRAL: &str = "#8e8e93";

/// What the views need besides the library: the club in effect and your styles.
#[derive(Clone, Default)]
pub struct ViewContext {
    pub club: Option<Arc<Club>>,
    pub club_id: Option<String>,
    /// Your styles' names at this club: your style key → club style ids
    /// (your rules, and styles of the same name).
    pub mapping: HashMap<String, Vec<String>>,
    pub tempo_change: TempoChange,
    pub styles: StyleLibrary,
    /// Recordings that are the club's suggested songs: recording id → style ids.
    pub song_matches: HashMap<Id, Vec<String>>,
    /// Fade-out length when a song is cut short (seconds).
    pub fade_secs: f64,
    /// Silence at the start and end is skipped.
    pub trim_silence: bool,
}

impl ViewContext {
    /// With a club: its rules, your mapping, and the tempo change it allows
    /// (`default_change_pct` when it doesn't say).
    pub fn with_club(mut self, club: Option<Arc<Club>>, club_id: Option<String>, default_change_pct: f64) -> Self {
        self.tempo_change = club
            .as_ref()
            .and_then(|c| c.set.tempo_change)
            .unwrap_or(TempoChange { faster: default_change_pct, slower: default_change_pct });
        self.mapping = club.as_deref().map(|c| club_names(c, club_id.as_deref(), &self.styles)).unwrap_or_default();
        self.club = club;
        self.club_id = club_id;
        self
    }
}

// ---------------------------------------------------------------- names

pub fn my_style<'a>(lib: &'a StyleLibrary, key: &str) -> Option<&'a LibraryStyle> {
    lib.styles.iter().find(|s| s.key == key)
}

/// "cross-step_waltz" → "Cross Step Waltz".
pub fn prettify(key: &str) -> String {
    let spaced: String = key.chars().map(|c| if c == '-' || c == '_' { ' ' } else { c }).collect();
    spaced
        .split(' ')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Your name for one of your styles.
pub fn style_name(lib: &StyleLibrary, key: &str) -> String {
    my_style(lib, key).map_or_else(|| prettify(key), |s| s.name.clone())
}

/// A name with case, accents, spaces, hyphens and punctuation left out
/// ("Cross-step Waltz" → "crossstepwaltz").
fn name_key(n: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    n.nfkd().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Names are the same when they differ only in case, accents, spacing, hyphens or punctuation.
pub fn same_name(a: &str, b: &str) -> bool {
    name_key(a) == name_key(b)
}

/// The club styles each of your styles may be called at a club: your rules,
/// and those with the same name.
pub fn club_names(club: &Club, club_id: Option<&str>, lib: &StyleLibrary) -> HashMap<String, Vec<String>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    let mut add = |mine: &str, id: &str| {
        let ids = out.entry(mine.to_string()).or_default();
        if club.styles.contains_key(id) && !ids.iter().any(|x| x == id) {
            ids.push(id.to_string());
        }
    };
    for r in club_id.and_then(|c| lib.club_rules.get(c)).into_iter().flatten() {
        add(&r.mine, &r.club);
    }
    for mine in &lib.styles {
        for (id, s) in &club.styles {
            if same_name(&style_def(id, s).name, &mine.name) {
                add(&mine.key, id);
            }
        }
    }
    out
}

// ---------------------------------------------------------------- styles shown

/// A club style's look and tempo ranges.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleDef {
    pub key: String,
    pub name: String,
    pub short: String,
    pub color: String,
    /// Preferred tempo, then the allowed one, as `[min, max]`.
    pub sweet: Option<[f64; 2]>,
    pub extended: Option<[f64; 2]>,
    pub note: Option<String>,
}

fn pair(r: Option<&Range>) -> Option<[f64; 2]> {
    let r = r?;
    if r.min.is_none() && r.max.is_none() {
        return None;
    }
    Some([r.min.unwrap_or(0.0), r.max.unwrap_or(f64::INFINITY)])
}

/// A club style with its tempo ranges as `[min, max]` pairs.
pub fn style_def(id: &str, s: &Style) -> StyleDef {
    StyleDef {
        key: id.to_string(),
        name: s.name.clone(),
        short: s.short.clone().filter(|x| !x.is_empty()).unwrap_or_else(|| s.name.clone()),
        color: s.color.clone().filter(|x| !x.is_empty()).unwrap_or_else(|| NEUTRAL.into()),
        sweet: pair(s.preferred.as_ref()),
        extended: pair(s.tempo.as_ref()),
        note: s.note.clone(),
    }
}

/// How a style looks on a song.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Shown {
    /// Your style key, or the club style id (when `club` is set).
    pub key: String,
    pub name: String,
    pub short: String,
    pub color: String,
    /// The club style (none without a club).
    pub club: Option<StyleDef>,
    /// How the song fits this club style (none without a club or for a slot).
    pub fit: Option<Fit>,
    /// Why the song doesn't fit (the DJ chose it anyway): warnings.
    pub problems: Vec<String>,
    /// Playback rate the style needs (1 = as is).
    pub rate: f64,
    /// Tempo it would be played at for this style (with that rate), in the style's counting.
    pub bpm: Option<f64>,
    /// Shown because the song matches it (no mapping or choice narrowed it down).
    pub auto: bool,
}

/// One of your styles as you name it.
pub fn mine_shown(lib: &StyleLibrary, key: &str) -> Shown {
    let mine = my_style(lib, key);
    let name = mine.map_or_else(|| prettify(key), |m| m.name.clone());
    Shown {
        key: key.to_string(),
        short: mine.map(|m| m.short.clone()).filter(|s| !s.is_empty()).unwrap_or_else(|| name.clone()),
        color: mine.map(|m| m.color.clone()).filter(|s| !s.is_empty()).unwrap_or_else(|| NEUTRAL.into()),
        name,
        club: None,
        fit: None,
        problems: vec![],
        rate: 1.0,
        bpm: None,
        auto: false,
    }
}

/// A club style as shown, with how the song fits it. In a set (`in_set`), a
/// song that would need a tempo change it isn't given plays out of the
/// style's range: a warning.
pub fn club_style_shown(club: &Club, id: &str, fit: Option<&Assessment>, in_set: bool) -> Shown {
    let s = &club.styles[id];
    let def = style_def(id, s);
    let mut problems = fit.map(|f| f.problems.clone()).unwrap_or_default();
    if let Some(f) = fit.filter(|f| in_set && f.fit == Fit::Stretch) {
        if let Some(bpm) = f.bpm {
            let pct = (f.rate - 1.0) * 100.0;
            problems.push(format!(
                "plays at {} BPM; {} is {} BPM ({}{}% would fit)",
                matching::num(bpm / f.rate),
                def.name,
                matching::range_text(s.tempo.as_ref().or(s.preferred.as_ref())),
                if pct > 0.0 { "+" } else { "−" },
                matching::num(pct.abs())
            ));
        }
    }
    Shown {
        key: id.to_string(),
        name: def.name.clone(),
        short: def.short.clone(),
        color: def.color.clone(),
        fit: fit.map(|f| f.fit),
        rate: fit.map_or(1.0, |f| f.rate),
        bpm: fit.and_then(|f| f.bpm),
        club: Some(def),
        problems,
        auto: false,
    }
}

/// Your style keys an entry shows: its own selection, else all of the version's.
pub fn entry_style_keys(version: Option<&Version>, entry: Option<&SetlistEntry>) -> Vec<String> {
    entry.and_then(|e| e.styles.clone()).unwrap_or_else(|| version.map(|v| v.styles.clone()).unwrap_or_default())
}

/// What matching needs to know about a song (none: no song, e.g. a slot).
pub fn song_facts(ctx: &ViewContext, track: &Track, version: &Version, entry: Option<&SetlistEntry>) -> Option<SongFacts> {
    let rec = track.recording_of(&version.id)?;
    Some(SongFacts {
        listed: ctx.song_matches.get(&rec.rec.id).cloned().unwrap_or_default(),
        bpm: rec.rec.bpm,
        target: resolve_settings(Some(version), entry).target_bpm,
        meter: rec.rec.beats_per_bar,
        meter_detected: rec.rec.meter_source.as_deref() != Some("user"),
        length_secs: (rec.rec.duration_ms > 0).then(|| played_duration_ms(ctx, rec, Some(version), entry, 1.0) / 1000.0),
    })
}

/// The styles an entry (or a version) shows: the club's in a club's set, else yours.
pub fn shown_styles(ctx: &ViewContext, facts: Option<&SongFacts>, version: Option<&Version>, entry: Option<&SetlistEntry>) -> Vec<Shown> {
    let keys = entry_style_keys(version, entry);
    let Some(club) = ctx.club.as_deref() else {
        return keys.iter().map(|k| mine_shown(&ctx.styles, k)).collect();
    };
    let chosen = entry.and_then(|e| e.club_styles.as_deref());
    let r = matching::club_shown(club, facts, ctx.tempo_change, chosen, &keys, &ctx.mapping);
    r.ids
        .iter()
        .map(|id| Shown { auto: r.source == ShownSource::Matches, ..club_style_shown(club, id, r.fits.get(id), entry.is_some()) })
        .collect()
}

/// The styles a song (one version, maybe in a set) shows.
pub fn shown_for(ctx: &ViewContext, track: &Track, version: &Version, entry: Option<&SetlistEntry>) -> Vec<Shown> {
    shown_styles(ctx, song_facts(ctx, track, version, entry).as_ref(), Some(version), entry)
}

/// A custom label replacing style names: the entry's, else the version's display name.
pub fn custom_label(version: Option<&Version>, entry: Option<&SetlistEntry>) -> Option<String> {
    entry
        .and_then(|e| e.label.clone())
        .filter(|l| !l.is_empty())
        .or_else(|| version.and_then(|v| v.display_name.clone()).filter(|l| !l.is_empty()))
}

/// The text dancers see: a custom label, else the shown styles joined.
pub fn label_of(shown: &[Shown], custom: Option<&str>, sep: &str) -> String {
    match custom {
        Some(c) if !c.is_empty() => c.to_string(),
        _ => shown.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(sep),
    }
}

// ---------------------------------------------------------------- tempo and length

/// Effective settings: entry overrides, then the version's.
pub fn resolve_settings(version: Option<&Version>, entry: Option<&SetlistEntry>) -> Overrides {
    let e = entry.map(|e| &e.overrides);
    let v = version.map(|v| &v.settings);
    macro_rules! pick {
        ($f:ident) => {
            e.and_then(|e| e.$f.clone()).or_else(|| v.and_then(|v| v.$f.clone()))
        };
    }
    Overrides {
        target_bpm: pick!(target_bpm),
        cue_in_ms: pick!(cue_in_ms),
        cue_out_ms: pick!(cue_out_ms),
        fade_out_ms: pick!(fade_out_ms),
        gain_db: pick!(gain_db),
        stems: pick!(stems),
    }
}

/// Centre of a style's preferred range (else its allowed one).
pub fn style_center(s: &StyleDef) -> Option<f64> {
    s.sweet.or(s.extended).map(|r| (r[0] + r[1]) / 2.0)
}

/// A tempo in the counting of a style (folded to half or double time).
pub fn bpm_for_style(bpm: Option<f64>, style: Option<&StyleDef>) -> Option<f64> {
    let b = bpm?;
    Some(style.and_then(style_center).map_or(b, |c| fold_bpm(b, c)))
}

/// How a version will be played.
#[derive(Debug, Clone, Copy, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Playback {
    /// Original tempo in the style's counting.
    pub base: Option<f64>,
    /// Tempo it will be played at.
    pub target: Option<f64>,
    /// Playback rate (target / base).
    pub rate: f64,
}

/// How a version will be played (maybe within a set entry), as the decks load it.
pub fn playback(rec: Option<&Recording>, version: &Version, entry: Option<&SetlistEntry>, club_style: Option<&StyleDef>) -> Playback {
    let target = resolve_settings(Some(version), entry).target_bpm;
    let Some(bpm) = rec.and_then(|r| r.rec.bpm).filter(|b| *b > 0.0) else {
        return Playback { base: None, target, rate: 1.0 };
    };
    if let Some(t) = target {
        let base = fold_bpm(bpm, t);
        return Playback { base: Some(base), target, rate: (t / base).clamp(0.5, 2.0) };
    }
    let base = bpm_for_style(Some(bpm), club_style);
    Playback { base, target: base, rate: 1.0 }
}

/// Audible length in ms (cue points, fade-out, silence trimming and tempo applied).
pub fn played_duration_ms(ctx: &ViewContext, rec: &Recording, version: Option<&Version>, entry: Option<&SetlistEntry>, rate: f64) -> f64 {
    let r = resolve_settings(version, entry);
    let trim = ctx.trim_silence;
    let start = r.cue_in_ms.or(if trim { rec.rec.audio_start_ms } else { None }).unwrap_or(0);
    let mut end = r.cue_out_ms.or(if trim { rec.rec.audio_end_ms } else { None }).unwrap_or(rec.rec.duration_ms);
    if let Some(f) = r.fade_out_ms {
        end = end.min(f + (ctx.fade_secs * 1000.0) as i64);
    }
    (end - start).max(0) as f64 / if rate > 0.0 { rate } else { 1.0 }
}

/// Where a tempo sits in a style: its sweet spot, the allowed range, outside, or unknown.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TempoFit {
    Sweet,
    Extended,
    Outside,
    Unknown,
}

/// Tempo ranges count as met within half a BPM.
const RANGE_SLACK_BPM: f64 = 0.5;
/// Without an allowed range, this much around the sweet spot is acceptable.
const SWEET_ONLY_SLACK: f64 = 0.1;

pub fn tempo_fit(bpm: Option<f64>, style: Option<&StyleDef>) -> TempoFit {
    let (Some(b), Some(s)) = (bpm, style) else { return TempoFit::Unknown };
    let within = |r: [f64; 2]| b >= r[0] - RANGE_SLACK_BPM && b <= r[1] + RANGE_SLACK_BPM;
    match (s.sweet, s.extended) {
        (None, None) => TempoFit::Unknown,
        (Some(r), _) if within(r) => TempoFit::Sweet,
        (_, Some(r)) if within(r) => TempoFit::Extended,
        (Some(r), None) if b >= r[0] * (1.0 - SWEET_ONLY_SLACK) && b <= r[1] * (1.0 + SWEET_ONLY_SLACK) => TempoFit::Extended,
        _ => TempoFit::Outside,
    }
}

/// The first club style a song shows (for tempo ranges).
pub fn club_style_of(shown: &[Shown]) -> Option<&StyleDef> {
    shown.iter().find_map(|s| s.club.as_ref())
}

// ---------------------------------------------------------------- library view

/// The style filter's value for songs without a style.
pub const NO_STYLE: &str = "__none";
/// Songs longer than this are marked long (ms).
const LONG_SONG_MS: f64 = 270_000.0;

/// Which songs a library view lists.
#[derive(Debug, Clone, serde::Deserialize, Default, PartialEq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum View {
    #[default]
    All,
    /// Songs whose audio is missing.
    Missing,
    /// The club's suggested songs that you have.
    ClubSongs,
    Crate { id: Id },
    Setlist { id: Id },
    History,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SortKey {
    /// The view's own order (crates, setlists, history).
    Position,
    Title,
    #[default]
    Artist,
    Version,
    Style,
    Bpm,
    Time,
    Meter,
    Plays,
    Played,
}

/// What a library view shows and how.
#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct LibraryQuery {
    pub view: View,
    /// Your style keys (or `NO_STYLE`); empty = all.
    pub style_filter: Vec<String>,
    /// Words that must all appear.
    pub search: String,
    pub sort: Option<SortKey>,
    pub descending: bool,
    /// Fold songs' versions under one row…
    pub fold: bool,
    /// …except these songs (or, unfolded, fold these).
    pub toggled: Vec<Id>,
    /// With one style filtered on, also suggest unlabelled songs whose tempo fits it.
    pub suggest_by_tempo: bool,
}

/// Where a song plays from.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Availability {
    /// A file on this computer.
    File,
    SpotifyOnly,
    /// Both.
    FileAndSpotify,
    /// Nothing plays it here.
    Missing,
}

/// A row: one version of a song.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryRow {
    /// Unique in the view.
    pub key: String,
    pub version_id: Id,
    pub track_id: Id,
    pub recording_id: Id,
    /// In a crate, setlist or history.
    pub position: Option<usize>,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// The version's name where it matters (several versions, or not "Original").
    pub version_label: String,
    pub comment: String,
    /// Your styles on it.
    pub styles: Vec<Shown>,
    /// Not labelled with the filtered style, but its tempo fits.
    pub suggested: bool,
    /// Tempo it plays at (in its club style's counting), and the rate that takes.
    pub bpm: Option<f64>,
    pub rate: f64,
    pub tempo_fit: TempoFit,
    /// Analysis hasn't finished yet.
    pub analyzing: bool,
    pub duration_ms: f64,
    pub long: bool,
    pub meter: Option<u8>,
    pub meter_by_you: bool,
    pub plays: i64,
    pub played_at: Option<i64>,
    /// The set entry a history row comes from.
    pub history_entry_id: Option<Id>,
    pub availability: Availability,
    /// Played (or playing) in the open set.
    pub played_in_set: bool,
    /// Folded versions under this row: how many, and whether they're shown.
    pub fold_count: usize,
    pub fold_open: bool,
    /// One of a song's other versions, shown under it.
    pub child: bool,
    /// The song's versions, and whether this one stands for it.
    pub version_count: usize,
    pub is_primary: bool,
}

/// A style filter chip: how many songs here have it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleCount {
    pub key: String,
    pub name: String,
    pub short: String,
    pub color: String,
    pub songs: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryView {
    pub rows: Vec<LibraryRow>,
    /// Your styles with songs here (and the selected ones), then songs without one.
    pub style_counts: Vec<StyleCount>,
    pub no_style_songs: usize,
    /// The view lists songs (it folds versions); otherwise particular versions.
    pub foldable: bool,
    /// The sort in effect (views have their own default).
    pub sort: SortKey,
    /// Songs in the library at all (for the empty view's hint).
    pub library_songs: usize,
    /// With one style filtered on: the club styles it's called at the club.
    pub filter_targets: Vec<StyleDef>,
}

/// Inputs to a library view besides the query.
pub struct LibraryData<'a> {
    pub tracks: &'a [Track],
    pub crates: &'a [Crate],
    /// The setlist a setlist view lists, and the one that's open (played songs).
    pub setlist: Option<&'a Setlist>,
    pub open_set: Option<&'a Setlist>,
    pub history: &'a [HistoryItem],
    pub now: i64,
}

struct Item<'a> {
    track: &'a Track,
    version: &'a Version,
    rec: Option<&'a Recording>,
    key: String,
    position: Option<usize>,
    played_at: Option<i64>,
    history_entry_id: Option<Id>,
    suggested: bool,
}

fn item<'a>(track: &'a Track, version: &'a Version, key: String) -> Item<'a> {
    Item { track, version, rec: track.recording_of(&version.id), key, position: None, played_at: None, history_entry_id: None, suggested: false }
}

/// The version that stands for a song: its primary, else the first.
pub fn primary_version(t: &Track) -> Option<&Version> {
    t.rec.primary.as_ref().and_then(|p| t.rec.versions.iter().find(|v| &v.id == p)).or(t.rec.versions.first())
}

/// A version's name where it matters: several versions, or not "Original".
pub fn version_label(v: &Version, t: &Track) -> String {
    if t.rec.versions.len() > 1 || v.name != "Original" {
        v.name.clone()
    } else {
        String::new()
    }
}

fn availability(rec: Option<&Recording>) -> Availability {
    let Some(r) = rec else { return Availability::Missing };
    let spotify = r.rec.sources.iter().any(|s| s.kind == "spotify");
    match (r.files.is_empty(), spotify) {
        (false, true) => Availability::FileAndSpotify,
        (false, false) => Availability::File,
        (true, true) => Availability::SpotifyOnly,
        (true, false) => Availability::Missing,
    }
}

pub fn library_view(ctx: &ViewContext, data: &LibraryData, q: &LibraryQuery) -> LibraryView {
    let in_library: Vec<&Track> = data.tracks.iter().filter(|t| t.rec.in_library).collect();
    let by_version: HashMap<&str, (&Track, &Version)> =
        data.tracks.iter().flat_map(|t| t.rec.versions.iter().map(move |v| (v.id.as_str(), (t, v)))).collect();
    let library_items = || in_library.iter().flat_map(|t| t.rec.versions.iter().filter(|v| !v.hidden).map(move |v| (*t, v)));
    let crate_ = match &q.view {
        View::Crate { id } => data.crates.iter().find(|c| &c.id == id),
        _ => None,
    };
    let smart_rules = crate_.and_then(|c| c.smart.as_ref()).and_then(|s| serde_json::from_value::<crate::smart::Rules>(s.clone()).ok());
    let plain_crate = crate_.filter(|c| c.smart.is_none());

    let base: Vec<Item> = match &q.view {
        View::All => library_items().map(|(t, v)| item(t, v, v.id.clone())).collect(),
        View::Missing => library_items().filter(|(t, v)| t.recording_of(&v.id).map_or(true, |r| r.missing)).map(|(t, v)| item(t, v, v.id.clone())).collect(),
        View::ClubSongs => data
            .tracks
            .iter()
            .flat_map(|t| t.rec.versions.iter().filter(|v| !v.hidden).map(move |v| (t, v)))
            .filter(|(t, v)| t.recording_of(&v.id).is_some_and(|r| ctx.song_matches.get(&r.rec.id).is_some_and(|m| !m.is_empty())))
            .map(|(t, v)| item(t, v, v.id.clone()))
            .collect(),
        View::Crate { .. } => match (&smart_rules, plain_crate) {
            (Some(rules), _) => {
                let candidates: Vec<crate::smart::Candidate> =
                    library_items().map(|(track, version)| crate::smart::Candidate { track, version }).collect();
                let ev = crate::smart::Evaluator { ctx, crates: data.crates, library: &candidates, now: data.now };
                candidates.iter().filter(|c| ev.matches(rules, c, 0)).map(|c| item(c.track, c.version, c.version.id.clone())).collect()
            }
            (None, Some(c)) => c
                .version_ids
                .iter()
                .enumerate()
                .filter_map(|(i, id)| by_version.get(id.as_str()).map(|(t, v)| Item { position: Some(i + 1), ..item(t, v, id.clone()) }))
                .collect(),
            _ => vec![],
        },
        View::Setlist { .. } => data
            .setlist
            .map(|s| {
                s.entries
                    .iter()
                    .enumerate()
                    .filter_map(|(i, e)| {
                        let (t, v) = by_version.get(e.version_id.as_deref()?)?;
                        Some(Item { position: Some(i + 1), ..item(t, v, e.id.clone()) })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        View::History => data
            .history
            .iter()
            .enumerate()
            .filter_map(|(i, h)| {
                let (t, v) = match h.version_id.as_deref().and_then(|id| by_version.get(id)) {
                    Some(tv) => *tv,
                    None => {
                        let t = data.tracks.iter().find(|t| t.rec.id == h.track_id)?;
                        (t, primary_version(t)?)
                    }
                };
                Some(Item {
                    played_at: Some(h.played_at),
                    history_entry_id: Some(h.entry_id.clone()),
                    position: Some(i + 1),
                    ..item(t, v, format!("{}-{i}", h.entry_id))
                })
            })
            .collect(),
    };

    // Style chips count songs (not versions) in the view.
    let mut songs_with: HashMap<&str, std::collections::HashSet<&str>> = HashMap::new();
    for it in &base {
        let keys: Vec<&str> = if it.version.styles.is_empty() { vec![NO_STYLE] } else { it.version.styles.iter().map(String::as_str).collect() };
        for k in keys {
            songs_with.entry(k).or_default().insert(it.track.rec.id.as_str());
        }
    }
    let count = |k: &str| songs_with.get(k).map_or(0, |s| s.len());
    let mut chip_keys: Vec<String> = ctx.styles.styles.iter().map(|s| s.key.clone()).collect();
    for k in songs_with.keys() {
        if *k != NO_STYLE && !chip_keys.iter().any(|x| x == k) {
            chip_keys.push(k.to_string());
        }
    }
    let style_counts = chip_keys
        .iter()
        .filter(|k| count(k) > 0 || q.style_filter.contains(k))
        .map(|k| {
            let s = mine_shown(&ctx.styles, k);
            StyleCount { songs: count(k), key: s.key, name: s.name, short: s.short, color: s.color }
        })
        .collect();

    // The single style filtered on, if exactly one is.
    let filter_style = match q.style_filter.as_slice() {
        [one] if one != NO_STYLE => Some(one.clone()),
        _ => None,
    };
    let filter_targets: Vec<StyleDef> = filter_style
        .as_deref()
        .and_then(|f| {
            let club = ctx.club.as_deref()?;
            Some(ctx.mapping.get(f)?.iter().filter(|id| club.styles.contains_key(*id)).map(|id| style_def(id, &club.styles[id])).collect())
        })
        .unwrap_or_default();

    // How a row plays: in the counting of its club style (the filtered one, if it has it).
    let entry_for = |it: &Item| -> Option<SetlistEntry> {
        let f = filter_style.as_ref()?;
        (it.suggested || it.version.styles.contains(f)).then(|| SetlistEntry { styles: Some(vec![f.clone()]), ..Default::default() })
    };
    let info = |it: &Item| {
        let entry = entry_for(it);
        let shown = shown_for(ctx, it.track, it.version, entry.as_ref());
        let st = club_style_of(&shown).cloned();
        let pb = playback(it.rec, it.version, entry.as_ref(), st.as_ref());
        let dur = it.rec.map_or(0.0, |r| played_duration_ms(ctx, r, Some(it.version), None, pb.rate));
        (pb, tempo_fit(pb.target, st.as_ref()), dur)
    };

    let mut items: Vec<Item> = if q.style_filter.is_empty() {
        base
    } else {
        let keys = &q.style_filter;
        let (labelled, rest): (Vec<Item>, Vec<Item>) = base.into_iter().partition(|it| {
            if it.version.styles.is_empty() {
                keys.iter().any(|k| k == NO_STYLE)
            } else {
                it.version.styles.iter().any(|k| keys.contains(k))
            }
        });
        let mut out = labelled;
        if filter_style.is_some() && q.suggest_by_tempo && filter_targets.iter().any(|t| t.sweet.is_some() || t.extended.is_some()) {
            let have: std::collections::HashSet<Id> = out.iter().map(|it| it.track.rec.id.clone()).collect();
            let suggested: Vec<Item> = rest
                .into_iter()
                .filter(|it| it.version.styles.is_empty() && !have.contains(&it.track.rec.id) && it.rec.is_some_and(|r| r.rec.bpm.is_some()))
                .map(|it| Item { key: format!("s-{}", it.key), suggested: true, ..it })
                .filter(|it| {
                    let entry = entry_for(it);
                    let shown = shown_for(ctx, it.track, it.version, entry.as_ref());
                    let st = club_style_of(&shown).cloned();
                    let pb = playback(it.rec, it.version, entry.as_ref(), st.as_ref());
                    st.is_some() && tempo_fit(pb.target, st.as_ref()) != TempoFit::Outside
                })
                .collect();
            out.extend(suggested);
        }
        out
    };

    let search = q.search.trim().to_lowercase();
    if !search.is_empty() {
        let words: Vec<&str> = search.split_whitespace().collect();
        items.retain(|it| {
            let names: Vec<String> = it.version.styles.iter().map(|k| style_name(&ctx.styles, k)).collect();
            let hay = format!(
                "{} {} {} {} {} {} {}",
                it.track.rec.title,
                it.track.rec.artist,
                it.track.rec.album,
                it.version.name,
                it.version.comment,
                names.join(" "),
                it.track.rec.notes
            )
            .to_lowercase();
            words.iter().all(|w| hay.contains(w))
        });
    }

    // Sort: crates and setlists default to their own order; history to when.
    let natural = plain_crate.is_some() || matches!(q.view, View::Setlist { .. } | View::History);
    let sort = match q.sort {
        Some(SortKey::Position) if !natural => SortKey::Artist,
        Some(k) => k,
        None if natural => SortKey::Position,
        None => SortKey::Artist,
    };
    let mut rows: Vec<(Item, (Playback, TempoFit, f64))> = items.into_iter().map(|it| {
        let i = info(&it);
        (it, i)
    }).collect();
    #[derive(PartialEq, PartialOrd)]
    enum K {
        N(f64),
        S(String),
    }
    let val = |(it, (pb, _, dur)): &(Item, (Playback, TempoFit, f64))| -> K {
        let t = &it.track.rec;
        match sort {
            SortKey::Position => K::N(it.position.unwrap_or(0) as f64),
            SortKey::Title => K::S(t.title.to_lowercase()),
            SortKey::Artist => K::S(format!("{}\u{0}{}", t.artist.to_lowercase(), t.title.to_lowercase())),
            SortKey::Version => K::S(it.version.name.to_lowercase()),
            SortKey::Style => K::S(it.version.styles.iter().map(|k| style_name(&ctx.styles, k)).collect::<Vec<_>>().join(" ")),
            SortKey::Bpm => K::N(pb.target.or(it.rec.and_then(|r| r.rec.bpm)).unwrap_or(0.0)),
            SortKey::Time => K::N(*dur),
            SortKey::Meter => K::N(it.rec.and_then(|r| r.rec.beats_per_bar).unwrap_or(0) as f64),
            SortKey::Plays => K::N(it.track.play_count as f64),
            SortKey::Played => K::N(it.played_at.or(it.track.last_played_at).unwrap_or(0) as f64),
        }
    };
    rows.sort_by(|a, b| {
        // Suggestions come after the songs labelled with the style.
        a.0.suggested.cmp(&b.0.suggested).then_with(|| {
            let o = val(a).partial_cmp(&val(b)).unwrap_or(std::cmp::Ordering::Equal);
            if q.descending {
                o.reverse()
            } else {
                o
            }
        })
    });

    let played_in_set: std::collections::HashSet<&str> = data
        .open_set
        .map(|s| {
            s.entries
                .iter()
                .filter(|e| e.status == "played" || e.status == "playing")
                .filter_map(|e| by_version.get(e.version_id.as_deref()?).map(|(t, _)| t.rec.id.as_str()))
                .collect()
        })
        .unwrap_or_default();
    let to_row = |it: &Item, (pb, fit, dur): &(Playback, TempoFit, f64)| LibraryRow {
        key: it.key.clone(),
        version_id: it.version.id.clone(),
        track_id: it.track.rec.id.clone(),
        recording_id: it.version.recording.clone(),
        position: it.position,
        title: it.track.rec.title.clone(),
        artist: it.track.rec.artist.clone(),
        album: it.track.rec.album.clone(),
        version_label: version_label(it.version, it.track),
        comment: it.version.comment.clone(),
        styles: it.version.styles.iter().map(|k| mine_shown(&ctx.styles, k)).collect(),
        suggested: it.suggested,
        bpm: pb.target.or(it.rec.and_then(|r| r.rec.bpm)),
        rate: pb.rate,
        tempo_fit: *fit,
        analyzing: it.rec.is_some_and(|r| r.rec.bpm.is_none() && !r.rec.analyzed && !r.files.is_empty()),
        duration_ms: *dur,
        long: *dur > LONG_SONG_MS,
        meter: it.rec.and_then(|r| r.rec.beats_per_bar),
        meter_by_you: it.rec.is_some_and(|r| r.rec.meter_source.as_deref() == Some("user")),
        plays: it.track.play_count,
        played_at: it.played_at,
        history_entry_id: it.history_entry_id.clone(),
        availability: availability(it.rec),
        played_in_set: !matches!(q.view, View::History) && played_in_set.contains(it.track.rec.id.as_str()),
        fold_count: 0,
        fold_open: false,
        child: false,
        version_count: it.track.rec.versions.len(),
        is_primary: primary_version(it.track).is_some_and(|p| p.id == it.version.id),
    };

    // Views listing songs fold a song's versions under the one standing for it.
    let foldable = matches!(q.view, View::All | View::Missing | View::ClubSongs) || smart_rules.is_some();
    let out_rows = if !foldable {
        rows.iter().map(|(it, i)| to_row(it, i)).collect()
    } else {
        let mut order: Vec<&str> = vec![];
        let mut groups: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, (it, _)) in rows.iter().enumerate() {
            let id = it.track.rec.id.as_str();
            groups.entry(id).or_insert_with(|| {
                order.push(id);
                vec![]
            }).push(i);
        }
        let mut out = vec![];
        for id in order {
            let g = &groups[id];
            if g.len() == 1 {
                out.push(to_row(&rows[g[0]].0, &rows[g[0]].1));
                continue;
            }
            let primary = primary_version(rows[g[0]].0.track).map(|v| v.id.clone());
            let head = *g.iter().find(|&&i| Some(&rows[i].0.version.id) == primary.as_ref()).unwrap_or(&g[0]);
            let open = q.fold == q.toggled.iter().any(|t| t == id);
            out.push(LibraryRow { fold_count: g.len(), fold_open: open, ..to_row(&rows[head].0, &rows[head].1) });
            if open {
                for &i in g.iter().filter(|&&i| i != head) {
                    out.push(LibraryRow { child: true, ..to_row(&rows[i].0, &rows[i].1) });
                }
            }
        }
        out
    };

    LibraryView { rows: out_rows, style_counts, no_style_songs: count(NO_STYLE), foldable, sort, library_songs: in_library.len(), filter_targets }
}

/// Counts the sidebar shows.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySummary {
    pub songs: usize,
    /// Songs whose audio is missing here.
    pub missing: usize,
    /// The club's suggested songs that you have.
    pub club_songs: usize,
    /// Songs (versions) in each crate, smart crates evaluated.
    pub crates: HashMap<Id, usize>,
}

pub fn library_summary(ctx: &ViewContext, tracks: &[Track], crates: &[Crate], now: i64) -> LibrarySummary {
    let in_library: Vec<&Track> = tracks.iter().filter(|t| t.rec.in_library).collect();
    let candidates: Vec<crate::smart::Candidate> = in_library
        .iter()
        .flat_map(|t| t.rec.versions.iter().filter(|v| !v.hidden).map(move |v| crate::smart::Candidate { track: t, version: v }))
        .collect();
    let ev = crate::smart::Evaluator { ctx, crates, library: &candidates, now };
    let club_songs: std::collections::HashSet<&str> = tracks
        .iter()
        .filter(|t| t.recordings.iter().any(|r| ctx.song_matches.get(&r.rec.id).is_some_and(|m| !m.is_empty())))
        .map(|t| t.rec.id.as_str())
        .collect();
    LibrarySummary {
        songs: in_library.len(),
        missing: in_library.iter().filter(|t| t.missing).count(),
        club_songs: club_songs.len(),
        crates: crates.iter().map(|c| (c.id.clone(), ev.members(c, 0).len())).collect(),
    }
}

// ---------------------------------------------------------------- set view

/// More automatic matches than this show as "A / B +2".
const SHOWN_AUTO_MATCHES: usize = 2;

/// An entry of a set, ready to show.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetRow {
    pub entry: SetlistEntry,
    /// Song number (pauses and skipped entries aren't counted).
    pub number: Option<usize>,
    pub is_pause: bool,
    /// The song (none for a slot or a pause).
    pub track_id: Option<Id>,
    pub recording_id: Option<Id>,
    pub title: String,
    pub artist: String,
    pub version_label: String,
    /// What dancers see: a custom label, else the styles shown (short names).
    pub label: String,
    /// The styles shown (full names for tooltips; colours).
    pub styles: Vec<Shown>,
    /// In a club's set: a song that is none of its styles.
    pub no_style: bool,
    /// The tempo it plays at, and the rate that takes.
    pub bpm: Option<f64>,
    pub rate: f64,
    /// Playing length (a pause: its length) in ms.
    pub duration_ms: f64,
    pub long: bool,
    pub warnings: Vec<String>,
    /// A song coming up that the room already heard in this set.
    pub repeat: bool,
    /// Its recording was already played in this set as another song (that song's title).
    pub heard_as: Option<String>,
    /// Club tags on it: name and colour.
    pub tags: Vec<TagShown>,
    /// The styles it can be shown as (Show As): the club's in a club's set
    /// (those it fits first), else the version's own.
    pub show_as: Vec<ShowAs>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowAs {
    pub key: String,
    pub name: String,
    /// The song fits it (a club style it doesn't fit can still be chosen, with a warning).
    pub fits: bool,
    /// Shown now.
    pub on: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagShown {
    pub key: String,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetView {
    pub setlist: Setlist,
    /// The club whose guidelines it follows.
    pub club: Option<ClubSummary>,
    pub rows: Vec<SetRow>,
    /// Everything not skipped, with pauses between songs (ms).
    pub total_ms: f64,
    /// What's still to come (ms).
    pub remaining_ms: f64,
    /// The set against its club's guidelines (none without a club).
    pub check: Option<crate::matching::SetCheck>,
    pub warnings: usize,
}

const NO_AUDIO_REMOVED: &str = "Its audio was removed from the library (Library → Files adds it again)";
const NO_AUDIO_HERE: &str = "Its file isn't on this computer";

/// A set, ready to show. `gap_secs`: the pause between songs.
pub fn set_view(ctx: &ViewContext, set: &Setlist, tracks: &[Track], gap_secs: f64) -> SetView {
    let by_version: HashMap<&str, (&Track, &Version)> =
        tracks.iter().flat_map(|t| t.rec.versions.iter().map(move |v| (v.id.as_str(), (t, v)))).collect();
    let played = |e: &SetlistEntry| e.status == "played" || e.status == "playing";
    // Songs (and recordings) the room already heard in this set.
    let mut heard_songs: std::collections::HashSet<&str> = Default::default();
    let mut heard_recordings: HashMap<&str, &str> = HashMap::new();
    for e in set.entries.iter().filter(|e| played(e)) {
        if let Some((t, v)) = e.version_id.as_deref().and_then(|id| by_version.get(id)) {
            heard_songs.insert(&t.rec.id);
            heard_recordings.entry(v.recording.as_str()).or_insert(&t.rec.title);
        }
    }
    let club = ctx.club.as_deref();
    let mut number = 0;
    let rows: Vec<SetRow> = set
        .entries
        .iter()
        .map(|e| {
            let is_pause = e.kind == EntryKind::Pause;
            let tv = if is_pause { None } else { e.version_id.as_deref().and_then(|id| by_version.get(id)).copied() };
            let rec = tv.and_then(|(t, v)| t.recording_of(&v.id));
            let shown = match tv {
                Some((t, v)) => shown_for(ctx, t, v, Some(e)),
                None if is_pause => vec![],
                None => shown_styles(ctx, None, None, Some(e)),
            };
            let st = club_style_of(&shown).cloned();
            let pb = tv.map(|(_, v)| playback(rec, v, Some(e), st.as_ref()));
            let dur = if is_pause {
                e.pause_before.unwrap_or(gap_secs) * 1000.0
            } else {
                match (tv, rec) {
                    (Some((_, v)), Some(r)) => played_duration_ms(ctx, r, Some(v), Some(e), pb.map_or(1.0, |p| p.rate)),
                    _ => 0.0,
                }
            };
            let mut warnings: Vec<String> = vec![];
            for p in shown.iter().flat_map(|s| &s.problems) {
                if !warnings.contains(p) {
                    warnings.push(p.clone());
                }
            }
            if let Some((t, v)) = tv {
                if t.recording_of(&v.id).map_or(true, |r| r.missing) {
                    let removed = rec.map_or(true, |r| r.rec.removed || r.rec.sources.is_empty());
                    warnings.insert(0, if removed { NO_AUDIO_REMOVED } else { NO_AUDIO_HERE }.to_string());
                }
            }
            let custom = custom_label(tv.map(|(_, v)| v), Some(e));
            let no_style = club.is_some() && tv.is_some() && shown.is_empty() && custom.is_none();
            let many = shown.len() > SHOWN_AUTO_MATCHES && shown.first().is_some_and(|s| s.auto);
            let label = custom.clone().unwrap_or_else(|| {
                let shorts: Vec<&str> = shown.iter().map(|s| s.short.as_str()).collect();
                if many {
                    format!("{} +{}", shorts[..SHOWN_AUTO_MATCHES].join(" / "), shown.len() - SHOWN_AUTO_MATCHES)
                } else {
                    shorts.join(" / ")
                }
            });
            let pending = e.status == "pending";
            let repeat = pending && tv.is_some_and(|(t, _)| heard_songs.contains(t.rec.id.as_str()));
            let heard_as = if pending && !repeat {
                tv.and_then(|(_, v)| heard_recordings.get(v.recording.as_str())).map(|s| s.to_string())
            } else {
                None
            };
            let counted = !is_pause && e.status != "skipped";
            let num = counted.then(|| {
                number += 1;
                number
            });
            let tags = e
                .tags
                .iter()
                .map(|k| {
                    let t = club.and_then(|c| c.tags.get(k));
                    TagShown {
                        key: k.clone(),
                        name: t.map_or_else(|| k.clone(), |t| t.name.clone()),
                        color: t.and_then(|t| t.color.clone()).unwrap_or_else(|| NEUTRAL.into()),
                    }
                })
                .collect();
            let on = |k: &str| shown.iter().any(|s| s.key == k);
            let show_as: Vec<ShowAs> = if is_pause {
                vec![]
            } else if let Some(club) = club {
                let facts = tv.and_then(|(t, v)| song_facts(ctx, t, v, Some(e)));
                let mut all: Vec<ShowAs> = club
                    .styles
                    .iter()
                    .map(|(id, st)| ShowAs {
                        key: id.clone(),
                        name: st.name.clone(),
                        fits: facts.as_ref().map_or(true, |f| crate::matching::assess(id, st, f, ctx.tempo_change).ok),
                        on: on(id),
                    })
                    .collect();
                all.sort_by_key(|x| !x.fits);
                all
            } else {
                tv.map(|(_, v)| v.styles.iter().map(|k| ShowAs { key: k.clone(), name: style_name(&ctx.styles, k), fits: true, on: on(k) }).collect()).unwrap_or_default()
            };
            SetRow {
                show_as,
                entry: e.clone(),
                number: num,
                is_pause,
                track_id: tv.map(|(t, _)| t.rec.id.clone()),
                recording_id: tv.map(|(_, v)| v.recording.clone()),
                title: match (tv, is_pause) {
                    (Some((t, _)), _) => t.rec.title.clone(),
                    (None, true) => e.label.clone().filter(|l| !l.is_empty()).unwrap_or_else(|| "Pause".into()),
                    (None, false) if shown.is_empty() => "Empty slot".into(),
                    (None, false) => format!("{} slot", shown.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(" / ")),
                },
                artist: tv.map(|(t, _)| t.rec.artist.clone()).unwrap_or_default(),
                version_label: tv.map(|(t, v)| version_label(v, t)).unwrap_or_default(),
                label: if is_pause { String::new() } else { label },
                styles: shown,
                no_style,
                bpm: pb.and_then(|p| p.target),
                rate: pb.map_or(1.0, |p| p.rate),
                duration_ms: dur,
                long: dur > LONG_SONG_MS,
                warnings,
                repeat,
                heard_as,
                tags,
            }
        })
        .collect();

    let kept: Vec<&SetRow> = rows.iter().filter(|r| r.entry.status != "skipped").collect();
    let songs = kept.iter().filter(|r| !r.is_pause).count();
    let total_ms = kept.iter().map(|r| r.duration_ms).sum::<f64>() + songs.saturating_sub(1) as f64 * gap_secs * 1000.0;
    let remaining_ms = rows.iter().filter(|r| r.entry.status == "pending").map(|r| r.duration_ms + gap_secs * 1000.0).sum();

    let check = club.map(|club| {
        let mut items = vec![];
        for (i, r) in rows.iter().enumerate() {
            if r.is_pause || r.entry.status == "skipped" {
                continue;
            }
            let pause_before = if i > 0 && rows[i - 1].is_pause { rows[i - 1].duration_ms / 1000.0 } else { 0.0 };
            items.push(crate::matching::SetItem {
                entry_id: r.entry.id.clone(),
                title: r.title.clone(),
                ids: r.styles.iter().map(|s| s.key.clone()).collect(),
                fits: r
                    .styles
                    .iter()
                    .map(|s| {
                        let a = Assessment {
                            id: s.key.clone(),
                            ok: s.problems.is_empty(),
                            fit: s.fit.unwrap_or(Fit::Unchecked),
                            rate: s.rate,
                            bpm: None,
                            problems: s.problems.clone(),
                        };
                        (s.key.clone(), a)
                    })
                    .collect(),
                length_secs: r.track_id.as_ref().map(|_| r.duration_ms / 1000.0),
                pause_secs: r.entry.pause_before.unwrap_or(gap_secs) + pause_before,
            });
        }
        // A set whose songs have all been played, or a planned set with no slots left, is finished.
        let complete = if set.mode == "preset" {
            rows.iter().filter(|r| !r.is_pause && r.entry.status != "skipped").all(|r| r.track_id.is_some())
        } else {
            rows.iter().all(|r| r.entry.status != "pending")
        };
        let mut result = crate::matching::check_set(club, &items, complete);
        // Songs that match several styles with nothing to choose between them.
        for r in &rows {
            let many = r.styles.len() > SHOWN_AUTO_MATCHES && r.styles.first().is_some_and(|s| s.auto);
            if many && r.entry.status != "skipped" {
                let shorts: Vec<&str> = r.styles.iter().map(|s| s.short.as_str()).collect();
                result.issues.push(crate::matching::Issue {
                    level: crate::matching::Level::Note,
                    text: format!(
                        "{}: matches {} styles ({}); choose with right-click → Show As, or map your styles (Settings → Styles & Tags)",
                        r.title,
                        r.styles.len(),
                        shorts.join(", ")
                    ),
                    entries: vec![r.entry.id.clone()],
                });
            }
        }
        result
    });
    let warnings = check.as_ref().map_or(0, |c| c.issues.iter().filter(|i| i.level == crate::matching::Level::Warn).count());
    SetView { setlist: set.clone(), club: None, rows, total_ms, remaining_ms, check, warnings }
}

// ---------------------------------------------------------------- clubs

/// What the interface shows of a club's guidelines (one version).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClubSummary {
    pub id: String,
    pub version: String,
    /// "Dancebreak", or "Dancebreak (Spring 2026)" for an older version.
    pub label: String,
    pub name: String,
    /// A newer version exists: its id.
    pub newer_version: Option<String>,
    /// Planned set length (minutes).
    pub set_minutes: Option<f64>,
    pub announcements: Vec<String>,
    pub styles: Vec<StyleDef>,
    pub tags: Vec<TagShown>,
}

pub fn club_summary(club: &Club, r: &ClubRef, info: Option<&crate::club::ClubInfo>) -> ClubSummary {
    let versions = info.map(|i| i.versions.as_slice()).unwrap_or_default();
    let i = versions.iter().position(|v| v.id == r.version);
    let label = match i {
        Some(0) if versions[0].name.is_empty() => club.name.clone(),
        Some(i) => format!("{} ({})", club.name, versions[i].label),
        None => format!("{} (missing version)", club.name),
    };
    ClubSummary {
        id: r.id.clone(),
        version: r.version.clone(),
        label,
        name: club.name.clone(),
        newer_version: versions.first().filter(|v| v.id != r.version).map(|v| v.id.clone()),
        set_minutes: club.set.minutes.as_ref().and_then(|m| m.max.or(m.min)),
        announcements: club.announcements.clone(),
        styles: club.styles.iter().map(|(id, s)| style_def(id, s)).collect(),
        tags: club
            .tags
            .iter()
            .map(|(k, t)| TagShown { key: k.clone(), name: t.name.clone(), color: t.color.clone().unwrap_or_else(|| NEUTRAL.into()) })
            .collect(),
    }
}

// ---------------------------------------------------------------- deck

/// What a deck shows about its song: worked out when it loads.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckSong {
    pub title: String,
    pub artist: String,
    pub version_label: String,
    /// A custom label, or the styles it shows (short), for the artwork's place.
    pub label: String,
    pub styles: Vec<Shown>,
    pub color: String,
    /// Its tempo before the tempo slider (in its style's counting).
    pub base_bpm: Option<f64>,
    /// The club style it's danced as (tempo ranges), if any.
    pub style: Option<StyleDef>,
    pub hot_cues: Vec<HotCue>,
    /// It has a file here (stems can be made).
    pub has_file: bool,
    /// The club's (or your) allowed slowdown (percent).
    pub slower_pct: f64,
}

pub fn deck_song(ctx: &ViewContext, track: &Track, version: &Version, entry: Option<&SetlistEntry>) -> DeckSong {
    let rec = track.recording_of(&version.id);
    let shown = shown_for(ctx, track, version, entry);
    let style = club_style_of(&shown).cloned();
    let pb = playback(rec, version, entry, style.as_ref());
    let base_bpm = rec.and_then(|r| r.rec.bpm).map(|b| match pb.target {
        Some(t) if resolve_settings(Some(version), entry).target_bpm.is_some() => fold_bpm(b, t),
        _ => pb.base.unwrap_or(b),
    });
    let custom = custom_label(Some(version), entry);
    DeckSong {
        title: track.rec.title.clone(),
        artist: track.rec.artist.clone(),
        version_label: version_label(version, track),
        label: custom.unwrap_or_else(|| shown.first().map(|s| s.short.clone()).unwrap_or_default()),
        color: shown.first().map_or_else(|| NEUTRAL.to_string(), |s| s.color.clone()),
        styles: shown,
        base_bpm,
        style,
        hot_cues: rec.map(|r| r.rec.hot_cues.clone()).unwrap_or_default(),
        has_file: rec.is_some_and(|r| !r.files.is_empty()),
        slower_pct: ctx.tempo_change.slower,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(name: &str, meter: u8, tempo: [f64; 2]) -> Style {
        Style {
            name: name.into(),
            short: Some(name[..3].to_uppercase()),
            color: Some("#123456".into()),
            meter: vec![meter],
            tempo: Some(Range { min: Some(tempo[0]), max: Some(tempo[1]) }),
            ..Default::default()
        }
    }
    fn club() -> Club {
        let mut c = Club { name: "Club".into(), ..Default::default() };
        c.set.tempo_change = Some(TempoChange { faster: 5.0, slower: 5.0 });
        c.styles.insert("rotary".into(), style("Rotary Waltz", 3, [135.0, 160.0]));
        c.styles.insert("cross-step".into(), style("Cross-Step Waltz", 3, [105.0, 125.0]));
        c.styles.insert("polka".into(), style("Polka", 2, [105.0, 130.0]));
        c
    }
    fn lib() -> StyleLibrary {
        let s = |key: &str, name: &str, short: &str| LibraryStyle { key: key.into(), name: name.into(), short: short.into(), color: "#111111".into(), ..Default::default() };
        let mut lib = StyleLibrary { styles: vec![s("waltz", "Waltz", "W"), s("kizomba", "Kizomba", "K"), s("my-polka", "polka", "")], ..Default::default() };
        lib.club_rules.insert(
            "club".into(),
            vec![ClubRule { mine: "waltz".into(), club: "rotary".into() }, ClubRule { mine: "waltz".into(), club: "cross-step".into() }],
        );
        lib
    }
    fn ctx() -> ViewContext {
        ViewContext { styles: lib(), trim_silence: true, fade_secs: 5.0, ..Default::default() }.with_club(Some(Arc::new(club())), Some("club".into()), 10.0)
    }
    fn track(bpm: f64, meter: u8, styles: &[&str]) -> Track {
        let version = Version { id: "v".into(), name: "Original".into(), recording: "r".into(), styles: styles.iter().map(|s| s.to_string()).collect(), ..serde_json::from_str(r#"{"id":"","name":""}"#).unwrap() };
        let rec = RecordingRecord {
            id: "r".into(),
            title: "Song".into(),
            duration_ms: 180_000,
            bpm: Some(bpm),
            beats_per_bar: Some(meter),
            meter_source: Some("detected".into()),
            analyzed: true,
            ..Default::default()
        };
        Track {
            rec: TrackRecord { id: "t".into(), title: "Song".into(), versions: vec![version], ..Default::default() },
            recordings: vec![Recording { rec, files: vec![], file_paths: Default::default(), missing: false, songs: vec!["t".into()] }],
            missing: false,
            play_count: 0,
            last_played_at: None,
        }
    }
    fn names(ctx: &ViewContext, t: &Track) -> Vec<String> {
        shown_for(ctx, t, &t.rec.versions[0], None).into_iter().map(|s| s.name).collect()
    }

    #[test]
    fn your_own_styles_without_a_club() {
        let c = ViewContext { styles: lib(), ..Default::default() };
        assert_eq!(names(&c, &track(145.0, 3, &["waltz"])), ["Waltz"]);
    }

    #[test]
    fn your_mapping_narrowed_by_tempo() {
        assert_eq!(names(&ctx(), &track(145.0, 3, &["waltz"])), ["Rotary Waltz"]);
        assert_eq!(names(&ctx(), &track(115.0, 3, &["waltz"])), ["Cross-Step Waltz"]);
        // Unmapped styles show every match.
        assert_eq!(names(&ctx(), &track(118.0, 4, &["kizomba"])), ["Polka"]);
        assert_eq!(names(&ctx(), &track(118.0, 4, &[])), ["Polka"]);
    }

    #[test]
    fn same_names_need_no_rule() {
        let c = ctx();
        assert_eq!(c.mapping["my-polka"], ["polka"]);
        assert_eq!(c.mapping["waltz"], ["rotary", "cross-step"]);
        assert!(!c.mapping.contains_key("kizomba"));
        for n in ["crossstep waltz", "Cross-Step Waltz", "cross step  waltz", "Cross–step waltz"] {
            assert!(same_name(n, "Cross-step Waltz"), "{n}");
        }
        assert!(same_name("Night Club Two-Step", "Nightclub Two Step"));
        assert!(same_name("Valsé", "valse"));
        assert!(!same_name("Rotary Waltz", "Waltz"));
    }

    #[test]
    fn a_needed_tempo_change_warns_in_a_set() {
        let c = ctx();
        let t = track(133.0, 3, &["waltz"]);
        let v = &t.rec.versions[0];
        // Rotary needs +1.5%; Cross-Step would need −6%, more than the club's 5%.
        let e = SetlistEntry::default();
        let in_set = shown_for(&c, &t, v, Some(&e));
        assert_eq!(in_set.len(), 1);
        assert_eq!((in_set[0].name.as_str(), in_set[0].fit, in_set[0].bpm), ("Rotary Waltz", Some(Fit::Stretch), Some(135.0)));
        assert_eq!(in_set[0].problems, ["plays at 133 BPM; Rotary Waltz is 135–160 BPM (+1.5% would fit)"]);
        // In the library, it's just a fit.
        assert!(shown_for(&c, &t, v, None)[0].problems.is_empty());
        // Given the tempo, it fits.
        let mut e = SetlistEntry::default();
        e.overrides.target_bpm = Some(135.0);
        assert!(shown_for(&c, &t, v, Some(&e))[0].problems.is_empty());
    }

    #[test]
    fn a_slot_shows_its_chosen_style() {
        let e = SetlistEntry { club_styles: Some(vec!["polka".into()]), ..Default::default() };
        let names: Vec<String> = shown_styles(&ctx(), None, None, Some(&e)).into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["Polka"]);
    }
}

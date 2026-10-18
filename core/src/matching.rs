//! Club guidelines applied to songs: which of a club's styles a song is, what
//! a song in a club's set shows, and whether a set follows the guidelines.
//!
//! A club defines its styles by meter, tempo and length (see `club.rs` and
//! examples/clubs/). A song *matches* a style when its meter fits and it can be
//! played within the style's tempo (directly, at half or double time, or by
//! changing its tempo as much as the club allows) at a fitting length.
//!
//! What a song in a club's set shows, in order of precedence:
//! 1. the club styles chosen for that entry (the DJ's choice: any style, even
//!    one the song doesn't match, with a warning);
//! 2. the styles the club suggests this very song for (its listed songs, found
//!    by fingerprint, Spotify track or file hash);
//! 3. the club styles it matches. When it matches several, your styles pick
//!    which of them show by default: those your styles on the song are called
//!    at the club (your rules in Settings → Styles & Tags, or the same name),
//!    or all of them if none is. This never adds a style the song doesn't
//!    match, and doesn't matter when only one matches.

use crate::app::fold_bpm;
use crate::club::{Club, Range, Style, TempoChange};
use serde::Serialize;
use std::collections::HashMap;

// ---------------------------------------------------------------- ranges

/// Within `r` (an open end always fits), with `slack` either side.
pub fn in_range(v: f64, r: Option<&Range>, slack: f64) -> bool {
    let Some(r) = r else { return true };
    r.min.map_or(true, |m| v >= m - slack) && r.max.map_or(true, |m| v <= m + slack)
}

pub fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// A number as people write it: `120`, `3.5`.
pub fn num(v: f64) -> String {
    format!("{}", round1(v))
}

/// "120", "28–34", "at least 3", "at most 4:30" (with `fmt` for the numbers).
pub fn range_text_with(r: Option<&Range>, fmt: &dyn Fn(f64) -> String) -> String {
    let Some(r) = r else { return String::new() };
    match (r.min, r.max) {
        (Some(a), Some(b)) if a == b => fmt(a),
        (Some(a), Some(b)) => format!("{}–{}", fmt(a), fmt(b)),
        (Some(a), None) => format!("at least {}", fmt(a)),
        (None, Some(b)) => format!("at most {}", fmt(b)),
        (None, None) => String::new(),
    }
}

pub fn range_text(r: Option<&Range>) -> String {
    range_text_with(r, &num)
}

fn mid(r: &Range) -> Option<f64> {
    match (r.min, r.max) {
        (Some(a), Some(b)) => Some((a + b) / 2.0),
        (a, b) => a.or(b),
    }
}

/// `m:ss`.
pub fn fmt_secs(s: f64) -> String {
    let t = s.round() as i64;
    format!("{}:{:02}", t / 60, t % 60)
}

fn pct(v: f64) -> String {
    format!("{}%", num(v))
}

/// `+3.5` or `−3.5` (a real minus sign).
fn signed(v: f64) -> String {
    format!("{}{}", if v > 0.0 { "+" } else { "−" }, num(v.abs()))
}

// ---------------------------------------------------------------- matching

/// What matching needs to know about a song.
#[derive(Debug, Clone, Default)]
pub struct SongFacts {
    /// Tempo as recorded (in the song's own counting).
    pub bpm: Option<f64>,
    /// Tempo the DJ set for it (then it isn't changed further).
    pub target: Option<f64>,
    /// Beats per bar, and whether it was detected (vs set by hand).
    pub meter: Option<u8>,
    pub meter_detected: bool,
    /// Playing length at its own tempo (seconds).
    pub length_secs: Option<f64>,
    /// Club styles that suggest this song (it's one of their listed songs).
    pub listed: Vec<String>,
}

/// How a song fits a style. `Listed`: a song the club suggests for it;
/// `Preferred`: in the preferred tempo; `Range`: in the allowed tempo;
/// `Stretch`: needs a tempo change (within what the club allows);
/// `Unchecked`: the style has no tempo, or the song's tempo is unknown.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Fit {
    Listed,
    Preferred,
    Range,
    Stretch,
    Unchecked,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Assessment {
    pub id: String,
    /// The song fits the style (possibly with a tempo change).
    pub ok: bool,
    pub fit: Fit,
    /// Playback rate needed (1 = as is).
    pub rate: f64,
    /// Tempo it would be played at, in the style's counting.
    pub bpm: Option<f64>,
    /// Why it doesn't fit (or what to watch for): shown as warnings.
    pub problems: Vec<String>,
}

/// The style says what fits it (meter, tempo or length); without, only its
/// suggested songs do (e.g. a choreography).
pub fn has_criteria(s: &Style) -> bool {
    !s.meter.is_empty() || s.tempo.is_some() || s.preferred.is_some() || s.length.is_some()
}

/// A meter's class: Orpheus knows a song as triple or duple (detected, or
/// set by you), not 2/4 from 4/4 (nor 3/4 from 6/8), so a style's meter is
/// compared by class.
fn meter_class(m: u8) -> u8 {
    match m {
        3 | 6 => 3,
        2 | 4 => 2,
        other => other,
    }
}

fn meter_fits(style: &Style, f: &SongFacts) -> Option<bool> {
    if style.meter.is_empty() {
        return Some(true);
    }
    let m = meter_class(f.meter?);
    Some(style.meter.iter().any(|&s| meter_class(s) == m))
}

fn meter_name(m: u8) -> String {
    match meter_class(m) {
        3 => "triple (3/4, 6/8)".into(),
        2 => "duple (2/4, 4/4)".into(),
        _ => format!("{m} beats a bar"),
    }
}

pub fn assess(id: &str, style: &Style, f: &SongFacts, tc: TempoChange) -> Assessment {
    let done = |ok, fit, rate, bpm, problems| Assessment { id: id.to_string(), ok, fit, rate, bpm, problems };
    // The club suggests this song for the style: it fits, whatever its tempo.
    if f.listed.iter().any(|l| l == id) {
        return done(true, Fit::Listed, 1.0, None, vec![]);
    }
    // A style danced to particular songs only (no meter, tempo or length to go by).
    if style.all_songs().next().is_some() && !has_criteria(style) {
        return done(false, Fit::Unchecked, 1.0, None, vec![format!("not one of the songs suggested for {}", style.name)]);
    }
    let mut problems = vec![];
    let (mut fit, mut rate, mut bpm) = (Fit::Unchecked, 1.0, None);

    if meter_fits(style, f) == Some(false) {
        let mut theirs: Vec<String> = style.meter.iter().map(|&m| meter_name(m)).collect();
        theirs.dedup();
        problems.push(format!("{} meter; {} is {}", meter_name(f.meter.unwrap_or_default()), style.name, theirs.join(" or ")));
    }

    let allowed = style.tempo.as_ref().or(style.preferred.as_ref());
    let own = f.target.or(f.bpm);
    if let (Some(allowed), Some(own)) = (allowed, own) {
        if let Some(center) = mid(allowed) {
            let base = fold_bpm(own, center);
            bpm = Some(round1(base));
            if style.preferred.is_some() && in_range(base, style.preferred.as_ref(), 0.5) {
                fit = Fit::Preferred;
            } else if in_range(base, Some(allowed), 0.5) {
                fit = Fit::Range;
            } else {
                let edge = match allowed.min {
                    Some(m) if base < m => m,
                    _ => allowed.max.unwrap_or(base),
                };
                let need = edge / base;
                let change = (need - 1.0) * 100.0;
                let limit = if change > 0.0 { tc.faster } else { tc.slower };
                if f.target.is_none() && change.abs() <= limit + 0.05 {
                    fit = Fit::Stretch;
                    rate = need;
                    bpm = Some(round1(edge));
                } else if f.target.is_some() {
                    problems.push(format!("set to {} BPM; {} is {} BPM", num(base), style.name, range_text(Some(allowed))));
                } else {
                    problems.push(format!(
                        "{} BPM: needs {}% for {} ({}), more than the {}% allowed",
                        num(base),
                        signed(change),
                        style.name,
                        range_text(Some(allowed)),
                        num(limit)
                    ));
                }
            }
        }
    }

    if let (Some(len_range), Some(len)) = (style.length.as_ref(), f.length_secs) {
        let len = len / rate;
        if !in_range(len, Some(len_range), 1.0) {
            problems.push(format!("{} long; {}: {}", fmt_secs(len), style.name, range_text_with(Some(len_range), &fmt_secs)));
        }
    }
    done(problems.is_empty(), fit, rate, bpm, problems)
}

fn score(a: &Assessment, f: &SongFacts, style: &Style) -> f64 {
    let base = match a.fit {
        Fit::Listed => -1.0,
        Fit::Preferred => 0.0,
        Fit::Range => 1.0,
        Fit::Unchecked => 1.5,
        Fit::Stretch => 2.0,
    };
    let meter_unknown = if !style.meter.is_empty() && f.meter.is_none() { 0.5 } else { 0.0 };
    base + (a.rate - 1.0).abs() * 10.0 + meter_unknown
}

/// Club styles a song matches, best fit first: those that suggest it, and
/// those whose tempo it fits. Other styles without a tempo (e.g. a Mixer) match
/// only by your mapping or choice: they'd otherwise match every song.
pub fn matches(club: &Club, f: &SongFacts, tc: TempoChange) -> Vec<Assessment> {
    let mut out: Vec<(f64, Assessment)> = club
        .styles
        .iter()
        .filter(|(id, s)| f.listed.iter().any(|l| l == *id) || s.tempo.is_some() || s.preferred.is_some())
        .map(|(id, s)| {
            let a = assess(id, s, f, tc);
            (score(&a, f, s), a)
        })
        .filter(|(_, a)| a.ok)
        .collect();
    // Stable, like the original: ties keep the club file's order.
    out.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
    out.into_iter().map(|(_, a)| a).collect()
}

/// Why a song shows the club styles it does.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ShownSource {
    Chosen,
    Listed,
    Mapping,
    Matches,
    None,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClubShown {
    /// Club style ids to show, in order.
    pub ids: Vec<String>,
    /// How each shown style fits (problems = warnings).
    pub fits: HashMap<String, Assessment>,
    pub source: ShownSource,
}

fn unchecked(id: &str) -> Assessment {
    Assessment { id: id.to_string(), ok: true, fit: Fit::Unchecked, rate: 1.0, bpm: None, problems: vec![] }
}

/// The club styles a song (or a slot: no facts) shows.
/// - `chosen`: the entry's chosen club styles, if any;
/// - `my_keys`: your styles on the song;
/// - `mapping`: what your styles are called at this club (your style key → club style ids).
pub fn club_shown(
    club: &Club,
    f: Option<&SongFacts>,
    tc: TempoChange,
    chosen: Option<&[String]>,
    my_keys: &[String],
    mapping: &HashMap<String, Vec<String>>,
) -> ClubShown {
    let known = |ids: &[String]| -> Vec<String> { ids.iter().filter(|id| club.styles.contains_key(*id)).cloned().collect() };
    let fits_of = |ids: &[String]| -> HashMap<String, Assessment> {
        ids.iter().map(|id| (id.clone(), f.map_or_else(|| unchecked(id), |f| assess(id, &club.styles[id], f, tc)))).collect()
    };
    if let Some(chosen) = chosen.filter(|c| !c.is_empty()) {
        let ids = known(chosen);
        return ClubShown { fits: fits_of(&ids), ids, source: ShownSource::Chosen };
    }
    let Some(f) = f else { return ClubShown { ids: vec![], fits: HashMap::new(), source: ShownSource::None } };
    // The club lists this song for some styles: it is those (e.g. a line
    // dance's edit), even if its tempo would fit others too.
    let listed = known(&f.listed);
    if !listed.is_empty() {
        return ClubShown { fits: fits_of(&listed), ids: listed, source: ShownSource::Listed };
    }
    let all = matches(club, f, tc);
    let mut ticked: Vec<String> = vec![];
    for k in my_keys {
        for id in mapping.get(k).into_iter().flatten() {
            if club.styles.contains_key(id) && !ticked.contains(id) {
                ticked.push(id.clone());
            }
        }
    }
    let hit: Vec<Assessment> = if all.len() > 1 { all.iter().filter(|a| ticked.contains(&a.id)).cloned().collect() } else { vec![] };
    let source = if all.is_empty() { ShownSource::None } else if !hit.is_empty() { ShownSource::Mapping } else { ShownSource::Matches };
    let shown = if hit.is_empty() { all } else { hit };
    ClubShown { ids: shown.iter().map(|a| a.id.clone()).collect(), fits: shown.into_iter().map(|a| (a.id.clone(), a)).collect(), source }
}

// ---------------------------------------------------------------- set check

/// One song of a set, as the check sees it.
#[derive(Debug, Clone, Default)]
pub struct SetItem {
    pub entry_id: String,
    /// For messages ("Polka" / the song title).
    pub title: String,
    /// Club styles it shows.
    pub ids: Vec<String>,
    pub fits: HashMap<String, Assessment>,
    /// Playing length (seconds) at its playing tempo, if known.
    pub length_secs: Option<f64>,
    /// Pause before it (seconds).
    pub pause_secs: f64,
}

/// `Warn`: against the guidelines; `Note`: worth a look (e.g. not finished yet).
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    Warn,
    Note,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub level: Level,
    pub text: String,
    pub entries: Vec<String>,
}

/// A count the guidelines ask for, and how the set stands.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tally {
    pub key: String,
    pub name: String,
    pub have: f64,
    pub want: String,
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SetCheck {
    pub issues: Vec<Issue>,
    pub tallies: Vec<Tally>,
    pub songs: usize,
    pub minutes: f64,
}

/// Checks a set (songs in order; pauses and skipped songs left out) against a
/// club's guidelines. `complete`: the set is finished (too few is a warning,
/// not a note).
pub fn check_set(club: &Club, items: &[SetItem], complete: bool) -> SetCheck {
    let mut issues: Vec<Issue> = vec![];
    let mut tallies: Vec<Tally> = vec![];
    let n = items.len();
    let name = |id: &str| club.styles.get(id).map_or(id.to_string(), |s| s.name.clone());
    let where_ = |i: usize| if n > 1 { i as f64 / (n - 1) as f64 * 100.0 } else { 0.0 };
    let total_secs: f64 = items.iter().map(|it| it.length_secs.unwrap_or(0.0) + it.pause_secs).sum();
    let minutes = total_secs / 60.0;
    let warn = |text: String, entries: Vec<String>| Issue { level: Level::Warn, text, entries };

    // Songs the guidelines can't place, and fits the DJ overrode.
    for it in items {
        if it.ids.is_empty() {
            issues.push(warn(format!("{}: not one of {}'s styles", it.title, club.name), vec![it.entry_id.clone()]));
        }
        for id in &it.ids {
            for p in it.fits.get(id).map(|a| a.problems.as_slice()).unwrap_or_default() {
                issues.push(warn(format!("{}: {p}", it.title), vec![it.entry_id.clone()]));
            }
        }
    }

    let mut tally = |issues: &mut Vec<Issue>, key: String, label: String, have: f64, r: Option<&Range>, fmt: Option<&dyn Fn(f64) -> String>| {
        let Some(r) = r else { return };
        let ok = in_range(have, Some(r), 0.0);
        let want = match fmt {
            Some(f) => range_text_with(Some(r), f),
            None => range_text(Some(r)),
        };
        // Too few is only a note while the set may still grow.
        if !ok {
            let too_many = r.max.is_some_and(|m| have > m);
            let shown = fmt.map_or_else(|| num(have), |f| f(have));
            issues.push(Issue {
                level: if too_many || complete { Level::Warn } else { Level::Note },
                text: format!("{label}: {shown}, {} expects {want}", club.name),
                entries: vec![],
            });
        }
        tallies.push(Tally { key, name: label, have, want, ok });
    };
    tally(&mut issues, "songs".into(), "Songs".into(), n as f64, club.set.songs.as_ref(), None);
    tally(&mut issues, "minutes".into(), "Minutes".into(), minutes.round(), club.set.minutes.as_ref(), None);

    let shows = |it: &SetItem, ids: &[String]| it.ids.iter().any(|id| ids.contains(id));
    let mut counted = |issues: &mut Vec<Issue>, ids: &[String], label: &str, key: &str, count: Option<&Range>, share: Option<&Range>| {
        let hit = items.iter().filter(|it| shows(it, ids)).count();
        tally(issues, key.to_string(), label.to_string(), hit as f64, count, None);
        if share.is_some() && n > 0 {
            let p = (hit as f64 / n as f64 * 100.0).round();
            tally(issues, format!("{key}:share"), format!("{label} share"), p, share, Some(&pct));
        }
    };
    for (id, s) in &club.styles {
        counted(&mut issues, std::slice::from_ref(id), &s.name, &format!("style:{id}"), s.count.as_ref(), s.share.as_ref());
    }
    for (id, g) in &club.groups {
        counted(&mut issues, &g.styles, &g.name, &format!("group:{id}"), g.count.as_ref(), g.share.as_ref());
    }

    // Where in the set.
    let positions = |issues: &mut Vec<Issue>, ids: &[String], when: &[Range], label: &str| {
        if when.is_empty() {
            return;
        }
        for (i, it) in items.iter().enumerate() {
            if !shows(it, ids) {
                continue;
            }
            let p = where_(i);
            if !when.iter().any(|r| in_range(p, Some(r), 0.5)) {
                let allowed: Vec<String> = when.iter().map(|r| range_text_with(Some(r), &pct)).collect();
                issues.push(warn(
                    format!("{}: {label} at song {} ({}% through); {}: {}", it.title, i + 1, p.round(), club.name, allowed.join(" or ")),
                    vec![it.entry_id.clone()],
                ));
            }
        }
    };
    for (id, s) in &club.styles {
        positions(&mut issues, std::slice::from_ref(id), &s.when, &s.name);
    }
    for g in club.groups.values() {
        positions(&mut issues, &g.styles, &g.when, &g.name);
    }

    // First and last.
    if n > 0 {
        for (id, s) in &club.styles {
            if s.first && !items[0].ids.contains(id) {
                issues.push(warn(format!("The set should start with a {}", s.name), vec![items[0].entry_id.clone()]));
            }
            if s.last && !items[n - 1].ids.contains(id) {
                issues.push(Issue {
                    level: if complete { Level::Warn } else { Level::Note },
                    text: format!("The set should end with a {}", s.name),
                    entries: vec![items[n - 1].entry_id.clone()],
                });
            }
        }
    }

    // In a row.
    let runs = |issues: &mut Vec<Issue>, ids: &[String], max: Option<u32>, label: &str| {
        let Some(max) = max.filter(|&m| m > 0) else { return };
        let mut run: Vec<&SetItem> = vec![];
        let flush = |run: &mut Vec<&SetItem>, issues: &mut Vec<Issue>| {
            if run.len() > max as usize {
                issues.push(warn(format!("{} {label} in a row (at most {max})", run.len()), run.iter().map(|r| r.entry_id.clone()).collect()));
            }
            run.clear();
        };
        for it in items {
            if shows(it, ids) {
                run.push(it);
            } else {
                flush(&mut run, issues);
            }
        }
        flush(&mut run, issues);
    };
    for (id, s) in &club.styles {
        runs(&mut issues, std::slice::from_ref(id), s.max_in_a_row.or(club.set.max_in_a_row), &s.name);
    }
    for g in club.groups.values() {
        runs(&mut issues, &g.styles, g.max_in_a_row, &g.name);
    }

    // Rules for part of the set.
    for r in club.rules.values() {
        let part: Vec<&SetItem> = items
            .iter()
            .enumerate()
            .filter(|(i, _)| {
                r.songs.as_ref().map_or(true, |s| in_range((i + 1) as f64, Some(s), 0.0))
                    && (r.when.is_empty() || r.when.iter().any(|w| in_range(where_(*i), Some(w), 0.5)))
            })
            .map(|(_, it)| it)
            .collect();
        if let Some(len) = r.length.as_ref() {
            for it in &part {
                if let Some(l) = it.length_secs.filter(|&l| !in_range(l, Some(len), 1.0)) {
                    issues.push(warn(
                        format!("{}: {} long; {}: {}", it.title, fmt_secs(l), r.name, range_text_with(Some(len), &fmt_secs)),
                        vec![it.entry_id.clone()],
                    ));
                }
            }
        }
        if r.count.is_some() {
            let have = if r.styles.is_empty() { part.len() } else { part.iter().filter(|it| shows(it, &r.styles)).count() };
            let label = if r.styles.is_empty() {
                r.name.clone()
            } else {
                let names: Vec<String> = r.styles.iter().map(|s| name(s)).collect();
                format!("{} in {}", names.join(" / "), r.name.to_lowercase())
            };
            tally(&mut issues, format!("rule:{}", r.name), label, have as f64, r.count.as_ref(), None);
        }
    }
    SetCheck { issues, tallies, songs: n, minutes }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::club::Rule;

    fn r(min: Option<f64>, max: Option<f64>) -> Option<Range> {
        Some(Range { min, max })
    }
    fn rr(min: f64, max: f64) -> Option<Range> {
        r(Some(min), Some(max))
    }

    /// A small club like the Dancebreak example.
    fn club() -> Club {
        let mut c = Club { name: "Club".into(), ..Default::default() };
        c.set.songs = rr(4.0, 6.0);
        c.set.tempo_change = Some(TempoChange { faster: 10.0, slower: 8.0 });
        c.set.max_in_a_row = Some(2);
        let s = |name: &str| Style { name: name.into(), ..Default::default() };
        c.styles.insert("cross-step".into(), Style { meter: vec![3], tempo: rr(105.0, 125.0), preferred: rr(110.0, 120.0), count: r(Some(2.0), None), ..s("Cross-Step Waltz") });
        c.styles.insert("rotary".into(), Style { meter: vec![3], tempo: rr(135.0, 160.0), preferred: rr(140.0, 155.0), last: true, ..s("Rotary Waltz") });
        c.styles.insert("polka".into(), Style { meter: vec![2], tempo: rr(105.0, 130.0), when: vec![Range { min: Some(25.0), max: Some(75.0) }], ..s("Polka") });
        c.styles.insert("lindy".into(), Style { meter: vec![4], tempo: rr(125.0, 160.0), length: r(None, Some(210.0)), ..s("Lindy Hop") });
        c.styles.insert("mixer".into(), s("Mixer"));
        c
    }
    fn tc() -> TempoChange {
        club().set.tempo_change.unwrap()
    }
    fn song(bpm: f64, meter: Option<u8>) -> SongFacts {
        SongFacts { bpm: Some(bpm), meter, meter_detected: true, length_secs: Some(180.0), ..Default::default() }
    }
    fn ids(v: &[Assessment]) -> Vec<&str> {
        v.iter().map(|a| a.id.as_str()).collect()
    }

    #[test]
    fn matches_by_meter_and_tempo_best_first() {
        let c = club();
        assert_eq!(ids(&matches(&c, &song(115.0, Some(3)), tc())), ["cross-step"]);
        assert_eq!(matches(&c, &song(148.0, Some(3)), tc())[0].id, "rotary");
        // Duple songs aren't waltzes; Lindy Hop needs +8.7 % (allowed), so it
        // comes last. A style without a tempo (Mixer) isn't a match by itself.
        assert_eq!(ids(&matches(&c, &song(115.0, Some(4)), tc())), ["polka", "lindy"]);
    }

    #[test]
    fn counts_half_and_double_time() {
        assert_eq!(matches(&club(), &song(74.0, Some(3)), tc())[0].id, "rotary");
    }

    #[test]
    fn tempo_changes_within_what_the_club_allows() {
        let c = club();
        let a = assess("rotary", &c.styles["rotary"], &song(128.0, Some(3)), tc());
        assert!(a.ok);
        assert_eq!(a.fit, Fit::Stretch);
        assert!((a.rate - 135.0 / 128.0).abs() < 1e-3);
        let far = assess("rotary", &c.styles["rotary"], &song(122.0, Some(3)), tc());
        assert!(!far.ok);
        assert!(far.problems[0].contains("needs +10.7%"), "{}", far.problems[0]);
        // Slowing down is limited separately (8 %).
        assert!(assess("rotary", &c.styles["rotary"], &song(172.0, Some(3)), tc()).ok);
        assert!(!assess("rotary", &c.styles["rotary"], &song(176.0, Some(3)), tc()).ok);
    }

    #[test]
    fn a_set_tempo_isnt_changed_further() {
        let c = club();
        let at = |t| SongFacts { target: Some(t), ..song(128.0, Some(3)) };
        assert!(!assess("rotary", &c.styles["rotary"], &at(128.0), tc()).ok);
        assert_eq!(assess("rotary", &c.styles["rotary"], &at(145.0), tc()).fit, Fit::Preferred);
    }

    #[test]
    fn length_at_the_playing_tempo() {
        let c = club();
        let long = SongFacts { length_secs: Some(240.0), ..song(140.0, Some(4)) };
        assert!(assess("lindy", &c.styles["lindy"], &long, tc()).problems[0].contains("4:00 long"));
        let ok = SongFacts { length_secs: Some(200.0), ..song(140.0, Some(4)) };
        assert!(assess("lindy", &c.styles["lindy"], &ok, tc()).ok);
    }

    #[test]
    fn meters_are_compared_as_duple_or_triple() {
        let c = club();
        // A 2/4 style takes duple songs, detected or set by you (stored as 4).
        assert!(assess("polka", &c.styles["polka"], &song(115.0, Some(4)), tc()).ok);
        let by_hand = SongFacts { meter_detected: false, ..song(115.0, Some(4)) };
        assert!(assess("polka", &c.styles["polka"], &by_hand, tc()).ok);
        // Triple isn't duple.
        let waltz = assess("polka", &c.styles["polka"], &song(115.0, Some(3)), tc());
        assert!(!waltz.ok && waltz.problems[0].contains("triple"), "{:?}", waltz.problems);
        assert!(!assess("lindy", &c.styles["lindy"], &SongFacts { length_secs: Some(200.0), ..song(140.0, Some(6)) }, tc()).ok);
        // Unknown meter: not ruled out.
        assert!(assess("polka", &c.styles["polka"], &song(115.0, None), tc()).ok);
    }

    fn with_songs() -> Club {
        let mut c = club();
        let bnp = crate::club::ClubSong { title: Some("BNP".into()), hash: Some("a0".into()), ..Default::default() };
        // A choreography: no meter, tempo or length; only its suggested songs.
        c.styles.insert("bnp".into(), Style { name: "Bohemian National Polka".into(), songs: vec![bnp], ..Default::default() });
        let slow = crate::club::ClubSong { title: Some("Slow polka".into()), hash: Some("a1".into()), ..Default::default() };
        c.styles.get_mut("polka").unwrap().songs = vec![slow];
        c
    }
    fn listed(s: SongFacts, l: &[&str]) -> SongFacts {
        SongFacts { listed: l.iter().map(|x| x.to_string()).collect(), ..s }
    }

    #[test]
    fn a_choreography_matches_only_its_songs() {
        let c = with_songs();
        assert!(!ids(&matches(&c, &song(118.0, Some(4)), tc())).contains(&"bnp"));
        assert_eq!(matches(&c, &listed(song(118.0, Some(4)), &["bnp"]), tc())[0].id, "bnp");
        assert!(assess("bnp", &c.styles["bnp"], &song(118.0, Some(4)), tc()).problems[0].contains("not one of the songs suggested"));
    }

    #[test]
    fn a_listed_song_shows_only_the_styles_listing_it() {
        let c = with_songs();
        let none = HashMap::new();
        let s = club_shown(&c, Some(&listed(song(118.0, Some(4)), &["bnp"])), tc(), None, &[], &none);
        assert_eq!((s.ids, s.source), (vec!["bnp".to_string()], ShownSource::Listed));
        // The DJ's choice still wins.
        let chosen = ["polka".to_string()];
        assert_eq!(club_shown(&c, Some(&listed(song(118.0, Some(4)), &["bnp"])), tc(), Some(&chosen), &[], &none).ids, ["polka"]);
    }

    #[test]
    fn suggested_songs_fit_whatever_their_tempo() {
        let c = with_songs();
        let slow = listed(SongFacts { meter_detected: false, ..song(90.0, Some(4)) }, &["polka"]);
        let a = assess("polka", &c.styles["polka"], &slow, tc());
        assert!(a.ok && a.fit == Fit::Listed);
        assert!(ids(&matches(&c, &song(115.0, Some(4)), tc())).contains(&"polka"));
    }

    fn mapping() -> HashMap<String, Vec<String>> {
        HashMap::from([
            ("waltz".to_string(), vec!["cross-step".to_string(), "rotary".to_string()]),
            ("my-polka".to_string(), vec!["polka".to_string()]),
        ])
    }
    fn shown(f: SongFacts, chosen: Option<&[String]>, mine: &str) -> ClubShown {
        club_shown(&club(), Some(&f), tc(), chosen, &[mine.to_string()], &mapping())
    }

    #[test]
    fn your_mapping_picks_among_matches() {
        assert_eq!(shown(song(115.0, Some(3)), None, "waltz").ids, ["cross-step"]);
        assert_eq!(shown(song(148.0, Some(3)), None, "waltz").ids, ["rotary"]);
        // Never adds a style the song doesn't match.
        assert!(shown(SongFacts { meter_detected: false, ..song(95.0, Some(4)) }, None, "my-polka").ids.is_empty());
        // Picks among several matches, and doesn't matter for one.
        assert_eq!(shown(song(115.0, Some(4)), None, "my-polka").ids, ["polka"]);
        assert_eq!(shown(song(115.0, Some(4)), None, "kizomba").ids, ["polka", "lindy"]);
        assert_eq!(shown(song(148.0, Some(3)), None, "my-polka").ids, ["rotary"]);
        // Without a mapping, every match.
        assert_eq!(shown(song(115.0, Some(3)), None, "kizomba").ids, ["cross-step"]);
    }

    #[test]
    fn the_djs_choice_wins() {
        let chosen = ["rotary".to_string()];
        let s = shown(song(115.0, Some(3)), Some(&chosen), "waltz");
        assert_eq!(s.source, ShownSource::Chosen);
        assert_eq!(s.ids, ["rotary"]);
        assert!(!s.fits["rotary"].ok);
    }

    fn item(title: &str, ids: &[&str], length: f64) -> SetItem {
        SetItem {
            entry_id: title.into(),
            title: title.into(),
            ids: ids.iter().map(|x| x.to_string()).collect(),
            length_secs: Some(length),
            pause_secs: 8.0,
            ..Default::default()
        }
    }

    #[test]
    fn set_check_counts_positions_runs_and_last() {
        let c = club();
        let items = [item("a", &["polka"], 180.0), item("b", &["cross-step"], 180.0), item("c", &["mixer"], 180.0), item("d", &["mixer"], 180.0), item("e", &["mixer"], 180.0)];
        let check = check_set(&c, &items, false);
        let text: Vec<&str> = check.issues.iter().map(|i| i.text.as_str()).collect();
        assert!(text.iter().any(|t| t.starts_with("a: Polka at song 1 (0% through)")), "{text:?}");
        assert!(text.contains(&"3 Mixer in a row (at most 2)"));
        assert!(text.contains(&"The set should end with a Rotary Waltz"));
        // One cross-step of the two expected: a note while the set isn't finished.
        assert_eq!(check.issues.iter().find(|i| i.text.starts_with("Cross-Step Waltz")).unwrap().level, Level::Note);
        let done = check_set(&c, &[item("x", &["cross-step"], 180.0)], true);
        assert_eq!(done.issues.iter().find(|i| i.text.starts_with("Cross-Step")).unwrap().level, Level::Warn);
        let songs = check.tallies.iter().find(|t| t.key == "songs").unwrap();
        assert!(songs.have == 5.0 && songs.ok);
    }

    #[test]
    fn rules_for_part_of_the_set() {
        let mut c = club();
        c.rules.insert("early".into(), Rule { name: "First 2 songs".into(), songs: rr(1.0, 2.0), length: r(None, Some(200.0)), ..Default::default() });
        let check = check_set(&c, &[item("long", &["mixer"], 240.0), item("ok", &["mixer"], 180.0), item("late-long", &["rotary"], 240.0)], false);
        let text: Vec<&str> = check.issues.iter().map(|i| i.text.as_str()).collect();
        assert!(text.contains(&"long: 4:00 long; First 2 songs: at most 3:20"), "{text:?}");
        assert!(!text.iter().any(|t| t.starts_with("late-long: 4:00")));
    }

    #[test]
    fn songs_without_a_club_style_are_flagged() {
        assert_eq!(check_set(&club(), &[item("odd", &[], 180.0)], false).issues[0].text, "odd: not one of Club's styles");
    }
}

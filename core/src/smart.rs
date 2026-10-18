//! Smart crates: rule-based collections evaluated against the library, and
//! the rules' vocabulary (fields, operators, what each value is) for editors.

use crate::matching::{assess, SongFacts};
use crate::model::*;
use crate::views::{self, bpm_for_style, playback, played_duration_ms, style_def, tempo_fit, StyleDef, TempoFit, ViewContext};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Match {
    #[default]
    All,
    Any,
}

/// A condition's value: a style key, number, `[min, max]`, text, source
/// kind, crate id or tag key, or several style or tag keys, depending on
/// the operator.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Value {
    Range([f64; 2]),
    Number(f64),
    Text(String),
    List(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Condition {
    pub field: String,
    pub op: String,
    #[serde(default)]
    pub value: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct Rules {
    #[serde(rename = "match")]
    pub match_: Match,
    pub conditions: Vec<Condition>,
}

impl Value {
    fn text(&self) -> String {
        match self {
            Value::Text(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Range([a, b]) => format!("{a}–{b}"),
            Value::List(l) => l.join(", "),
        }
    }
    /// Its keys (one, or a list).
    fn keys(&self) -> Vec<String> {
        match self {
            Value::List(l) => l.clone(),
            other => vec![other.text()],
        }
    }
    fn number(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------- vocabulary

/// How an operator's value is edited.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Input {
    None,
    Style,
    Number,
    Range,
    Text,
    Source,
    Crate,
    Tag,
    Days,
    /// Several of your styles, or tags.
    Styles,
    Tags,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpDef {
    pub op: &'static str,
    pub label: &'static str,
    pub input: Input,
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldDef {
    pub field: &'static str,
    pub label: &'static str,
    pub ops: Vec<OpDef>,
}

const fn op(op: &'static str, label: &'static str, input: Input) -> OpDef {
    OpDef { op, label, input }
}

/// The fields rules can test, in the editor's order.
pub fn fields() -> Vec<FieldDef> {
    use Input::*;
    let f = |field, label, ops| FieldDef { field, label, ops };
    vec![
        f(
            "style",
            "Dance style",
            vec![op("is", "is", Style), op("isAny", "is any of", Styles), op("isNot", "is not", Style), op("isSet", "is set", None), op("isEmpty", "is not set", None)],
        ),
        f(
            "bpm",
            "Tempo",
            vec![
                op("fitsStyle", "fits the range of", Style),
                op("between", "is between (BPM)", Range),
                op("atLeast", "is at least (BPM)", Number),
                op("atMost", "is at most (BPM)", Number),
                op("isEmpty", "is unknown", None),
            ],
        ),
        f(
            "tempoFit",
            "Tempo for its own style",
            vec![op("sweet", "is in the sweet spot", None), op("ok", "is acceptable", None), op("outside", "is outside the range", None)],
        ),
        f(
            "meter",
            "Meter",
            vec![op("triple", "is triple (3/4, 6/8)", None), op("duple", "is duple (2/4, 4/4)", None), op("isEmpty", "is unknown", None)],
        ),
        f("duration", "Length", vec![op("atMost", "is at most (seconds)", Number), op("atLeast", "is at least (seconds)", Number)]),
        f("text", "Title, artist, album or notes", vec![op("contains", "contains", Text), op("notContains", "does not contain", Text)]),
        f("source", "Source", vec![op("is", "is", Source), op("isNot", "is not", Source)]),
        f("plays", "Times played", vec![op("atLeast", "is at least", Number), op("atMost", "is at most", Number)]),
        f(
            "lastPlayed",
            "Last played",
            vec![op("withinDays", "in the last (days)", Days), op("notWithinDays", "not in the last (days)", Days), op("never", "never", None)],
        ),
        f("added", "Added", vec![op("withinDays", "in the last (days)", Days)]),
        f("crate", "Crate", vec![op("in", "contains it", Crate), op("notIn", "doesn't contain it", Crate)]),
        f("tag", "Tag", vec![op("has", "includes", Tag), op("hasAny", "includes any of", Tags), op("lacks", "doesn't include", Tag)]),
    ]
}

/// A new condition's defaults for a field: its first operator, with a value
/// to start from.
pub fn default_condition(field: &str) -> Condition {
    let all = fields();
    let def = all.iter().find(|d| d.field == field).unwrap_or(&all[0]);
    let first = &def.ops[0];
    let value = match first.input {
        Input::Range => Some(Value::Range(DEFAULT_RANGE)),
        Input::Number | Input::Days => Some(Value::Number(if def.field == "duration" { DEFAULT_LENGTH_SECS } else { DEFAULT_COUNT })),
        Input::Source => Some(Value::Text("file".into())),
        Input::Styles | Input::Tags => Some(Value::List(vec![])),
        _ => None,
    };
    Condition { field: def.field.to_string(), op: first.op.to_string(), value }
}

/// A new tempo range condition starts here (BPM).
const DEFAULT_RANGE: [f64; 2] = [100.0, 120.0];
/// A new length condition (seconds).
const DEFAULT_LENGTH_SECS: f64 = 210.0;
/// A new count or days condition.
const DEFAULT_COUNT: f64 = 30.0;

/// Short description of a condition ("Tempo is between (BPM) 100–120").
pub fn describe(c: &Condition, styles: &StyleLibrary, crates: &[Crate]) -> String {
    let all = fields();
    let Some(f) = all.iter().find(|d| d.field == c.field) else { return c.field.clone() };
    let op = f.ops.iter().find(|o| o.op == c.op);
    let val = match (op.map(|o| o.input), &c.value) {
        (Some(Input::None), _) | (_, None) => String::new(),
        (Some(Input::Style), Some(v)) => views::style_name(styles, &v.text()),
        (Some(Input::Styles), Some(v)) => v.keys().iter().map(|k| views::style_name(styles, k)).collect::<Vec<_>>().join(" or "),
        (Some(Input::Crate), Some(v)) => crates.iter().find(|x| x.id == v.text()).map_or("?".into(), |x| x.name.clone()),
        (_, Some(v)) => v.text(),
    };
    let label = op.map_or(c.op.as_str(), |o| o.label);
    if val.is_empty() {
        format!("{} {label}", f.label)
    } else {
        format!("{} {label} {val}", f.label)
    }
}

// ---------------------------------------------------------------- evaluation

/// One version of a song, as rules see it.
pub struct Candidate<'a> {
    pub track: &'a Track,
    pub version: &'a Version,
}

/// Rules nested in rules (a smart crate testing another) stop here.
const MAX_DEPTH: usize = 4;
const DAY_MS: i64 = 86_400_000;

pub struct Evaluator<'a> {
    pub ctx: &'a ViewContext,
    pub crates: &'a [Crate],
    /// The whole library (a rule may test another smart crate).
    pub library: &'a [Candidate<'a>],
    pub now: i64,
}

impl Evaluator<'_> {
    /// Members of a crate (hand-picked or smart), as version ids.
    pub fn members(&self, c: &Crate, depth: usize) -> HashSet<Id> {
        match c.smart.as_ref().and_then(|s| serde_json::from_value::<Rules>(s.clone()).ok()) {
            Some(rules) if depth < MAX_DEPTH => self.library.iter().filter(|r| self.matches(&rules, r, depth + 1)).map(|r| r.version.id.clone()).collect(),
            Some(_) => HashSet::new(),
            None => c.version_ids.iter().cloned().collect(),
        }
    }

    /// Whether a version meets the rules. No conditions: no songs.
    pub fn matches(&self, rules: &Rules, r: &Candidate, depth: usize) -> bool {
        if rules.conditions.is_empty() {
            return false;
        }
        match rules.match_ {
            Match::All => rules.conditions.iter().all(|c| self.test(c, r, depth)),
            Match::Any => rules.conditions.iter().any(|c| self.test(c, r, depth)),
        }
    }

    fn test(&self, cond: &Condition, row: &Candidate, depth: usize) -> bool {
        let (t, v) = (row.track, row.version);
        let rec = t.recording_of(&v.id);
        let own = &v.styles;
        let ctx = self.ctx;
        let shown_club = || -> Option<StyleDef> { views::club_style_of(&views::shown_for(ctx, t, v, None)).cloned() };
        // The club style one of your styles maps to that fits `bpm` (else the first mapped).
        let club_of = |k: &str, bpm: Option<f64>| -> Option<StyleDef> {
            let club = ctx.club.as_deref()?;
            let ids: Vec<&String> = ctx.mapping.get(k)?.iter().filter(|id| club.styles.contains_key(*id)).collect();
            let facts = SongFacts { bpm, meter_detected: true, ..Default::default() };
            let id = ids.iter().find(|id| assess(id, &club.styles[**id], &facts, ctx.tempo_change).ok).or(ids.first())?;
            Some(style_def(id, &club.styles[*id]))
        };
        let value = cond.value.as_ref();
        let num = value.and_then(Value::number);
        let cmp = |x: Option<f64>| -> bool {
            match (cond.op.as_str(), x, num) {
                ("isEmpty", x, _) => x.is_none(),
                ("atLeast", Some(x), Some(n)) => x >= n,
                ("atMost", Some(x), Some(n)) => x <= n,
                _ => false,
            }
        };
        let club_style = shown_club();
        let pb = playback(rec, v, None, club_style.as_ref());
        let rec_bpm = rec.and_then(|r| r.rec.bpm);
        match cond.field.as_str() {
            "style" => match cond.op.as_str() {
                "is" => value.is_some_and(|x| own.contains(&x.text())),
                "isAny" => value.is_some_and(|x| x.keys().iter().any(|k| own.contains(k))),
                "isNot" => !value.is_some_and(|x| own.contains(&x.text())),
                "isSet" => !own.is_empty(),
                "isEmpty" => own.is_empty(),
                _ => false,
            },
            "bpm" => {
                let bpm = pb.target.or(rec_bpm);
                match (cond.op.as_str(), bpm, value) {
                    ("isEmpty", b, _) => b.is_none(),
                    (_, None, _) => false,
                    ("fitsStyle", b, Some(x)) => {
                        let st = club_of(&x.text(), b);
                        st.is_some_and(|st| matches!(tempo_fit(bpm_for_style(b, Some(&st)), Some(&st)), TempoFit::Sweet | TempoFit::Extended))
                    }
                    ("between", Some(b), Some(Value::Range([lo, hi]))) => b >= *lo && b <= *hi,
                    _ => cmp(bpm),
                }
            }
            "tempoFit" => {
                let st = club_style.as_ref();
                let fit = tempo_fit(pb.target.or_else(|| rec_bpm.and_then(|b| bpm_for_style(Some(b), st))), st);
                match cond.op.as_str() {
                    "sweet" => fit == TempoFit::Sweet,
                    "ok" => matches!(fit, TempoFit::Sweet | TempoFit::Extended),
                    "outside" => fit == TempoFit::Outside,
                    _ => false,
                }
            }
            "duration" => cmp(rec.map(|r| played_duration_ms(ctx, r, Some(v), None, pb.rate) / 1000.0)),
            "plays" => cmp(Some(t.play_count as f64)),
            "text" => {
                let q = value.map(|x| x.text()).unwrap_or_default().to_lowercase();
                let q = q.trim();
                if q.is_empty() {
                    return true;
                }
                let hay = format!("{} {} {} {} {} {} {}", t.rec.title, t.rec.artist, t.rec.album, t.rec.genre, t.rec.notes, v.name, v.comment).to_lowercase();
                let has = hay.contains(q);
                if cond.op == "notContains" {
                    !has
                } else {
                    has
                }
            }
            "source" => {
                let kind = value.map(|x| x.text()).unwrap_or_default();
                let has = rec.is_some_and(|r| r.rec.sources.iter().any(|s| s.kind == kind));
                if cond.op == "isNot" {
                    !has
                } else {
                    has
                }
            }
            "meter" => {
                let m = rec.and_then(|r| r.rec.beats_per_bar);
                match cond.op.as_str() {
                    "isEmpty" => m.is_none(),
                    "triple" => matches!(m, Some(3 | 6)),
                    "duple" => matches!(m, Some(2 | 4)),
                    _ => false,
                }
            }
            "lastPlayed" => {
                let last = t.last_played_at;
                if cond.op == "never" {
                    return last.is_none();
                }
                let days = num.unwrap_or(0.0);
                let recent = last.is_some_and(|l| (self.now - l) as f64 <= days * DAY_MS as f64);
                if cond.op == "notWithinDays" {
                    !recent
                } else {
                    recent
                }
            }
            "added" => (self.now - t.rec.added_at) as f64 <= num.unwrap_or(0.0) * DAY_MS as f64,
            "crate" => {
                let id = value.map(|x| x.text()).unwrap_or_default();
                let in_it = self.crates.iter().find(|c| c.id == id).is_some_and(|c| self.members(c, depth).contains(&v.id));
                if cond.op == "notIn" {
                    !in_it
                } else {
                    in_it
                }
            }
            "tag" => {
                if cond.op == "hasAny" {
                    return value.is_some_and(|x| x.keys().iter().any(|k| v.tags.contains(k)));
                }
                let has = value.is_some_and(|x| v.tags.contains(&x.text()));
                if cond.op == "lacks" {
                    !has
                } else {
                    has
                }
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::club::{Club, Range, Style, TempoChange};
    use std::sync::Arc;

    fn style(name: &str, tempo: [f64; 2], preferred: [f64; 2]) -> Style {
        let r = |x: [f64; 2]| Some(Range { min: Some(x[0]), max: Some(x[1]) });
        Style { name: name.into(), meter: vec![3], tempo: r(tempo), preferred: r(preferred), ..Default::default() }
    }

    /// Waltzes like the Dancebreak example's, and your styles mapped onto them.
    fn ctx() -> ViewContext {
        let mut club = Club { name: "Club".into(), ..Default::default() };
        club.styles.insert("rotary".into(), style("Rotary Waltz", [135.0, 160.0], [140.0, 155.0]));
        club.styles.insert("cross-step".into(), style("Cross-Step Waltz", [105.0, 125.0], [110.0, 120.0]));
        ViewContext {
            club: Some(Arc::new(club)),
            mapping: [("rotary-waltz", "rotary"), ("cross-step-waltz", "cross-step")].iter().map(|(a, b)| (a.to_string(), vec![b.to_string()])).collect(),
            tempo_change: TempoChange { faster: 10.0, slower: 8.0 },
            ..Default::default()
        }
    }

    struct Song {
        style: Option<&'static str>,
        bpm: f64,
        meter: Option<u8>,
        spotify: bool,
        duration_ms: i64,
        tags: Vec<String>,
    }

    fn track(n: usize, s: &Song) -> Track {
        let id = format!("t{n}");
        let source = if s.spotify { SourceRef { kind: "spotify".into(), key: "spotify:track:x".into() } } else { SourceRef { kind: "file".into(), key: id.clone() } };
        let rec = RecordingRecord {
            id: format!("r{n}"),
            title: id.clone(),
            duration_ms: s.duration_ms,
            bpm: Some(s.bpm),
            bpm_source: Some("user".into()),
            beats_per_bar: s.meter,
            analyzed: true,
            sources: vec![source],
            ..Default::default()
        };
        let version: Version = serde_json::from_value(serde_json::json!({
            "id": format!("v{n}"), "name": "Original", "recording": format!("r{n}"),
            "styles": s.style.into_iter().collect::<Vec<_>>(), "tags": s.tags,
        }))
        .unwrap();
        Track {
            rec: TrackRecord { id: id.clone(), title: id, versions: vec![version], ..Default::default() },
            recordings: vec![Recording { rec, files: if s.spotify { vec![] } else { vec!["/x".into()] }, file_paths: Default::default(), missing: false, songs: vec![] }],
            missing: false,
            play_count: 0,
            last_played_at: None,
        }
    }

    fn library() -> Vec<Track> {
        let song = |style, bpm| Song { style, bpm, meter: None, spotify: false, duration_ms: 180_000, tags: vec![] };
        // Rotary sweet 140–155 (ok 135–160); cross-step 110–120 (ok 105–125).
        let songs = [
            song(Some("rotary-waltz"), 150.0),
            song(Some("cross-step-waltz"), 116.0),
            song(None, 148.0),
            Song { meter: Some(3), tags: vec!["classic".into()], ..song(None, 112.0) },
            Song { spotify: true, duration_ms: 300_000, ..song(None, 96.0) },
        ];
        songs.iter().enumerate().map(|(i, s)| track(i + 1, s)).collect()
    }

    fn run(rules: serde_json::Value, crates: &[Crate]) -> Vec<String> {
        let tracks = library();
        let ctx = ctx();
        let lib: Vec<Candidate> = tracks.iter().map(|t| Candidate { track: t, version: &t.rec.versions[0] }).collect();
        let ev = Evaluator { ctx: &ctx, crates, library: &lib, now: 10 * DAY_MS };
        let rules: Rules = serde_json::from_value(rules).unwrap();
        lib.iter().filter(|c| ev.matches(&rules, c, 0)).map(|c| c.version.id.clone()).collect()
    }

    fn waltzes() -> Vec<Crate> {
        vec![Crate { id: "w".into(), name: "Waltzes".into(), position: 1, version_ids: vec!["v1".into(), "v2".into(), "v3".into(), "v4".into()], smart: None }]
    }

    fn one(field: &str, op: &str, value: serde_json::Value) -> serde_json::Value {
        serde_json::json!({ "match": "all", "conditions": [{ "field": field, "op": op, "value": value }] })
    }

    #[test]
    fn filters_by_style_label() {
        assert_eq!(run(one("style", "is", "rotary-waltz".into()), &[]), ["v1"]);
        assert_eq!(run(one("style", "isEmpty", serde_json::Value::Null), &[]).len(), 3);
    }

    #[test]
    fn a_mixed_waltz_crate_by_tempo() {
        let rules = |style: &str| {
            serde_json::json!({ "match": "all", "conditions": [
                { "field": "crate", "op": "in", "value": "w" },
                { "field": "bpm", "op": "fitsStyle", "value": style },
            ]})
        };
        assert_eq!(run(rules("rotary-waltz"), &waltzes()), ["v1", "v3"]);
        assert_eq!(run(rules("cross-step-waltz"), &waltzes()), ["v2", "v4"]);
    }

    #[test]
    fn numbers_text_sources_and_any() {
        assert_eq!(run(one("bpm", "between", serde_json::json!([110, 120])), &[]), ["v2", "v4"]);
        assert_eq!(run(one("duration", "atLeast", 240.into()), &[]), ["v5"]);
        assert_eq!(run(one("source", "is", "spotify".into()), &[]), ["v5"]);
        assert_eq!(run(one("tag", "has", "classic".into()), &[]), ["v4"]);
        assert_eq!(run(one("meter", "triple", serde_json::Value::Null), &[]), ["v4"]);
        let any = serde_json::json!({ "match": "any", "conditions": [
            { "field": "style", "op": "is", "value": "rotary-waltz" },
            { "field": "source", "op": "is", "value": "spotify" },
        ]});
        assert_eq!(run(any, &[]), ["v1", "v5"]);
        assert_eq!(run(one("lastPlayed", "never", serde_json::Value::Null), &[]).len(), 5);
        assert!(run(serde_json::json!({ "match": "all", "conditions": [] }), &[]).is_empty());
    }

    #[test]
    fn any_of_several_styles_or_tags() {
        assert_eq!(run(one("style", "isAny", serde_json::json!(["rotary-waltz", "cross-step-waltz"])), &[]), ["v1", "v2"]);
        assert_eq!(run(one("tag", "hasAny", serde_json::json!(["classic", "nope"])), &[]), ["v4"]);
        // The library's filters together: a tag, a meter and a tempo range all at once.
        let filters = serde_json::json!({ "match": "all", "conditions": [
            { "field": "tag", "op": "hasAny", "value": ["classic"] },
            { "field": "meter", "op": "triple" },
            { "field": "bpm", "op": "between", "value": [100, 115] },
        ]});
        assert_eq!(run(filters, &[]), ["v4"]);
        let lib = StyleLibrary::default();
        assert!(describe(&serde_json::from_value(serde_json::json!({ "field": "style", "op": "isAny", "value": ["a", "b"] })).unwrap(), &lib, &[]).contains(" or "));
    }

    #[test]
    fn smart_crates_referring_to_each_other_dont_loop() {
        let smart = |id: &str, other: &str| Crate {
            id: id.into(),
            name: id.into(),
            position: 2,
            version_ids: vec![],
            smart: Some(one("crate", "in", other.into())),
        };
        let crates = vec![smart("a", "b"), smart("b", "a")];
        assert!(run(one("crate", "in", "a".into()), &crates).is_empty());
    }
}

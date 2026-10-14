//! Types shared between the backend, the on-disk library and the UI.
//! All JSON is camelCase to match `src/lib/types.ts`.
//!
//! IDs are short random strings (not counters) so libraries edited on several
//! machines can be merged with git or a sync service without collisions.

use serde::{Deserialize, Serialize};

pub type Id = String;

pub fn new_id() -> Id {
    // 12 base-36 characters ≈ 62 bits of randomness.
    let mut n = u128::from_le_bytes(*uuid::Uuid::new_v4().as_bytes());
    let mut s = String::with_capacity(12);
    for _ in 0..12 {
        let d = (n % 36) as u8;
        s.push(if d < 10 { (b'0' + d) as char } else { (b'a' + d - 10) as char });
        n /= 36;
    }
    s
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn yes() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_zero_u32(v: &u32) -> bool {
    *v == 0
}

fn is_zero(f: &f64) -> bool {
    *f == 0.0
}

/// Where a track's audio can come from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct SourceRef {
    /// `file` (key = content hash of the audio) or `spotify` (key = track URI).
    pub kind: String,
    pub key: String,
}

/// Playback settings that a version defines and a setlist entry may override.
/// `None` = inherit (entry → version → track defaults).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Overrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_bpm: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cue_in_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cue_out_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fade_out_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gain_db: Option<f64>,
    /// Stem name → enabled (e.g. `{"vocals": false}`). Stored for future stem playback.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stems: Option<serde_json::Map<String, serde_json::Value>>,
}

/// A playable configuration of a track (style, tempo, cue points, ...).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub id: Id,
    pub name: String,
    /// Shown to dancers instead of the style names, e.g. the name of a
    /// particular line dance (a setlist entry's label overrides it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Your dance styles this version can be danced as (in order).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub styles: Vec<String>,
    /// Older libraries stored a single style; read and folded into `styles`.
    #[serde(default, rename = "style", skip_serializing)]
    pub legacy_style: Option<String>,
    #[serde(flatten)]
    pub settings: Overrides,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comment: String,
    /// Your tags (keys into `styles.json`'s `tags`), e.g. "familiar".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionPatch {
    pub name: Option<String>,
    #[serde(default, with = "double_option")]
    pub display_name: Option<Option<String>>,
    /// Replaces the version's styles.
    pub styles: Option<Vec<String>>,
    #[serde(default, with = "double_option")]
    pub target_bpm: Option<Option<f64>>,
    #[serde(default, with = "double_option")]
    pub cue_in_ms: Option<Option<i64>>,
    #[serde(default, with = "double_option")]
    pub cue_out_ms: Option<Option<i64>>,
    #[serde(default, with = "double_option")]
    pub fade_out_ms: Option<Option<i64>>,
    #[serde(default, with = "double_option")]
    pub gain_db: Option<Option<f64>>,
    #[serde(default, with = "double_option")]
    pub stems: Option<Option<serde_json::Map<String, serde_json::Value>>>,
    pub comment: Option<String>,
    pub tags: Option<Vec<String>>,
}

/// One of *your* dance styles: what versions in your library are tagged with.
/// Club profiles have their own styles; `StyleLibrary::mappings` connects them.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct LibraryStyle {
    /// Stable id stored on versions.
    pub key: String,
    pub name: String,
    pub short: String,
    pub color: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub note: String,
}

/// One rule of how your styles show up at a club: songs tagged `style` whose
/// planned tempo is within [`min_bpm`, `max_bpm`] count as club style `to`
/// (empty = not danced at this club). Rules are tried in order; the first match
/// wins. Resolution happens in the UI (`src/lib/styles.ts`).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct StyleMappingRule {
    pub style: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_bpm: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_bpm: Option<f64>,
    pub to: String,
}

/// One of your tags: a label on versions that isn't a dance style.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct LibraryTag {
    pub key: String,
    pub name: String,
    pub color: String,
}

/// `styles.json` in the library folder: your styles and tags, and for each club
/// profile id the rules mapping them onto the club's styles. Without a
/// matching rule, a club style with the same key or name is used.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct StyleLibrary {
    pub styles: Vec<LibraryStyle>,
    pub mappings: std::collections::BTreeMap<String, Vec<StyleMappingRule>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<LibraryTag>,
}

/// A saved cue point (hot cue) on a track; shared by all its versions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HotCue {
    /// 1-based pad number.
    pub slot: u8,
    pub ms: i64,
}

/// A track as stored in `tracks/<id>.json`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct TrackRecord {
    pub id: Id,
    pub title: String,
    pub artist: String,
    pub album: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub genre: String,
    pub duration_ms: i64,
    /// BPM used for playback: detected, from tags, or entered by the DJ.
    pub bpm: Option<f64>,
    /// `detected`, `tag`, `tap` or `user`.
    pub bpm_source: Option<String>,
    /// Last BPM found by analysis (kept so an edited BPM can be reset).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected_bpm: Option<f64>,
    /// BPM from the file's tags or Spotify metadata, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag_bpm: Option<f64>,
    /// Beats per bar used for the song (3 = triple, e.g. waltzes; 4 = duple).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub beats_per_bar: Option<u8>,
    /// `detected` or `user`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meter_source: Option<String>,
    /// Last meter found by analysis.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected_meter: Option<u8>,
    pub rating: Option<i64>,
    #[serde(skip_serializing_if = "is_zero")]
    pub gain_db: f64,
    pub loudness_db: Option<f64>,
    pub audio_start_ms: Option<i64>,
    pub audio_end_ms: Option<i64>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub notes: String,
    pub analyzed: bool,
    /// Version of the analysis that ran (see `analysis::VERSION`); older
    /// results are redone in the background.
    #[serde(skip_serializing_if = "is_zero_u32")]
    pub analysis_version: u32,
    /// `file`, `spotify`, or absent (prefer files).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artwork_url: Option<String>,
    pub added_at: i64,
    pub sources: Vec<SourceRef>,
    pub versions: Vec<Version>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hot_cues: Vec<HotCue>,
    /// IDs of tracks merged into this one (keeps history references valid).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<Id>,
    /// `false` for songs used only in setlists (e.g. a Spotify song added
    /// straight to a set). They are hidden from library views and removed once
    /// no setlist or crate uses them.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub in_library: bool,
}

/// A track as sent to the UI: the record plus machine-local/derived state.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    #[serde(flatten)]
    pub rec: TrackRecord,
    /// Known paths of this track's files on this machine.
    pub files: Vec<String>,
    /// No playable source (files missing and no Spotify link).
    pub missing: bool,
    pub play_count: i64,
    pub last_played_at: Option<i64>,
}

/// Partial update for a track. Missing fields are left alone; `null` clears.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackPatch {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    #[serde(default, with = "double_option")]
    pub bpm: Option<Option<f64>>,
    pub bpm_source: Option<String>,
    /// Beats per bar set by the DJ (`null` = use the detected meter).
    #[serde(default, with = "double_option")]
    pub beats_per_bar: Option<Option<u8>>,
    #[serde(default, with = "double_option")]
    pub rating: Option<Option<i64>>,
    pub gain_db: Option<f64>,
    pub notes: Option<String>,
    #[serde(default, with = "double_option")]
    pub preferred_source: Option<Option<String>>,
}

pub mod double_option {
    use serde::{Deserialize, Deserializer};
    pub fn deserialize<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
    where
        T: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        Deserialize::deserialize(de).map(Some)
    }
}

/// `crates/<id>.json`: a collection of songs (versions).
///
/// Crates are the DJ's own groupings ("Waltzes", "Dancebreak classics",
/// "Wedding requests") and may mix dance styles freely. Dance styles are not
/// crates: they are labels on versions that any view can be filtered by.
///
/// A *smart* crate has `smart` rules instead of a hand-picked list: its songs
/// are whatever in the library currently matches (rules are evaluated by the
/// UI, see `src/lib/smart.ts`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Crate {
    pub id: Id,
    pub name: String,
    pub position: i64,
    /// Hand-picked members (empty for smart crates).
    pub version_ids: Vec<Id>,
    /// Rules for a smart crate (`{ "match": "all" | "any", "conditions": [...] }`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smart: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SetlistEntry {
    pub id: Id,
    /// `None` = a style slot: the DJ picks a song live.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_id: Option<Id>,
    /// Styles to show for this entry, in order: a subset of the version's
    /// styles, or the slot's style(s). `None` = all of the version's styles.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub styles: Option<Vec<String>>,
    /// Custom text shown instead of style names (setlist, projector).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Older libraries: single slot / override style; folded into `styles`.
    #[serde(rename = "style", skip_serializing)]
    pub legacy_style: Option<String>,
    /// Per-setlist overrides of the version's settings.
    #[serde(skip_serializing_if = "is_default_overrides")]
    pub overrides: Overrides,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub note: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Auto mode stops before this entry: the DJ starts it with Play Next.
    #[serde(skip_serializing_if = "is_false")]
    pub hold: bool,
    /// `pending`, `playing`, `played`, `skipped`.
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub played_at: Option<i64>,
}

impl Default for SetlistEntry {
    fn default() -> Self {
        Self {
            id: String::new(),
            version_id: None,
            styles: None,
            label: None,
            legacy_style: None,
            overrides: Overrides::default(),
            note: String::new(),
            tags: vec![],
            hold: false,
            status: "pending".into(),
            played_at: None,
        }
    }
}

fn is_default_overrides(o: &Overrides) -> bool {
    *o == Overrides::default()
}

/// `setlists/<id>.json`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Setlist {
    pub id: Id,
    pub name: String,
    pub event_date: Option<String>,
    pub profile_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub notes: String,
    /// `live` or `preset`; affects defaults and what the projector shows.
    pub mode: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub entries: Vec<SetlistEntry>,
}

/// Setlist without entries, for lists.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetlistSummary {
    pub id: Id,
    pub name: String,
    pub event_date: Option<String>,
    pub profile_id: String,
    pub notes: String,
    pub mode: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub entry_count: usize,
}

impl From<&Setlist> for SetlistSummary {
    fn from(s: &Setlist) -> Self {
        Self {
            id: s.id.clone(),
            name: s.name.clone(),
            event_date: s.event_date.clone(),
            profile_id: s.profile_id.clone(),
            notes: s.notes.clone(),
            mode: s.mode.clone(),
            created_at: s.created_at,
            updated_at: s.updated_at,
            entry_count: s.entries.len(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetlistPatch {
    pub name: Option<String>,
    #[serde(default, with = "double_option")]
    pub event_date: Option<Option<String>>,
    pub profile_id: Option<String>,
    pub notes: Option<String>,
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewEntry {
    pub version_id: Option<Id>,
    pub styles: Option<Vec<String>>,
    pub label: Option<String>,
    #[serde(default)]
    pub overrides: Overrides,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryPatch {
    #[serde(default, with = "double_option")]
    pub version_id: Option<Option<Id>>,
    #[serde(default, with = "double_option")]
    pub styles: Option<Option<Vec<String>>>,
    #[serde(default, with = "double_option")]
    pub label: Option<Option<String>>,
    /// Replaces all overrides.
    pub overrides: Option<Overrides>,
    pub note: Option<String>,
    pub hold: Option<bool>,
    pub tags: Option<Vec<String>>,
    pub status: Option<String>,
}

/// A song played in a setlist (history is derived from setlists).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    pub played_at: i64,
    pub track_id: Id,
    pub version_id: Option<Id>,
    pub styles: Vec<String>,
    pub setlist_id: Id,
    pub entry_id: Id,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub channels: u16,
    pub is_default: bool,
}

impl Version {
    /// Folds the legacy single `style` field into `styles`.
    pub fn normalize(&mut self) {
        if let Some(s) = self.legacy_style.take() {
            if !self.styles.contains(&s) {
                self.styles.insert(0, s);
            }
        }
    }
}

impl SetlistEntry {
    pub fn normalize(&mut self) {
        if let Some(s) = self.legacy_style.take() {
            if self.styles.is_none() {
                self.styles = Some(vec![s]);
            }
        }
    }
}


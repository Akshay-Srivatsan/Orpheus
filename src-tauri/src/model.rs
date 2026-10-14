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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    #[serde(flatten)]
    pub settings: Overrides,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comment: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionPatch {
    pub name: Option<String>,
    #[serde(default, with = "double_option")]
    pub style: Option<Option<String>>,
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
    /// 0–10 (Powers' DJ tips).
    pub energy: Option<i64>,
    /// 0–10 rhythmic intensity: how clearly audible the beat is.
    pub rhythm: Option<i64>,
    pub rating: Option<i64>,
    #[serde(skip_serializing_if = "is_false")]
    pub familiar: bool,
    #[serde(skip_serializing_if = "is_zero")]
    pub gain_db: f64,
    pub loudness_db: Option<f64>,
    pub audio_start_ms: Option<i64>,
    pub audio_end_ms: Option<i64>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub notes: String,
    pub analyzed: bool,
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
    #[serde(default, with = "double_option")]
    pub energy: Option<Option<i64>>,
    #[serde(default, with = "double_option")]
    pub rhythm: Option<Option<i64>>,
    #[serde(default, with = "double_option")]
    pub rating: Option<Option<i64>>,
    pub familiar: Option<bool>,
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
    /// Slot style, or an override of the version's style.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    /// Per-setlist overrides of the version's settings.
    #[serde(skip_serializing_if = "is_default_overrides")]
    pub overrides: Overrides,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub note: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
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
            style: None,
            overrides: Overrides::default(),
            note: String::new(),
            tags: vec![],
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
    pub lesson_style: Option<String>,
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
    pub lesson_style: Option<String>,
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
            lesson_style: s.lesson_style.clone(),
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
    #[serde(default, with = "double_option")]
    pub lesson_style: Option<Option<String>>,
    pub notes: Option<String>,
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewEntry {
    pub version_id: Option<Id>,
    pub style: Option<String>,
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
    pub style: Option<Option<String>>,
    /// Replaces all overrides.
    pub overrides: Option<Overrides>,
    pub note: Option<String>,
    pub tags: Option<Vec<String>>,
    pub status: Option<String>,
}

/// One line of `history/<yyyy-mm>.jsonl`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    pub played_at: i64,
    pub track_id: Id,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_id: Option<Id>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bpm: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setlist_id: Option<Id>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub channels: u16,
    pub is_default: bool,
}

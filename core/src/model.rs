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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    #[default]
    Song,
    Pause,
}

impl EntryKind {
    fn is_song(&self) -> bool {
        *self == EntryKind::Song
    }
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

/// Where a recording's audio can come from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct SourceRef {
    /// `file` (key = content hash of the audio) or `spotify` (key = track URI).
    pub kind: String,
    pub key: String,
}

impl SourceRef {
    /// The id of a recording made from this source: content-addressed, so
    /// audio removed from the library and added again (the same file, or
    /// the same Spotify track) comes back under the id versions point to.
    pub fn recording_id(&self) -> Id {
        match self.kind.as_str() {
            "spotify" => format!("spotify-{}", self.key.rsplit(':').next().unwrap_or(&self.key)),
            _ => self.key.clone(),
        }
    }
}

/// Whether a chime plays before an announcement, and which (a version of a
/// song, played as it's set: start, end, fade, gain). Each `None`
/// inherits (entry → set → version → song → your settings; see `chime.rs`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct ChimeChoice {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// The version of a song that plays as the chime.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound: Option<Id>,
}

impl ChimeChoice {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
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

/// A playable version of a song: the recording it plays, and how (styles,
/// tempo, cue points, ...).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub id: Id,
    pub name: String,
    /// The recording it plays (empty in older files: the song's own).
    #[serde(default)]
    pub recording: Id,
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
    /// What auto mode says before it (may contain `{dance}`), unless a set
    /// entry says otherwise; empty = nothing; absent: the song's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub announcement: Option<String>,
    #[serde(default, skip_serializing_if = "ChimeChoice::is_default")]
    pub chime: ChimeChoice,
    /// Your tags (keys into `styles.json`'s `tags`), e.g. "familiar".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Removed from the library while a set still plays it: hidden from
    /// library views, deleted once no set does.
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden: bool,
    /// Left out of library lists by you (e.g. a chime, or an edit you rarely
    /// want): still in its song's Info, in sets and crates it's in, and shown
    /// with View → Show Hidden Versions.
    #[serde(default, skip_serializing_if = "is_false")]
    pub unlisted: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionPatch {
    pub name: Option<String>,
    /// Plays another recording.
    pub recording: Option<Id>,
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
    #[serde(default, with = "double_option")]
    pub announcement: Option<Option<String>>,
    pub chime: Option<ChimeChoice>,
    pub tags: Option<Vec<String>>,
    /// Shows it in the library again (`false`).
    pub hidden: Option<bool>,
    pub unlisted: Option<bool>,
}

/// One of *your* dance styles: what versions in your library are tagged with.
/// Club profiles have their own styles; `StyleLibrary::club_rules` connects them.
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

/// One of your tags: a label on versions that isn't a dance style.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct LibraryTag {
    pub key: String,
    pub name: String,
    pub color: String,
}

/// `styles.json` in the library folder: your styles and tags, and for each club
/// profile id the rules relating them to the club's styles. A club style with
/// the same name as one of yours needs no rule.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct StyleLibrary {
    pub styles: Vec<LibraryStyle>,
    /// Per club id: "what I call `mine`, the club may call `club`". When a
    /// song fits several of the club's styles, it shows those its styles lead
    /// to (`src/lib/clubs.ts`).
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub club_rules: std::collections::BTreeMap<String, Vec<ClubRule>>,
    /// The same as a table (your style key → club style ids), from older
    /// files: read, then saved as `club_rules` (see `upgrade`).
    #[serde(skip_serializing)]
    pub club_mappings: std::collections::BTreeMap<String, std::collections::BTreeMap<String, Vec<String>>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<LibraryTag>,
    /// How announcements say words (dance names etc.).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pronunciations: Vec<Pronunciation>,
}

/// "What I call `mine` (your style key), the club may call `club` (its style id)."
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(default)]
pub struct ClubRule {
    pub mine: String,
    pub club: String,
}

impl StyleLibrary {
    /// Turns an older file's table into rules.
    pub fn upgrade(&mut self) {
        for (club, rows) in std::mem::take(&mut self.club_mappings) {
            for (mine, ids) in rows {
                for id in ids {
                    self.add_rule(&club, ClubRule { mine: mine.clone(), club: id });
                }
            }
        }
    }

    /// Adds a rule unless it's already there.
    pub fn add_rule(&mut self, club: &str, rule: ClubRule) {
        let rules = self.club_rules.entry(club.to_string()).or_default();
        if !rules.contains(&rule) {
            rules.push(rule);
        }
    }
}

/// An announcement pronunciation: `word` (or phrase, whole words, any case)
/// is said as `say`: a respelling ("shot-eesh"), or phonemes between slashes
/// (Kokoro's phoneme symbols).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Pronunciation {
    pub word: String,
    pub say: String,
}

/// A saved cue point (hot cue) on a recording; shared by every version playing it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HotCue {
    /// 1-based pad number.
    pub slot: u8,
    pub ms: i64,
}

/// A recording: a piece of audio, as stored in `recordings/<id>.json`. Its
/// sources (files by content hash, Spotify tracks) are the same audio. Songs'
/// versions play recordings; several songs can use one (a song that works for
/// two line dances). One nothing plays any more leaves the library (its files
/// are listed in `excluded.json`).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct RecordingRecord {
    pub id: Id,
    /// From the file's tags or Spotify: the title etc. of a song made from it.
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
    /// Beats per bar (3 = triple, e.g. waltzes; 4 = duple).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub beats_per_bar: Option<u8>,
    /// `detected` or `user`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meter_source: Option<String>,
    /// Last meter found by analysis.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected_meter: Option<u8>,
    #[serde(skip_serializing_if = "is_zero")]
    pub gain_db: f64,
    /// Integrated loudness (LUFS, EBU R128) and true peak (dBTP), from analysis.
    pub loudness_db: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_db: Option<f64>,
    pub audio_start_ms: Option<i64>,
    pub audio_end_ms: Option<i64>,
    /// Chromaprint fingerprint (text form): identifies this recording in any
    /// audio format (see `fingerprint.rs`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hot_cues: Vec<HotCue>,
    /// IDs of recordings merged into this one (keeps references valid).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<Id>,
    /// Removed from the library while songs or sets still play it: a
    /// "ghost" that can't play, kept (fingerprint, length, BPM…) so the same
    /// audio can stand in for it again (the same file or Spotify track, or a
    /// file whose fingerprint matches: `Store::reconnect`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub removed: bool,
}

/// A recording on this machine whose audio is the same as a missing one's
/// (by fingerprint): it can stand in for it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reconnect {
    /// The missing recording (removed, or its file isn't here) that songs still play.
    pub missing: Id,
    /// The recording that sounds the same.
    pub found: Id,
    pub coverage: f64,
    pub error: f64,
}

/// `excluded.json` in the library folder: audio removed from the library.
/// Files (by content hash, so it works on every machine) aren't added again by
/// scanning the library folders; adding one explicitly (Library → Files) does.
/// Versions that still play removed audio keep pointing to it (they show as
/// missing); recording ids are content-addressed (`SourceRef::recording_id`),
/// so adding it again brings it back for them.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Excluded {
    pub files: Vec<ExcludedFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct ExcludedFile {
    pub hash: String,
    /// What it was called, for showing it.
    pub title: String,
    pub artist: String,
    pub removed_at: i64,
}

/// An audio file in the library folders (Library → Files).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryFile {
    pub path: String,
    /// The library folder it's in (absent: added from elsewhere).
    pub root: Option<String>,
    pub hash: String,
    /// The recording it is, if it's in the library.
    pub recording: Option<Id>,
    /// Songs playing it (removed: still pointing to it).
    pub songs: Vec<Id>,
    /// Removed from the library (scanning skips it).
    pub removed: bool,
}

/// A recording as sent to the UI: the record plus machine-local/derived state.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recording {
    #[serde(flatten)]
    pub rec: RecordingRecord,
    /// Known paths of this recording's files on this machine.
    pub files: Vec<String>,
    /// The same, for each file (by content hash).
    pub file_paths: std::collections::BTreeMap<String, Vec<String>>,
    /// No playable source (files missing and no Spotify link).
    pub missing: bool,
    /// Songs whose versions play it.
    pub songs: Vec<Id>,
}

/// A song as stored in `tracks/<id>.json`: what the DJ thinks of as one song,
/// with versions to play. Versions may play different recordings (several
/// people's edits for one line dance; edits of different songs for one
/// choreography).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct TrackRecord {
    pub id: Id,
    pub title: String,
    pub artist: String,
    pub album: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub genre: String,
    pub rating: Option<i64>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub notes: String,
    /// What auto mode says before it (may contain `{dance}`), unless its
    /// version or a set entry says otherwise; empty = nothing; absent: yours.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub announcement: Option<String>,
    #[serde(skip_serializing_if = "ChimeChoice::is_default")]
    pub chime: ChimeChoice,
    pub added_at: i64,
    pub versions: Vec<Version>,
    /// The version that stands for the song (folded library rows, songs
    /// added without a version chosen); absent: the first.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary: Option<Id>,
    /// IDs of songs combined into this one (keeps history references valid).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<Id>,
    /// `false` for songs used only in setlists (e.g. a Spotify song added
    /// straight to a set). They are hidden from library views and removed once
    /// no setlist or crate uses them.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub in_library: bool,
    /// Older files kept the recording in the song: read, then moved to
    /// `recordings/` (see `Store::reload`).
    #[serde(flatten, skip_serializing)]
    pub legacy: LegacyAudio,
}

/// The audio fields older song files had (now a `RecordingRecord`).
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct LegacyAudio {
    pub duration_ms: i64,
    pub bpm: Option<f64>,
    pub bpm_source: Option<String>,
    pub detected_bpm: Option<f64>,
    pub tag_bpm: Option<f64>,
    pub beats_per_bar: Option<u8>,
    pub meter_source: Option<String>,
    pub detected_meter: Option<u8>,
    pub gain_db: f64,
    pub loudness_db: Option<f64>,
    pub audio_start_ms: Option<i64>,
    pub audio_end_ms: Option<i64>,
    pub fingerprint: Option<String>,
    pub analyzed: bool,
    pub analysis_version: u32,
    pub preferred_source: Option<String>,
    pub artwork_url: Option<String>,
    pub sources: Vec<SourceRef>,
    pub hot_cues: Vec<HotCue>,
}

impl TrackRecord {
    /// The recording an older file kept inside the song (`None` if it had none).
    pub fn take_legacy_recording(&mut self) -> Option<RecordingRecord> {
        let l = std::mem::take(&mut self.legacy);
        if l.sources.is_empty() {
            return None;
        }
        Some(RecordingRecord {
            id: self.id.clone(),
            title: self.title.clone(),
            artist: self.artist.clone(),
            album: self.album.clone(),
            genre: self.genre.clone(),
            duration_ms: l.duration_ms,
            bpm: l.bpm,
            bpm_source: l.bpm_source,
            detected_bpm: l.detected_bpm,
            tag_bpm: l.tag_bpm,
            beats_per_bar: l.beats_per_bar,
            meter_source: l.meter_source,
            detected_meter: l.detected_meter,
            gain_db: l.gain_db,
            loudness_db: l.loudness_db,
            peak_db: None,
            audio_start_ms: l.audio_start_ms,
            audio_end_ms: l.audio_end_ms,
            fingerprint: l.fingerprint,
            analyzed: l.analyzed,
            analysis_version: l.analysis_version,
            preferred_source: l.preferred_source,
            artwork_url: l.artwork_url,
            added_at: self.added_at,
            sources: l.sources,
            hot_cues: l.hot_cues,
            aliases: self.aliases.clone(),
            removed: false,
        })
    }
}

/// A song as sent to the UI: the record, the recordings its versions play, and play stats.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    #[serde(flatten)]
    pub rec: TrackRecord,
    pub recordings: Vec<Recording>,
    /// No version has a playable source.
    pub missing: bool,
    pub play_count: i64,
    pub last_played_at: Option<i64>,
}

impl Track {
    /// The recording a version of this song plays.
    pub fn recording_of(&self, version_id: &str) -> Option<&Recording> {
        let v = self.rec.versions.iter().find(|v| v.id == version_id)?;
        self.recordings.iter().find(|r| r.rec.id == v.recording)
    }
}

/// Partial update for a song. Missing fields are left alone; `null` clears.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackPatch {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    #[serde(default, with = "double_option")]
    pub rating: Option<Option<i64>>,
    pub notes: Option<String>,
    #[serde(default, with = "double_option")]
    pub announcement: Option<Option<String>>,
    pub chime: Option<ChimeChoice>,
    /// The primary version (`null`: the first).
    #[serde(default, with = "double_option")]
    pub primary: Option<Option<Id>>,
}

/// Partial update for a recording. Missing fields are left alone; `null` clears.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingPatch {
    #[serde(default, with = "double_option")]
    pub bpm: Option<Option<f64>>,
    pub bpm_source: Option<String>,
    /// Beats per bar set by the DJ (`null` = use the detected meter).
    #[serde(default, with = "double_option")]
    pub beats_per_bar: Option<Option<u8>>,
    pub gain_db: Option<f64>,
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
    /// In a club's set: the club styles to show for this entry (style ids),
    /// or a slot's style. `None` = what the song matches (see `src/lib/clubs.ts`).
    /// May name styles the song doesn't match: the DJ's choice, with a warning.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub club_styles: Option<Vec<String>>,
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
    /// `song` (a song or a style slot) or `pause`: no song, only a pause, an
    /// announcement and/or a hold.
    #[serde(skip_serializing_if = "EntryKind::is_song")]
    pub kind: EntryKind,
    /// Auto mode stops before this entry: the DJ starts it with Play Next.
    #[serde(skip_serializing_if = "is_false")]
    pub hold: bool,
    /// Pause before this entry (before its announcement), overriding the
    /// default; for a pause entry, its length.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pause_before: Option<f64>,
    /// Pause between this entry's announcement and its start.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pause_after: Option<f64>,
    /// What auto mode says before this entry (may contain `{dance}`), instead
    /// of a random pattern; empty = no announcement.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub announcement: Option<String>,
    #[serde(skip_serializing_if = "ChimeChoice::is_default")]
    pub chime: ChimeChoice,
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
            club_styles: None,
            label: None,
            legacy_style: None,
            overrides: Overrides::default(),
            note: String::new(),
            tags: vec![],
            kind: EntryKind::Song,
            hold: false,
            pause_before: None,
            pause_after: None,
            announcement: None,
            chime: ChimeChoice::default(),
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
    /// The club guidelines (one version) the set is planned for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub club: Option<ClubRef>,
    /// Older sets: a club profile (the old format, no longer used).
    #[serde(skip_serializing)]
    pub profile_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub notes: String,
    /// `live` or `preset`; affects defaults and what the projector shows.
    pub mode: String,
    /// The chime before announcements in this set, over its songs'.
    #[serde(skip_serializing_if = "ChimeChoice::is_default")]
    pub chime: ChimeChoice,
    pub created_at: i64,
    pub updated_at: i64,
    pub entries: Vec<SetlistEntry>,
}

/// A club and one version of its guidelines.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClubRef {
    pub id: String,
    pub version: String,
}

/// Setlist without entries, for lists.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetlistSummary {
    pub id: Id,
    pub name: String,
    pub event_date: Option<String>,
    pub club: Option<ClubRef>,
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
            club: s.club.clone(),
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
    /// Moves the set to a club's guidelines (a version), or to none.
    #[serde(default, with = "double_option")]
    pub club: Option<Option<ClubRef>>,
    pub notes: Option<String>,
    pub mode: Option<String>,
    pub chime: Option<ChimeChoice>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewEntry {
    pub version_id: Option<Id>,
    #[serde(default)]
    pub kind: EntryKind,
    pub pause_before: Option<f64>,
    pub announcement: Option<String>,
    #[serde(default)]
    pub chime: ChimeChoice,
    pub styles: Option<Vec<String>>,
    pub club_styles: Option<Vec<String>>,
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
    pub club_styles: Option<Option<Vec<String>>>,
    #[serde(default, with = "double_option")]
    pub label: Option<Option<String>>,
    /// Replaces all overrides.
    pub overrides: Option<Overrides>,
    pub note: Option<String>,
    pub hold: Option<bool>,
    #[serde(default, with = "double_option")]
    pub pause_before: Option<Option<f64>>,
    #[serde(default, with = "double_option")]
    pub pause_after: Option<Option<f64>>,
    #[serde(default, with = "double_option")]
    pub announcement: Option<Option<String>>,
    pub chime: Option<ChimeChoice>,
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


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_style_tables_become_rules() {
        let mut lib: StyleLibrary =
            serde_json::from_str(r#"{"styles":[],"clubMappings":{"club":{"waltz":["rotary","cross-step"]}}}"#).unwrap();
        lib.upgrade();
        lib.upgrade();
        let rule = |m: &str, c: &str| ClubRule { mine: m.into(), club: c.into() };
        assert_eq!(lib.club_rules["club"], vec![rule("waltz", "rotary"), rule("waltz", "cross-step")]);
        let saved = serde_json::to_string(&lib).unwrap();
        assert!(saved.contains("clubRules") && !saved.contains("clubMappings"), "{saved}");
    }
}

//! App settings (per machine) and club profiles (shareable).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AudioSettings {
    /// cpal device id (`host:id`); `None` = system default.
    pub master_device: Option<String>,
    /// Zero-based channel pair on the master device (DJ2GO2: 0,1 = speakers).
    pub master_channels: [u16; 2],
    /// `None` = no headphone cue output.
    pub cue_device: Option<String>,
    /// DJ2GO2: 2,3 = headphones.
    pub cue_channels: [u16; 2],
    pub buffer_ms: u32,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            master_device: None,
            master_channels: [0, 1],
            cue_device: None,
            cue_channels: [0, 1],
            buffer_ms: 80,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProjectorSettings {
    /// Heading shown on the projector (usually the club or event name).
    pub title: String,
    /// `auto` follows the setlist mode; `live` and `preset` force a layout.
    pub display_mode: String,
    pub history_count: u32,
    /// Live layout: how many upcoming songs / slots to list (0 = only "Up next").
    pub upcoming_count: u32,
    /// Live layout: show the next dance once it is queued.
    pub show_next: bool,
    /// During the pause between songs, show the upcoming dance style so dancers
    /// can find a partner.
    pub reveal_next_in_gap: bool,
    pub show_artist: bool,
    pub show_bpm: bool,
    pub show_clock: bool,
    /// Cover art of the current song.
    pub show_artwork: bool,
    pub font_scale: f64,
    pub theme: String,
}

impl Default for ProjectorSettings {
    fn default() -> Self {
        Self {
            title: String::new(),
            display_mode: "auto".into(),
            history_count: 5,
            upcoming_count: 0,
            show_next: true,
            reveal_next_in_gap: true,
            show_artist: true,
            show_bpm: false,
            show_clock: true,
            show_artwork: true,
            font_scale: 1.0,
            theme: "dark".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MidiSettings {
    pub enabled: bool,
    pub mapping_id: String,
    /// User-created mappings (same shape as the built-in ones).
    pub custom_mappings: Vec<crate::midi::Mapping>,
}

impl Default for MidiSettings {
    fn default() -> Self {
        // Empty = automatic: pick the built-in mapping matching the controller.
        Self { enabled: true, mapping_id: String::new(), custom_mappings: vec![] }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct SpotifySettings {
    /// Your own Spotify app's client id, used for search and playlists (Web
    /// API). Empty = librespot's shared client id, which Spotify rate-limits.
    pub client_id: String,
    /// Reconnect automatically at startup if credentials are cached.
    pub auto_connect: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Folder holding the shareable library (tracks, setlists, crates, profiles).
    /// `None` = `<app data>/library`. Point this at a git repository or synced folder.
    pub library_dir: Option<String>,
    pub active_profile_id: String,
    pub library_folders: Vec<String>,
    pub audio: AudioSettings,
    /// Overrides the profile's pause length when set.
    pub gap_seconds: Option<f64>,
    /// Overrides the profile's auto-advance setting when set.
    pub auto_advance: Option<bool>,
    /// When "Play Next" is pressed while a song is still playing, fade it out
    /// over this many ms first. Songs never overlap.
    pub manual_next_fade_ms: u32,
    /// Overrides the profile's fade-out duration when set.
    pub fade_out_ms: Option<u32>,
    /// Pitch-preserving tempo changes.
    pub keylock: bool,
    /// Time-stretch engine: `signalsmith` (default), `soundtouch` or `bungee`.
    pub stretch_engine: String,
    /// Range of the tempo sliders / MIDI pitch faders in percent.
    pub tempo_range_pct: f64,
    pub auto_gain: bool,
    pub target_loudness_db: f64,
    /// Skip leading/trailing silence detected during analysis.
    pub trim_silence: bool,
    /// Require pressing Play Next twice when a song is still playing.
    pub confirm_next_while_playing: bool,
    /// Load the up-next song onto the free deck automatically.
    pub auto_load_next: bool,
    /// While the on-air deck is playing, ignore jog scrubbing, CUE and jumps on it.
    pub protect_on_air: bool,
    /// When a deck goes on air, switch headphone cueing to the other deck.
    pub headphones_follow_free_deck: bool,
    pub projector: ProjectorSettings,
    pub midi: MidiSettings,
    pub spotify: SpotifySettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            library_dir: None,
            active_profile_id: "none".into(),
            library_folders: vec![],
            audio: AudioSettings::default(),
            gap_seconds: None,
            auto_advance: None,
            manual_next_fade_ms: 2500,
            fade_out_ms: None,
            keylock: true,
            stretch_engine: "signalsmith".into(),
            tempo_range_pct: 10.0,
            auto_gain: false,
            target_loudness_db: -14.0,
            trim_silence: true,
            confirm_next_while_playing: true,
            auto_load_next: true,
            protect_on_air: true,
            headphones_follow_free_deck: true,
            projector: ProjectorSettings::default(),
            midi: MidiSettings::default(),
            spotify: SpotifySettings { client_id: String::new(), auto_connect: true },
        }
    }
}

/// The parts of a club profile the backend needs. The full profile (including
/// rules, which are evaluated in the UI) is stored as raw JSON.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct ProfileCore {
    pub id: String,
    pub name: String,
    pub gap: GapConfig,
    pub auto_advance: bool,
    pub fade_out_seconds: f64,
    pub styles: Vec<StyleCore>,
    pub tags: Vec<TagCore>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GapConfig {
    pub target_seconds: f64,
}

impl Default for GapConfig {
    fn default() -> Self {
        Self { target_seconds: 8.0 }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct StyleCore {
    pub key: String,
    pub name: String,
    pub short: String,
    pub color: String,
    pub sweet: Option<[f64; 2]>,
    pub extended: Option<[f64; 2]>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct TagCore {
    pub id: String,
    pub name: String,
    pub color: String,
    pub announce: bool,
}

/// Only "No club" is built in; clubs are your own (or imported, see
/// `examples/clubs/`).
pub const BUILTIN_PROFILES: &[&str] = &[include_str!("../profiles/none.json")];

impl Settings {
    pub fn library_path(&self, data_dir: &std::path::Path) -> std::path::PathBuf {
        match &self.library_dir {
            Some(d) if !d.trim().is_empty() => std::path::PathBuf::from(d),
            _ => data_dir.join("library"),
        }
    }
}

impl ProfileCore {
    pub fn style(&self, key: &str) -> Option<&StyleCore> {
        self.styles.iter().find(|s| s.key == key)
    }

    pub fn style_name(&self, key: &str) -> String {
        self.style(key).map(|s| s.name.clone()).unwrap_or_else(|| prettify_key(key))
    }
}

pub fn prettify_key(key: &str) -> String {
    key.split('-')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

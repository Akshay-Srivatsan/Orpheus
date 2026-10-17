//! Chimes before announcements, and what an entry's announcement and chime
//! are: the entry's own choice, else its set's (chimes only), its version's,
//! its song's, and finally your settings.

use crate::model::{ChimeChoice, EntryKind, Setlist, SetlistEntry, TrackRecord, Version};
use crate::settings::AnnounceSettings;
use anyhow::{anyhow, Result};
use std::f32::consts::TAU;
use std::path::Path;

/// The built-in chime: notes (Hz, start in seconds), each ringing out.
const NOTES: [(f32, f32); 2] = [(783.99, 0.0), (659.25, 0.45)];
/// How long a note of the built-in chime rings (seconds), and how fast it fades.
const RING_SECS: f32 = 1.6;
const DECAY_PER_SEC: f32 = 3.2;
/// Overtones of each note (multiple of the note, level).
const OVERTONES: [(f32, f32); 3] = [(1.0, 1.0), (2.0, 0.35), (3.0, 0.12)];
/// How fast a note of the built-in chime is struck (seconds to full level).
const ATTACK_SECS: f32 = 0.0025;
/// Peak level of the built-in chime.
const PEAK: f32 = 0.5;
/// Silence between the chime and the announcement (seconds).
const GAP_SECS: f32 = 0.3;
/// Rate a chime is previewed at.
pub const PREVIEW_RATE: u32 = 48_000;
/// A chime file is cut off after this long (seconds): it's a chime, not a song.
pub const MAX_FILE_SECS: f64 = 10.0;

/// The levels an entry's announcement and chime come from, most specific first.
pub struct Layers<'a> {
    pub entry: Option<&'a SetlistEntry>,
    pub set: Option<&'a Setlist>,
    pub version: Option<&'a Version>,
    pub song: Option<&'a TrackRecord>,
}

/// A resolved chime: none, or a sound (empty = the built-in one).
pub type Chime = Option<String>;

impl<'a> Layers<'a> {
    /// The same, without the most specific level (what it inherits).
    pub fn above_entry(&self) -> Self {
        Self { entry: None, set: self.set, version: self.version, song: self.song }
    }

    /// The announcement text something chose for it (empty = nothing), if any did.
    pub fn announcement(&self) -> Option<&'a str> {
        self.entry
            .and_then(|e| e.announcement.as_deref())
            .or_else(|| self.version.and_then(|v| v.announcement.as_deref()))
            .or_else(|| self.song.and_then(|s| s.announcement.as_deref()))
    }

    /// Whether something is said before it.
    pub fn speaks(&self, a: &AnnounceSettings) -> bool {
        match self.announcement() {
            Some(t) => !t.trim().is_empty(),
            None => a.enabled && self.entry.map_or(true, |e| e.kind == EntryKind::Song) && a.patterns.iter().any(|p| !p.trim().is_empty()),
        }
    }

    fn chimes(&self) -> impl Iterator<Item = &'a ChimeChoice> {
        [self.entry.map(|e| &e.chime), self.set.map(|s| &s.chime), self.version.map(|v| &v.chime), self.song.map(|s| &s.chime)].into_iter().flatten()
    }

    /// Whether a chime plays before its announcement.
    pub fn chime_enabled(&self, a: &AnnounceSettings) -> bool {
        self.chimes().find_map(|c| c.enabled).unwrap_or(a.chime)
    }

    /// The chime's sound (empty = the built-in one), whether or not it plays.
    pub fn chime_sound(&self, a: &AnnounceSettings) -> String {
        self.chimes().find_map(|c| c.sound.clone()).unwrap_or_else(|| a.chime_sound.clone())
    }

    /// What the levels choose, without your settings.
    pub fn chosen_chime(&self) -> ChimeChoice {
        ChimeChoice { enabled: self.chimes().find_map(|c| c.enabled), sound: self.chimes().find_map(|c| c.sound.clone()) }
    }

    pub fn chime(&self, a: &AnnounceSettings) -> Chime {
        self.chime_enabled(a).then(|| self.chime_sound(a))
    }
}

/// The built-in chime at `rate`.
pub fn built_in(rate: u32) -> Vec<f32> {
    let rate_f = rate as f32;
    let end = NOTES.iter().map(|(_, at)| at + RING_SECS).fold(0.0, f32::max);
    let mut out = vec![0.0f32; (end * rate_f) as usize];
    for (freq, at) in NOTES {
        let start = (at * rate_f) as usize;
        for (i, s) in out[start..].iter_mut().enumerate().take((RING_SECS * rate_f) as usize) {
            let t = i as f32 / rate_f;
            // A quick attack, then an exponential decay (a struck bell).
            let env = (t / ATTACK_SECS).min(1.0) * (-DECAY_PER_SEC * t).exp();
            *s += env * OVERTONES.iter().map(|(m, l)| l * (TAU * freq * m * t).sin()).sum::<f32>();
        }
    }
    let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if peak > 0.0 {
        out.iter_mut().for_each(|s| *s *= PEAK / peak);
    }
    out
}

/// A chime file, mono at `rate` (at most `MAX_FILE_SECS`).
fn from_file(path: &Path, rate: u32) -> Result<Vec<f32>> {
    let (mut samples, from) = crate::decode::decode_mono(path)?;
    samples.truncate((MAX_FILE_SECS * from as f64) as usize);
    if samples.is_empty() {
        return Err(anyhow!("{} has no audio", path.display()));
    }
    Ok(resample(&samples, from, rate))
}

/// Linear resampling (plenty for a chime).
fn resample(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || from == 0 {
        return samples.to_vec();
    }
    let step = from as f64 / to as f64;
    let n = (samples.len() as f64 / step) as usize;
    (0..n)
        .map(|i| {
            let x = i as f64 * step;
            let j = x as usize;
            let f = (x - j as f64) as f32;
            let a = samples[j];
            let b = *samples.get(j + 1).unwrap_or(&a);
            a + (b - a) * f
        })
        .collect()
}

/// The chime's samples at `rate`: its file, or the built-in one. A file that
/// can't be read gives the built-in chime, and the reason.
pub fn samples(sound: &str, rate: u32) -> (Vec<f32>, Option<String>) {
    if sound.is_empty() {
        return (built_in(rate), None);
    }
    match from_file(Path::new(sound), rate) {
        Ok(s) => (s, None),
        Err(e) => (built_in(rate), Some(format!("The chime {sound} can't be played ({e:#}); the built-in one plays instead."))),
    }
}

/// `speech` with the chime before it.
pub fn before(chime: Vec<f32>, speech: &[f32], rate: u32) -> Vec<f32> {
    let mut out = chime;
    out.extend(std::iter::repeat(0.0).take((GAP_SECS * rate as f32) as usize));
    out.extend_from_slice(speech);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chime(enabled: Option<bool>, sound: Option<&str>) -> ChimeChoice {
        ChimeChoice { enabled, sound: sound.map(String::from) }
    }

    #[test]
    fn the_most_specific_choice_wins() {
        let a = AnnounceSettings { chime: false, chime_sound: "mine.ogg".into(), ..Default::default() };
        let song = TrackRecord { chime: chime(Some(true), None), announcement: Some("Song".into()), ..Default::default() };
        let version: Version = serde_json::from_str(r#"{"id":"v","name":"v","chime":{"sound":"version.ogg"}}"#).unwrap();
        let mut set = Setlist::default();
        let entry = SetlistEntry::default();
        let layers = |set: &Setlist, entry: &SetlistEntry| -> (Chime, Option<String>) {
            let l = Layers { entry: Some(entry), set: Some(set), version: Some(&version), song: Some(&song) };
            (l.chime(&a), l.announcement().map(String::from))
        };
        // The song turns the chime on although you don't; its version picks the sound.
        assert_eq!(layers(&set, &entry), (Some("version.ogg".into()), Some("Song".into())));
        // The set's sound is over the version's; its "off" is over the song's "on".
        set.chime = chime(None, Some("set.ogg"));
        assert_eq!(layers(&set, &entry).0, Some("set.ogg".into()));
        set.chime = chime(Some(false), None);
        assert_eq!(layers(&set, &entry).0, None);
        // The entry is over everything.
        let entry = SetlistEntry { chime: chime(Some(true), Some("")), announcement: Some(String::new()), ..Default::default() };
        assert_eq!(layers(&set, &entry), (Some(String::new()), Some(String::new())));
        // Nothing chosen: your settings.
        let plain = Layers { entry: Some(&SetlistEntry::default()), set: None, version: None, song: None };
        assert_eq!(plain.chime(&a), None);
        assert_eq!(plain.chime(&AnnounceSettings { chime: true, ..a.clone() }), Some("mine.ogg".into()));
    }

    #[test]
    fn a_song_or_version_can_speak_with_announcements_off() {
        let a = AnnounceSettings { enabled: false, ..Default::default() };
        let song = TrackRecord { announcement: Some("A mixer!".into()), ..Default::default() };
        let entry = SetlistEntry::default();
        assert!(Layers { entry: Some(&entry), set: None, version: None, song: Some(&song) }.speaks(&a));
        assert!(!Layers { entry: Some(&entry), set: None, version: None, song: None }.speaks(&a));
        // The entry's "nothing" is over the song's text.
        let quiet = SetlistEntry { announcement: Some(String::new()), ..Default::default() };
        assert!(!Layers { entry: Some(&quiet), set: None, version: None, song: Some(&song) }.speaks(&a));
    }

    #[test]
    fn the_chime_comes_first() {
        let rate = 24_000;
        let c = built_in(rate);
        assert!(!c.is_empty() && c.iter().all(|s| s.abs() <= PEAK + 1e-4));
        let speech = vec![0.25; 100];
        let out = before(c.clone(), &speech, rate);
        assert_eq!(&out[..c.len()], &c[..]);
        assert_eq!(&out[out.len() - 100..], &speech[..]);
        let (fallback, why) = samples("/nonexistent/chime.ogg", rate);
        assert_eq!(fallback.len(), c.len());
        assert!(why.is_some());
        assert_eq!(resample(&[0.0, 1.0], 1, 2).len(), 4);
    }
}

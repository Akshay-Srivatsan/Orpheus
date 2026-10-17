//! Chimes before announcements (a version of a song, played as it's set),
//! and what an entry's announcement and chime are: the entry's own choice, else its set's (chimes only), its version's,
//! its song's, and finally your settings.

use crate::model::{ChimeChoice, EntryKind, Id, Setlist, SetlistEntry, TrackRecord, Version};
use crate::settings::AnnounceSettings;
use anyhow::{anyhow, Result};

/// Silence between the chime and the announcement (seconds).
const GAP_SECS: f32 = 0.3;
/// Rate a chime is previewed at.
pub const PREVIEW_RATE: u32 = 48_000;
/// A chime with no end of its own (a version with no end or fade set) is cut off after this long (seconds): it's a chime, not a song.
pub const MAX_OPEN_SECS: f64 = 10.0;
/// How long a chime cut off that way fades out (seconds).
const CUT_FADE_SECS: f64 = 0.5;

/// The levels an entry's announcement and chime come from, most specific first.
pub struct Layers<'a> {
    pub entry: Option<&'a SetlistEntry>,
    pub set: Option<&'a Setlist>,
    pub version: Option<&'a Version>,
    pub song: Option<&'a TrackRecord>,
}

/// A resolved chime: none, or the version it plays.
pub type Chime = Option<Id>;

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

    /// The chime's version, whether or not it plays.
    pub fn chime_sound(&self, a: &AnnounceSettings) -> Option<Id> {
        self.chimes().find_map(|c| c.sound.clone()).or_else(|| a.chime_sound.clone()).filter(|v| !v.is_empty())
    }

    /// What the levels choose, without your settings.
    pub fn chosen_chime(&self) -> ChimeChoice {
        ChimeChoice { enabled: self.chimes().find_map(|c| c.enabled), sound: self.chimes().find_map(|c| c.sound.clone()) }
    }

    pub fn chime(&self, a: &AnnounceSettings) -> Chime {
        self.chime_enabled(a).then(|| self.chime_sound(a)).flatten()
    }
}

/// Audio to play as a chime: part of a file (from `start`, to `end` or
/// `MAX_OPEN_SECS` later, fading out over `fade` before the end), at `gain`.
#[derive(Debug, Clone, PartialEq)]
pub struct Excerpt {
    pub path: std::path::PathBuf,
    pub start_secs: f64,
    pub end_secs: Option<f64>,
    pub fade_secs: f64,
    pub gain: f32,
}

/// An excerpt, mono at `rate`.
pub fn render(x: &Excerpt, rate: u32) -> Result<Vec<f32>> {
    let mut dec = crate::decode::LocalDecoder::open(&x.path)?;
    if x.start_secs > 0.0 {
        dec.seek(x.start_secs)?;
    }
    let (end, fade) = match x.end_secs {
        Some(e) if e > x.start_secs => (e, x.fade_secs),
        _ => (x.start_secs + MAX_OPEN_SECS, CUT_FADE_SECS),
    };
    let frames = ((end - x.start_secs) * dec.sample_rate as f64) as usize;
    let (mut stereo, mut mono) = (Vec::new(), Vec::with_capacity(frames));
    while mono.len() < frames && dec.read(&mut stereo)? {
        mono.extend(stereo.chunks_exact(2).map(|f| (f[0] + f[1]) * 0.5 * x.gain));
        stereo.clear();
    }
    mono.truncate(frames);
    if mono.is_empty() {
        return Err(anyhow!("{} has no audio there", x.path.display()));
    }
    let fade_frames = ((fade * dec.sample_rate as f64) as usize).min(mono.len());
    let n = mono.len();
    for (i, s) in mono[n - fade_frames..].iter_mut().enumerate() {
        *s *= 1.0 - i as f32 / fade_frames as f32;
    }
    Ok(resample(&mono, dec.sample_rate, rate))
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
        let a = AnnounceSettings { chime: false, chime_sound: Some("mine".into()), ..Default::default() };
        let song = TrackRecord { chime: chime(Some(true), None), announcement: Some("Song".into()), ..Default::default() };
        let version: Version = serde_json::from_str(r#"{"id":"v","name":"v","chime":{"sound":"bell"}}"#).unwrap();
        let mut set = Setlist::default();
        let entry = SetlistEntry::default();
        let layers = |set: &Setlist, entry: &SetlistEntry| -> (Chime, Option<String>) {
            let l = Layers { entry: Some(entry), set: Some(set), version: Some(&version), song: Some(&song) };
            (l.chime(&a), l.announcement().map(String::from))
        };
        // The song turns the chime on although you don't; its version picks the sound.
        assert_eq!(layers(&set, &entry), (Some("bell".into()), Some("Song".into())));
        // The set's sound is over the version's; its "off" is over the song's "on".
        set.chime = chime(None, Some("gong"));
        assert_eq!(layers(&set, &entry).0, Some("gong".into()));
        set.chime = chime(Some(false), None);
        assert_eq!(layers(&set, &entry).0, None);
        // The entry is over everything.
        let entry = SetlistEntry { chime: chime(Some(true), Some("ding")), announcement: Some(String::new()), ..Default::default() };
        assert_eq!(layers(&set, &entry), (Some("ding".into()), Some(String::new())));
        // Nothing chosen: your settings; on without a version: nothing to play.
        let plain = Layers { entry: Some(&SetlistEntry::default()), set: None, version: None, song: None };
        assert_eq!(plain.chime(&a), None);
        assert_eq!(plain.chime(&AnnounceSettings { chime: true, ..a.clone() }), Some("mine".into()));
        assert_eq!(plain.chime(&AnnounceSettings { chime: true, chime_sound: None, ..a.clone() }), None);
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
        let c = vec![0.5; 50];
        let speech = vec![0.25; 100];
        let out = before(c.clone(), &speech, rate);
        assert_eq!(&out[..c.len()], &c[..]);
        assert_eq!(out.len(), 50 + (GAP_SECS * rate as f32) as usize + 100);
        assert_eq!(&out[out.len() - 100..], &speech[..]);
        let missing = Excerpt { path: "/nonexistent/chime.ogg".into(), start_secs: 0.0, end_secs: None, fade_secs: 0.0, gain: 1.0 };
        assert!(render(&missing, rate).is_err());
        assert_eq!(resample(&[0.0, 1.0], 1, 2).len(), 4);
    }
}

//! Chimes before announcements (a version of a song, played as it's set),
//! and what an entry's announcement and chime are: the entry's own choice, else its set's (chimes only), its version's,
//! its song's, and finally your settings.

use crate::model::{ChimeChoice, EntryKind, Id, Setlist, SetlistEntry, TrackRecord, Version};
use crate::settings::AnnounceSettings;
use anyhow::{anyhow, Result};

/// Silence between the chime and the announcement (seconds).
pub const GAP_SECS: f32 = 0.3;
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

    /// The pause before its announcement, and after it (seconds), unless the
    /// entry has its own: its set's, else its version's, else its song's, else yours.
    pub fn announce_pauses(&self, a: &AnnounceSettings) -> (f64, f64) {
        let pick = |f: fn(&Setlist) -> Option<f64>, g: fn(&Version) -> Option<f64>, h: fn(&TrackRecord) -> Option<f64>, yours: f64| {
            self.set.and_then(f).or_else(|| self.version.and_then(g)).or_else(|| self.song.and_then(h)).unwrap_or(yours)
        };
        (
            pick(|s| s.announce_pause_before, |v| v.announce_pause_before, |t| t.announce_pause_before, a.pause_before_secs),
            pick(|s| s.announce_pause_after, |v| v.announce_pause_after, |t| t.announce_pause_after, a.pause_after_secs),
        )
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
/// `MAX_OPEN_SECS` later, fading out over `fade` before the end), at `gain`,
/// played `rate` times as fast (with `stretch`'s engine, keeping its pitch
/// with `keylock`), as a deck plays it.
#[derive(Debug, Clone, PartialEq)]
pub struct Excerpt {
    pub path: std::path::PathBuf,
    pub start_secs: f64,
    pub end_secs: Option<f64>,
    pub fade_secs: f64,
    pub gain: f32,
    pub rate: f64,
    pub keylock: bool,
    pub stretch: crate::audio::stretch::Engine,
}

/// Source frames stretched at a time.
const STRETCH_BLOCK: usize = 4096;

/// An excerpt, mono at `rate`.
pub fn render(x: &Excerpt, rate: u32) -> Result<Vec<f32>> {
    let mut dec = crate::decode::LocalDecoder::open(&x.path)?;
    let src_rate = dec.sample_rate;
    if x.start_secs > 0.0 {
        dec.seek(x.start_secs)?;
    }
    let (end, fade) = match x.end_secs {
        Some(e) if e > x.start_secs => (e, x.fade_secs),
        _ => (x.start_secs + MAX_OPEN_SECS * x.rate.max(0.01), CUT_FADE_SECS),
    };
    let frames = ((end - x.start_secs) * src_rate as f64) as usize;
    let (mut block, mut stereo) = (Vec::new(), Vec::with_capacity(frames * 2));
    while stereo.len() < frames * 2 && dec.read(&mut block)? {
        stereo.extend_from_slice(&block);
        block.clear();
    }
    stereo.truncate(frames * 2);
    if stereo.is_empty() {
        return Err(anyhow!("{} has no audio there", x.path.display()));
    }
    let n = stereo.len() / 2;
    let fade_frames = ((fade * src_rate as f64) as usize).min(n);
    for (i, f) in stereo[(n - fade_frames) * 2..].chunks_exact_mut(2).enumerate() {
        let g = 1.0 - i as f32 / fade_frames as f32;
        f[0] *= g;
        f[1] *= g;
    }
    let stereo = if (x.rate - 1.0).abs() > 1e-3 { stretch(&stereo, src_rate, x) } else { stereo };
    let mono: Vec<f32> = stereo.chunks_exact(2).map(|f| (f[0] + f[1]) * 0.5 * x.gain).collect();
    Ok(resample(&mono, src_rate, rate))
}

/// `stereo` played `x.rate` times as fast, at the same sample rate.
fn stretch(stereo: &[f32], src_rate: u32, x: &Excerpt) -> Vec<f32> {
    let mut s = crate::audio::stretch::create(x.stretch, src_rate);
    s.set_pitch(if x.keylock { 1.0 } else { x.rate as f32 });
    let mut out = Vec::with_capacity((stereo.len() as f64 / x.rate) as usize + STRETCH_BLOCK * 2);
    let mut buf = Vec::new();
    for chunk in stereo.chunks(STRETCH_BLOCK * 2) {
        buf.resize(((chunk.len() / 2) as f64 / x.rate).round() as usize * 2, 0.0);
        s.process(chunk, &mut buf);
        out.extend_from_slice(&buf);
    }
    let mut tail = Vec::new();
    s.flush(&mut tail);
    out.extend_from_slice(&tail);
    // The engine's delay: drop it from the start.
    let delay = ((s.latency_src_frames() / x.rate) as usize * 2).min(out.len());
    out.drain(..delay);
    out
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
    fn announcement_pauses_come_from_the_most_specific_level() {
        let a = AnnounceSettings { pause_before_secs: 3.0, pause_after_secs: 4.0, ..Default::default() };
        let song = TrackRecord { announce_pause_after: Some(6.0), ..Default::default() };
        let mut version: Version = serde_json::from_str(r#"{"id":"v","name":"v"}"#).unwrap();
        let mut set = Setlist::default();
        let at = |set: &Setlist, version: &Version| Layers { entry: None, set: Some(set), version: Some(version), song: Some(&song) }.announce_pauses(&a);
        assert_eq!(at(&set, &version), (3.0, 6.0), "yours, and the song's");
        version.announce_pause_after = Some(5.0);
        assert_eq!(at(&set, &version), (3.0, 5.0), "the version's over the song's");
        set.announce_pause_before = Some(1.0);
        set.announce_pause_after = Some(2.0);
        assert_eq!(at(&set, &version), (1.0, 2.0), "the set's over both");
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
        let missing = Excerpt {
            path: "/nonexistent/chime.ogg".into(),
            start_secs: 0.0,
            end_secs: None,
            fade_secs: 0.0,
            gain: 1.0,
            rate: 1.0,
            keylock: true,
            stretch: crate::audio::stretch::Engine::Signalsmith,
        };
        assert!(render(&missing, rate).is_err());
        assert_eq!(resample(&[0.0, 1.0], 1, 2).len(), 4);
    }

    /// A WAV file: `secs` of a tone, 16-bit stereo at `rate`.
    fn tone_file(dir: &std::path::Path, secs: f64, rate: u32) -> std::path::PathBuf {
        let n = (secs * rate as f64) as u32;
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + n * 4).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&rate.to_le_bytes());
        b.extend_from_slice(&(rate * 4).to_le_bytes());
        b.extend_from_slice(&4u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&(n * 4).to_le_bytes());
        for i in 0..n {
            let v = ((i as f32 / rate as f32 * 440.0 * std::f32::consts::TAU).sin() * 8000.0) as i16;
            b.extend_from_slice(&v.to_le_bytes());
            b.extend_from_slice(&v.to_le_bytes());
        }
        let p = dir.join("tone.wav");
        std::fs::write(&p, b).unwrap();
        p
    }

    #[test]
    fn a_chime_plays_at_its_tempo_and_stops_at_its_end() {
        let dir = std::env::temp_dir().join(format!("orpheus-chime-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = tone_file(&dir, 4.0, 44_100);
        let x = |rate: f64, end: Option<f64>| Excerpt {
            path: path.clone(),
            start_secs: 1.0,
            end_secs: end,
            fade_secs: 0.0,
            gain: 1.0,
            rate,
            keylock: true,
            stretch: crate::audio::stretch::Engine::Signalsmith,
        };
        let secs = |s: Vec<f32>| s.len() as f64 / 24_000.0;
        // 1 s to 3 s: 2 s as it is, about 1 s at double tempo.
        assert!((secs(render(&x(1.0, Some(3.0)), 24_000).unwrap()) - 2.0).abs() < 0.01);
        let fast = secs(render(&x(2.0, Some(3.0)), 24_000).unwrap());
        assert!((fast - 1.0).abs() < 0.15, "{fast}");
        // No end of its own: the rest of the file (under the limit).
        assert!((secs(render(&x(1.0, None), 24_000).unwrap()) - 3.0).abs() < 0.01);
        std::fs::remove_dir_all(&dir).ok();
    }
}

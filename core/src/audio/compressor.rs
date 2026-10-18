//! A slow compressor for songs with a wide dynamic range (an orchestral
//! waltz with quiet passages): it turns passages louder than a threshold some
//! dB below the song's (levelled) loudness down, then makes that up, so the
//! song's main level stays where levelling put it and its quiet passages come
//! up relative to it. A song that stays near one level is barely touched.
//! It follows passages, not beats (slow attack and release), and holds still
//! through silence (it never pulls up noise or a song's silent end).

/// How strongly dynamics are evened out (Settings → General).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Strength {
    /// The threshold, this far below the song's loudness (dB).
    pub depth_db: f32,
    pub ratio: f32,
}

impl Strength {
    /// `light`, `medium` or `strong` (anything else: off).
    pub fn from_setting(s: &str) -> Option<Self> {
        match s {
            "light" => Some(Self { depth_db: 8.0, ratio: 2.0 }),
            "medium" => Some(Self { depth_db: 12.0, ratio: 3.0 }),
            "strong" => Some(Self { depth_db: 16.0, ratio: 4.0 }),
            _ => None,
        }
    }

    /// As a deck holds it (0: off; see `index`).
    pub fn from_index(i: u8) -> Option<Self> {
        Self::from_setting(STRENGTHS.get(i as usize)?)
    }

    /// A setting as a deck holds it.
    pub fn index(setting: &str) -> u8 {
        STRENGTHS.iter().position(|s| *s == setting).unwrap_or(0) as u8
    }

    /// The most it raises a quiet passage (the make-up gain, dB).
    pub fn max_boost_db(&self) -> f32 {
        self.depth_db * (1.0 - 1.0 / self.ratio)
    }

    /// The gain (dB) for a passage at `level_db`, the song being at `reference_db`.
    pub fn gain_db(&self, level_db: f32, reference_db: f32) -> f32 {
        let over = level_db - (reference_db - self.depth_db);
        let reduction = if over > 0.0 { over * (1.0 - 1.0 / self.ratio) } else { 0.0 };
        self.max_boost_db() - reduction
    }
}

/// The settings, in the order decks hold them (0: off).
const STRENGTHS: [&str; 4] = ["off", "light", "medium", "strong"];

/// How fast it hears the level (s): long enough to follow passages, not notes.
const DETECT_SECS: f32 = 0.3;
/// How fast it turns down when a passage gets louder, and up when it gets quieter (s).
const ATTACK_SECS: f32 = 0.15;
const RELEASE_SECS: f32 = 2.5;
/// Quieter than this below the song's loudness is silence (or nearly): the gain holds.
const GATE_DB: f32 = 30.0;
/// The level scale's offset, as in LUFS (so the reference is a song's loudness).
const LUFS_OFFSET_DB: f32 = -0.691;

/// One deck's compressor state.
pub struct Compressor {
    power: f32,
    gain_db: f32,
}

impl Compressor {
    /// Starts unchanged, hearing silence: its gain holds until the song is
    /// heard (a quiet start is raised gradually, never at once).
    pub fn new() -> Self {
        Self { power: 0.0, gain_db: 0.0 }
    }

    /// Compresses interleaved stereo `frames` in place, at `rate` Hz.
    pub fn process(&mut self, frames: &mut [f32], rate: f32, s: Strength, reference_db: f32) {
        let coef = |secs: f32| 1.0 - (-1.0 / (secs * rate.max(1.0))).exp();
        let (detect, attack, release) = (coef(DETECT_SECS), coef(ATTACK_SECS), coef(RELEASE_SECS));
        for fr in frames.chunks_exact_mut(2) {
            self.power += (fr[0] * fr[0] + fr[1] * fr[1] - self.power) * detect;
            let level = LUFS_OFFSET_DB + 10.0 * self.power.max(1e-12).log10();
            if level > reference_db - GATE_DB {
                let want = s.gain_db(level, reference_db);
                self.gain_db += (want - self.gain_db) * if want < self.gain_db { attack } else { release };
            }
            let g = 10f32.powf(self.gain_db / 20.0);
            fr[0] *= g;
            fr[1] *= g;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(amp: f32, secs: f32, rate: f32) -> Vec<f32> {
        (0..(secs * rate) as usize)
            .flat_map(|i| {
                let s = amp * (i as f32 / rate * 440.0 * std::f32::consts::TAU).sin();
                [s, s]
            })
            .collect()
    }
    fn rms_db(x: &[f32]) -> f32 {
        10.0 * (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).log10()
    }

    #[test]
    fn the_main_level_stays_and_quiet_passages_come_up() {
        let s = Strength::from_setting("medium").unwrap();
        // At its loudness: no change; far above the threshold: turned down; below it: the full boost.
        assert!(s.gain_db(-14.0, -14.0).abs() < 1e-4);
        assert!(s.gain_db(-8.0, -14.0) < 0.0);
        assert!((s.gain_db(-40.0, -14.0) - s.max_boost_db()).abs() < 1e-4);

        let rate = 48_000.0;
        // A song at about -14 (a sine of 0.28 is about -14 LUFS-ish here), then a passage 18 dB quieter.
        let loud = tone(0.28, 4.0, rate);
        let quiet = tone(0.28 / 7.94, 8.0, rate);
        let reference = LUFS_OFFSET_DB + rms_db(&loud) + 3.01;
        let mut c = Compressor::new();
        let mut a = loud.clone();
        c.process(&mut a, rate, s, reference);
        let mut b = quiet.clone();
        c.process(&mut b, rate, s, reference);
        // The loud part is about where it was (after its first moment); the quiet one ends up to `max_boost` louder.
        let settled = a.len() / 4;
        assert!((rms_db(&a[settled..]) - rms_db(&loud[settled..])).abs() < 1.0, "{} {}", rms_db(&a), rms_db(&loud));
        let tail = &b[b.len() / 2..];
        let lift = rms_db(tail) - rms_db(&quiet[quiet.len() / 2..]);
        assert!((lift - s.max_boost_db()).abs() < 1.0, "lift {lift}");
    }

    #[test]
    fn silence_isnt_pulled_up() {
        let s = Strength::from_setting("strong").unwrap();
        let mut c = Compressor::new();
        let mut quiet = tone(1e-4, 5.0, 48_000.0);
        c.process(&mut quiet, 48_000.0, s, -14.0);
        assert!(c.gain_db.abs() < 0.5, "gain {}", c.gain_db);
        assert_eq!(Strength::from_setting("off"), None);
    }
}

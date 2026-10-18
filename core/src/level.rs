//! A song's level: integrated loudness (EBU R128, in LUFS) and true peak
//! (dBTP), measured on its stereo audio as it's decoded for analysis. Songs
//! are levelled to a loudness, their boost capped so their peaks stay clear.

use ebur128::{EbuR128, Mode};

/// What a song measures.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Level {
    pub lufs: f64,
    pub true_peak_db: f64,
}

/// Measures interleaved stereo audio fed to it (any length at a time).
pub struct Meter {
    inner: Option<EbuR128>,
}

impl Meter {
    pub fn new(rate: u32) -> Self {
        let inner = EbuR128::new(2, rate, Mode::I | Mode::TRUE_PEAK)
            .map_err(|e| log::warn!("loudness meter: {e:?}"))
            .ok();
        Self { inner }
    }

    pub fn add_stereo(&mut self, frames: &[f32]) {
        if let Some(m) = self.inner.as_mut() {
            if m.add_frames_f32(&frames[..frames.len() / 2 * 2]).is_err() {
                self.inner = None;
            }
        }
    }

    /// The level, unless it's silent (or couldn't be measured).
    pub fn finish(self) -> Option<Level> {
        let m = self.inner?;
        let lufs = m.loudness_global().ok().filter(|l| l.is_finite())?;
        let peak = (0..2).filter_map(|c| m.true_peak(c).ok()).fold(0.0f64, f64::max);
        (peak > 0.0).then(|| Level { lufs, true_peak_db: 20.0 * peak.log10() })
    }
}

/// The highest true peak a song is boosted to (dBTP): a little below full
/// scale, as resampling and time-stretching add some.
pub const PEAK_CEILING_DB: f64 = -1.0;
/// The most a song is turned up or down to reach the loudness (dB).
pub const MAX_LEVEL_DB: f64 = 12.0;

/// How much to change a song (dB) to reach `target` LUFS: its loudness's
/// difference, a boost capped so its true peak stays under `PEAK_CEILING_DB`.
pub fn level_gain_db(target: f64, lufs: f64, true_peak_db: Option<f64>) -> f64 {
    let mut g = target - lufs;
    if let Some(p) = true_peak_db {
        g = g.min((PEAK_CEILING_DB - p).max(0.0));
    }
    g.clamp(-MAX_LEVEL_DB, MAX_LEVEL_DB)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(amp: f32, secs: f32, rate: u32) -> Vec<f32> {
        (0..(secs * rate as f32) as usize)
            .flat_map(|i| {
                let s = amp * (i as f32 / rate as f32 * 1000.0 * std::f32::consts::TAU).sin();
                [s, s]
            })
            .collect()
    }

    #[test]
    fn measures_lufs_and_peak() {
        // A full-scale 1 kHz sine is -3.01 LUFS on one channel (R128's reference): on both, 0 LUFS.
        let mut m = Meter::new(48_000);
        for chunk in tone(1.0, 5.0, 48_000).chunks(4096) {
            m.add_stereo(chunk);
        }
        let l = m.finish().unwrap();
        assert!(l.lufs.abs() < 0.1, "{l:?}");
        assert!(l.true_peak_db.abs() < 0.5, "{l:?}");
        // Half the amplitude: 6 dB quieter.
        let mut m = Meter::new(48_000);
        m.add_stereo(&tone(0.5, 5.0, 48_000));
        assert!((m.finish().unwrap().lufs - -6.02).abs() < 0.1);
        assert_eq!(Meter::new(48_000).finish(), None, "silence has no level");
    }

    #[test]
    fn a_boost_keeps_the_peaks_clear() {
        // Loud enough: turned down, whatever its peak.
        assert_eq!(level_gain_db(-14.0, -10.0, Some(-0.1)), -4.0);
        // Quiet with room: boosted to the target.
        assert_eq!(level_gain_db(-14.0, -20.0, Some(-12.0)), 6.0);
        // Quiet but peaky: boosted only until the peak reaches the ceiling.
        assert_eq!(level_gain_db(-14.0, -20.0, Some(-4.0)), 3.0);
        // Already at the ceiling: not boosted.
        assert_eq!(level_gain_db(-14.0, -20.0, Some(0.5)), 0.0);
        // No peak known (older analysis): as before, within the limit.
        assert_eq!(level_gain_db(-14.0, -40.0, None), MAX_LEVEL_DB);
    }
}

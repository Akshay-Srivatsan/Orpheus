//! Audio fingerprints (Chromaprint), to recognise a particular recording, or
//! a particular edit of it, across encodings: an MP3 of a FLAC is the same
//! version; an edit with a section cut, a new intro or a tempo change is not.
//!
//! Fingerprints are written the way Chromaprint's `fpcalc` prints them
//! (compressed, URL-safe base64, "AQAD…"), so a club can make them with either
//! tool.

use anyhow::{anyhow, bail, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rusty_chromaprint::{Configuration, FingerprintCompressor, Fingerprinter};

/// Chromaprint's default algorithm (the one `fpcalc` and AcoustID use).
fn config() -> Configuration {
    Configuration::preset_test2()
}

/// Fingerprint of mono audio at `sr` Hz (the whole song).
pub fn compute(mono: &[f32], sr: u32) -> Result<Vec<u32>> {
    let mut fp = Fingerprinter::new(&config());
    fp.start(sr, 1).map_err(|e| anyhow!("{e:?}"))?;
    let samples: Vec<i16> = mono.iter().map(|&x| (x.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).collect();
    fp.consume(&samples);
    fp.finish();
    Ok(fp.fingerprint().to_vec())
}

/// The compact text form (as `fpcalc` prints it).
pub fn encode(fp: &[u32]) -> String {
    let c = config();
    URL_SAFE_NO_PAD.encode(FingerprintCompressor::from(&c).compress(fp))
}

/// Reads the compact text form back (from Orpheus or `fpcalc`).
pub fn decode(text: &str) -> Result<Vec<u32>> {
    let bytes = URL_SAFE_NO_PAD
        .decode(text.trim().trim_end_matches('='))
        .map_err(|_| anyhow!("not a fingerprint (expected Chromaprint's text form, \"AQAD…\")"))?;
    if bytes.len() < 4 {
        bail!("fingerprint too short");
    }
    let count = ((bytes[1] as usize) << 16) | ((bytes[2] as usize) << 8) | bytes[3] as usize;
    let body = &bytes[4..];
    // 3-bit "normal" values: for each sub-fingerprint, the gaps between its
    // set bits (7 = look up an exceptional value), ended by a 0.
    let mut bits = BitReader::new(body);
    let mut normal = Vec::new();
    let mut ends = 0;
    while ends < count {
        let v = bits.read(3).ok_or_else(|| anyhow!("fingerprint is cut short"))?;
        if v == 0 {
            ends += 1;
        }
        normal.push(v);
    }
    // Then 5-bit exceptional values, starting at the next byte.
    let mut exc = BitReader::new(&body[normal.len().saturating_mul(3).div_ceil(8)..]);
    let mut out = Vec::with_capacity(count);
    let (mut sub, mut last_bit, mut prev) = (0u32, 0u32, 0u32);
    for v in normal {
        if v == 0 {
            prev ^= sub;
            out.push(prev);
            sub = 0;
            last_bit = 0;
            continue;
        }
        let mut gap = v;
        if v == 7 {
            gap += exc.read(5).ok_or_else(|| anyhow!("fingerprint is cut short"))?;
        }
        last_bit += gap;
        if last_bit == 0 || last_bit > 32 {
            bail!("fingerprint is damaged");
        }
        sub |= 1 << (last_bit - 1);
    }
    Ok(out)
}

/// Reads little-endian packed n-bit values (Chromaprint's packing).
struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
    fn read(&mut self, n: usize) -> Option<u32> {
        let mut v = 0u32;
        for i in 0..n {
            let (byte, bit) = ((self.pos + i) / 8, (self.pos + i) % 8);
            let b = *self.data.get(byte)?;
            v |= (((b >> bit) & 1) as u32) << i;
        }
        self.pos += n;
        Some(v)
    }
}

/// Seconds per fingerprint value.
pub fn item_secs() -> f64 {
    config().item_duration_in_seconds() as f64
}

/// How closely two fingerprints line up: the longest stretch that matches
/// (as a share of the longer one) and its bit error (0 = identical, about
/// 0.5 = unrelated audio).
#[derive(Debug, Clone, Copy)]
pub struct Similarity {
    pub coverage: f64,
    pub error: f64,
}

/// Bit error below which audio counts as the same (re-encodes: 1–3 %;
/// unrelated or merely similar music: 20 % and up).
const SAME_AUDIO_ERROR: f64 = 0.12;
/// Frames averaged when deciding whether audio lines up.
const SMOOTH_FRAMES: usize = 16;
/// A version can start a little earlier or later (silence, a decoder's
/// padding): offsets searched (seconds).
const MAX_SHIFT_SECS: f64 = 10.0;
/// One stretch must cover this share of both songs …
pub const SAME_VERSION_COVERAGE: f64 = 0.95;
/// … at this bit error, …
pub const SAME_VERSION_ERROR: f64 = 0.05;
/// … and the lengths may differ by this much (seconds).
pub const SAME_VERSION_LENGTH_SECS: f64 = 3.0;

fn bit_error(a: u32, b: u32) -> f64 {
    (a ^ b).count_ones() as f64 / 32.0
}

pub fn similarity(a: &[u32], b: &[u32]) -> Similarity {
    let max_shift = (MAX_SHIFT_SECS / item_secs()) as isize;
    let longer = a.len().max(b.len()).max(1) as f64;
    let mut best = Similarity { coverage: 0.0, error: 1.0 };
    for off in -max_shift..=max_shift {
        let (i0, j0) = if off >= 0 { (off as usize, 0) } else { (0, (-off) as usize) };
        if i0 >= a.len() || j0 >= b.len() {
            continue;
        }
        let n = (a.len() - i0).min(b.len() - j0);
        let e: Vec<f64> = (0..n).map(|k| bit_error(a[i0 + k], b[j0 + k])).collect();
        // Longest run where the smoothed error says "same audio".
        let half = SMOOTH_FRAMES / 2;
        let mut sum: f64 = e.iter().take(SMOOTH_FRAMES.min(n)).sum();
        let (mut run, mut run_err, mut best_run, mut best_err) = (0usize, 0.0, 0usize, 1.0);
        for k in 0..n {
            let lo = k.saturating_sub(half);
            let hi = (k + half).min(n);
            if k > half && hi <= n && lo > 0 {
                sum += e[hi - 1] - e[lo - 1];
            } else {
                sum = e[lo..hi].iter().sum();
            }
            let smoothed = sum / (hi - lo) as f64;
            if smoothed < SAME_AUDIO_ERROR {
                run += 1;
                run_err += e[k];
                if run > best_run {
                    best_run = run;
                    best_err = run_err / run as f64;
                }
            } else {
                run = 0;
                run_err = 0.0;
            }
        }
        let cov = best_run as f64 / longer;
        if cov > best.coverage {
            best = Similarity { coverage: cov, error: best_err };
        }
    }
    best
}

/// Whether two fingerprints are the same version (re-encodes yes; edits no).
pub fn same_version(a: &[u32], b: &[u32]) -> bool {
    let len_diff = (a.len() as f64 - b.len() as f64).abs() * item_secs();
    if len_diff > SAME_VERSION_LENGTH_SECS {
        return false;
    }
    let s = similarity(a, b);
    s.coverage >= SAME_VERSION_COVERAGE && s.error < SAME_VERSION_ERROR
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tone sequence: a different chord every half second.
    fn music(secs: f64, sr: u32, seed: u32) -> Vec<f32> {
        let n = (secs * sr as f64) as usize;
        let mut s = seed;
        let mut freqs = [0.0f32; 3];
        (0..n)
            .map(|i| {
                if i % (sr as usize / 2) == 0 {
                    for f in freqs.iter_mut() {
                        s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                        *f = 110.0 * 2f32.powf((s >> 24) as f32 / 48.0);
                    }
                }
                let t = i as f32 / sr as f32;
                freqs.iter().map(|f| (t * f * std::f32::consts::TAU).sin()).sum::<f32>() * 0.25
            })
            .collect()
    }

    #[test]
    fn text_form_round_trips() {
        let fp = compute(&music(30.0, 22050, 1), 22050).unwrap();
        assert!(fp.len() > 100);
        let text = encode(&fp);
        // Algorithm 1 (Chromaprint's default); the next bytes are the length.
        assert!(text.starts_with("AQ"), "{}", &text[..8]);
        assert_eq!(decode(&text).unwrap(), fp);
        assert!(decode("not a fingerprint!").is_err());
    }

    #[test]
    fn same_audio_matches_and_edits_dont() {
        let sr = 22050;
        let song = music(60.0, sr, 7);
        let a = compute(&song, sr).unwrap();
        // The same audio, a little quieter and with added noise: a re-encode.
        let mut seed = 3u32;
        let noisy: Vec<f32> = song
            .iter()
            .map(|x| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                x * 0.8 + ((seed >> 16) as f32 / 65536.0 - 0.5) * 0.01
            })
            .collect();
        let b = compute(&noisy, sr).unwrap();
        assert!(same_version(&a, &b), "{:?}", similarity(&a, &b));
        // An edit: 15 s cut from the middle.
        let mut edit = song[..sr as usize * 20].to_vec();
        edit.extend_from_slice(&song[sr as usize * 35..]);
        let c = compute(&edit, sr).unwrap();
        assert!(!same_version(&a, &c), "{:?}", similarity(&a, &c));
        // A different song.
        let d = compute(&music(60.0, sr, 99), sr).unwrap();
        assert!(!same_version(&a, &d));
    }
}

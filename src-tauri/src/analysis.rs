//! Offline track analysis: BPM, colored waveform overview, loudness, silence.

use crate::store::Analysis;
use rustfft::{num_complex::Complex, FftPlanner};

/// Number of waveform bins stored per track; each bin is 4 bytes:
/// peak, low-band, mid-band, high-band (all 0..=255).
pub const WAVEFORM_BINS: usize = 1024;

pub fn analyze(mono: &[f32], sr: u32) -> Analysis {
    let duration_ms = (mono.len() as f64 / sr as f64 * 1000.0) as i64;
    let (start, end) = silence_bounds(mono, sr);
    Analysis {
        duration_ms,
        bpm: detect_bpm(mono, sr),
        loudness_db: loudness(mono, sr),
        audio_start_ms: start,
        audio_end_ms: end,
        waveform: waveform(mono, sr),
    }
}

/// Waveform overview with a rough 3-band split (like Serato/rekordbox colored
/// waveforms), computed with one-pole filters.
pub fn waveform(mono: &[f32], sr: u32) -> Vec<u8> {
    let n = mono.len();
    if n == 0 {
        return vec![];
    }
    let a_low = one_pole_coeff(250.0, sr);
    let a_high = one_pole_coeff(2500.0, sr);
    let mut lp_low = 0.0f32;
    let mut lp_high = 0.0f32;
    let mut out = Vec::with_capacity(WAVEFORM_BINS * 4);
    let mut bins = Vec::with_capacity(WAVEFORM_BINS);
    let per_bin = (n as f64 / WAVEFORM_BINS as f64).max(1.0);
    for b in 0..WAVEFORM_BINS {
        let s = (b as f64 * per_bin) as usize;
        let e = (((b + 1) as f64 * per_bin) as usize).min(n);
        let (mut peak, mut el, mut em, mut eh) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for &x in &mono[s.min(n)..e] {
            lp_low += a_low * (x - lp_low);
            lp_high += a_high * (x - lp_high);
            let low = lp_low;
            let mid = lp_high - lp_low;
            let high = x - lp_high;
            peak = peak.max(x.abs());
            el += low * low;
            em += mid * mid;
            eh += high * high;
        }
        let cnt = (e.saturating_sub(s)).max(1) as f32;
        bins.push([peak, (el / cnt).sqrt(), (em / cnt).sqrt(), (eh / cnt).sqrt()]);
    }
    let max_peak = bins.iter().map(|b| b[0]).fold(1e-6, f32::max);
    let max_band = bins.iter().map(|b| b[1].max(b[2]).max(b[3])).fold(1e-6, f32::max);
    for b in bins {
        out.push((b[0] / max_peak * 255.0) as u8);
        for v in &b[1..] {
            out.push((v / max_band * 255.0).min(255.0) as u8);
        }
    }
    out
}

fn one_pole_coeff(cutoff: f32, sr: u32) -> f32 {
    1.0 - (-2.0 * std::f32::consts::PI * cutoff / sr as f32).exp()
}

/// Approximate integrated loudness (LUFS-like, without K-weighting):
/// 400 ms blocks, absolute gate at -70 dB, relative gate at -10 dB.
pub fn loudness(mono: &[f32], sr: u32) -> Option<f64> {
    let block = (sr as f64 * 0.4) as usize;
    let hop = block / 4;
    if mono.len() < block {
        return None;
    }
    let mut powers = vec![];
    let mut i = 0;
    while i + block <= mono.len() {
        let p: f64 = mono[i..i + block].iter().map(|&x| (x as f64) * (x as f64)).sum::<f64>() / block as f64;
        powers.push(p);
        i += hop;
    }
    let to_db = |p: f64| -0.691 + 10.0 * p.max(1e-12).log10();
    let abs: Vec<f64> = powers.into_iter().filter(|&p| to_db(p) > -70.0).collect();
    if abs.is_empty() {
        return None;
    }
    let mean = abs.iter().sum::<f64>() / abs.len() as f64;
    let rel_gate = to_db(mean) - 10.0;
    let gated: Vec<f64> = abs.into_iter().filter(|&p| to_db(p) > rel_gate).collect();
    if gated.is_empty() {
        return None;
    }
    Some(to_db(gated.iter().sum::<f64>() / gated.len() as f64))
}

/// Leading/trailing silence (below -48 dBFS over 10 ms windows).
pub fn silence_bounds(mono: &[f32], sr: u32) -> (Option<i64>, Option<i64>) {
    let win = (sr as usize / 100).max(1);
    let thresh = 10f32.powf(-48.0 / 20.0);
    let windows: Vec<f32> = mono
        .chunks(win)
        .map(|c| (c.iter().map(|x| x * x).sum::<f32>() / c.len() as f32).sqrt())
        .collect();
    let first = windows.iter().position(|&r| r > thresh);
    let last = windows.iter().rposition(|&r| r > thresh);
    match (first, last) {
        (Some(f), Some(l)) => {
            let ms = |w: usize| (w * win) as i64 * 1000 / sr as i64;
            let start = ms(f);
            let end = ms((l + 1).min(windows.len()));
            (Some(start), Some(end))
        }
        _ => (None, None),
    }
}

const ANALYSIS_SR: f64 = 11025.0;
const HOP: usize = 128;
const FRAME: usize = 1024;

/// Onset-strength envelope (log spectral flux) at ANALYSIS_SR / HOP frames/s.
fn onset_envelope(mono: &[f32], sr: u32) -> (Vec<f32>, f64) {
    let factor = ((sr as f64 / ANALYSIS_SR).round() as usize).max(1);
    let ds: Vec<f32> = mono.chunks(factor).map(|c| c.iter().sum::<f32>() / c.len() as f32).collect();
    let rate = sr as f64 / factor as f64;
    let fps = rate / HOP as f64;
    if ds.len() < FRAME * 4 {
        return (vec![], fps);
    }
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FRAME);
    let window: Vec<f32> = (0..FRAME)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / FRAME as f32).cos())
        .collect();
    let max_bin = ((5000.0 / rate) * FRAME as f64) as usize;
    let max_bin = max_bin.clamp(16, FRAME / 2);
    let mut prev = vec![0.0f32; max_bin];
    let mut env = Vec::with_capacity(ds.len() / HOP);
    let mut buf = vec![Complex::new(0.0f32, 0.0); FRAME];
    let mut pos = 0;
    while pos + FRAME <= ds.len() {
        for i in 0..FRAME {
            buf[i] = Complex::new(ds[pos + i] * window[i], 0.0);
        }
        fft.process(&mut buf);
        let mut flux = 0.0f32;
        for k in 1..max_bin {
            let mag = (1.0 + 100.0 * buf[k].norm()).ln();
            let d = mag - prev[k];
            if d > 0.0 {
                flux += d;
            }
            prev[k] = mag;
        }
        env.push(flux);
        pos += HOP;
    }
    // Remove slowly varying trend (local mean over ~1 s) and half-wave rectify.
    let w = fps as usize;
    let mut out = vec![0.0f32; env.len()];
    let mut acc = 0.0f32;
    let mut q = std::collections::VecDeque::new();
    for i in 0..env.len() {
        acc += env[i];
        q.push_back(env[i]);
        if q.len() > w {
            acc -= q.pop_front().unwrap();
        }
        let mean = acc / q.len() as f32;
        out[i] = (env[i] - mean).max(0.0);
    }
    (out, fps)
}

fn autocorr(env: &[f32], max_lag: usize) -> Vec<f32> {
    let n = env.len();
    let mut ac = vec![0.0f32; max_lag + 1];
    for lag in 1..=max_lag.min(n.saturating_sub(1)) {
        let mut s = 0.0f32;
        for i in 0..n - lag {
            s += env[i] * env[i + lag];
        }
        ac[lag] = s / (n - lag) as f32;
    }
    ac
}

fn interp(ac: &[f32], x: f64) -> f32 {
    let i = x.floor() as usize;
    if i + 1 >= ac.len() {
        return 0.0;
    }
    let f = (x - i as f64) as f32;
    ac[i] * (1.0 - f) + ac[i + 1] * f
}

/// Global tempo estimate in BPM (beats folded into 60..=200).
pub fn detect_bpm(mono: &[f32], sr: u32) -> Option<f64> {
    let (env, fps) = onset_envelope(mono, sr);
    if env.len() < (fps * 10.0) as usize {
        return None;
    }
    let min_bpm = 50.0;
    let max_bpm = 220.0;
    let max_lag = (fps * 60.0 / min_bpm * 4.0).ceil() as usize + 2;
    let ac = autocorr(&env, max_lag);
    let lag_for = |bpm: f64| fps * 60.0 / bpm;

    // Score candidate tempos on a fine grid: comb over beat multiples with a
    // log-normal prior centred on 120 BPM to resolve octave ambiguity.
    let mut best = (0.0f64, f64::MIN);
    let mut bpm = min_bpm;
    while bpm <= max_bpm {
        let lag = lag_for(bpm);
        let mut s = 0.0f32;
        for k in 1..=4 {
            s += interp(&ac, lag * k as f64) / k as f32;
        }
        let prior = (-0.5 * ((bpm / 120.0).log2() / 0.9).powi(2)).exp();
        let score = s as f64 * prior;
        if score > best.1 {
            best = (bpm, score);
        }
        bpm += 0.05;
    }
    if best.1 <= 0.0 {
        return None;
    }
    let mut result = best.0;
    while result < 60.0 {
        result *= 2.0;
    }
    while result > 200.0 {
        result /= 2.0;
    }
    Some((result * 10.0).round() / 10.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn click_track(bpm: f64, secs: f64, sr: u32) -> Vec<f32> {
        let n = (secs * sr as f64) as usize;
        let mut v = vec![0.0f32; n];
        let period = 60.0 / bpm * sr as f64;
        let mut t = 0.0;
        while (t as usize) < n {
            let s = t as usize;
            for i in 0..400.min(n - s) {
                let env = (-(i as f32) / 80.0).exp();
                v[s + i] += env * (i as f32 * 0.3).sin() * 0.8;
            }
            t += period;
        }
        v
    }

    #[test]
    fn detects_click_tempos() {
        for &bpm in &[96.0, 118.0, 128.0, 150.0] {
            let audio = click_track(bpm, 40.0, 44100);
            let got = detect_bpm(&audio, 44100).unwrap();
            let ok = [got, got * 2.0, got / 2.0].iter().any(|g| (g - bpm).abs() < 1.0);
            assert!(ok, "expected {bpm}, got {got}");
        }
    }

    #[test]
    fn silence_and_waveform() {
        let sr = 44100;
        let mut audio = vec![0.0f32; sr as usize];
        audio.extend(click_track(120.0, 5.0, sr));
        audio.extend(vec![0.0f32; sr as usize]);
        let (s, e) = silence_bounds(&audio, sr);
        assert!((s.unwrap() - 1000).abs() < 20, "start {s:?}");
        assert!(e.unwrap() > 5500 && e.unwrap() < 6100, "end {e:?}");
        let w = waveform(&audio, sr);
        assert_eq!(w.len(), WAVEFORM_BINS * 4);
        assert!(loudness(&audio, sr).unwrap() < 0.0);
    }
}

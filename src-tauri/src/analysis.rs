//! Offline track analysis: BPM, meter, colored waveform overview, loudness, silence.

use crate::store::Analysis;
use rustfft::{num_complex::Complex, FftPlanner};

/// Bumped when analysis gains results (2: meter); older tracks are re-analysed.
pub const VERSION: u32 = 2;

/// Number of waveform bins stored per track; each bin is 4 bytes:
/// peak, low-band, mid-band, high-band (all 0..=255).
pub const WAVEFORM_BINS: usize = 1024;

/// `known_bpm`: a tempo the DJ set (or from tags); the meter is detected on
/// that beat grid rather than the detected one.
pub fn analyze(mono: &[f32], sr: u32, known_bpm: Option<f64>) -> Analysis {
    let duration_ms = (mono.len() as f64 / sr as f64 * 1000.0) as i64;
    let (start, end) = silence_bounds(mono, sr);
    let detected = detect_bpm(mono, sr);
    let meter = known_bpm.or(detected).and_then(|b| detect_meter(mono, sr, b));
    // A detected tempo at 2/3 of a triple song's beat is corrected.
    let bpm = match (known_bpm, meter) {
        (None, Some(m)) => Some(m.bpm),
        _ => detected,
    };
    Analysis {
        duration_ms,
        meter: meter.map(|m| m.beats_per_bar),
        bpm,
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
    onset_envelope_with(mono, sr, &Envelope { max_hz: 5000.0, ..Envelope::BPM })
}

/// How an onset envelope is computed.
struct Envelope {
    min_hz: f64,
    max_hz: f64,
    /// Magnitude scale before log compression.
    scale: f32,
    /// Remove the trend with a centred (vs trailing) one-second mean.
    centred: bool,
}

impl Envelope {
    /// As used for BPM detection.
    const BPM: Envelope = Envelope { min_hz: 0.0, max_hz: 5000.0, scale: 1.0, centred: false };
    /// For meter detection: normalised spectrum (closer to linear before the
    /// log), centred trend removal; matches the prototype it was tuned with.
    const METER: Envelope = Envelope { min_hz: 30.0, max_hz: 5000.0, scale: 2.0 / FRAME as f32, centred: true };
}

/// Onset envelope of the spectrum below `max_hz` (e.g. the bass, for accents), for meter detection.
fn onset_envelope_band(mono: &[f32], sr: u32, max_hz: f64) -> (Vec<f32>, f64) {
    onset_envelope_with(mono, sr, &Envelope { max_hz, ..Envelope::METER })
}

fn onset_envelope_with(mono: &[f32], sr: u32, cfg: &Envelope) -> (Vec<f32>, f64) {
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
    let max_bin = ((cfg.max_hz / rate) * FRAME as f64) as usize;
    let max_bin = max_bin.clamp(4, FRAME / 2);
    let min_bin = (((cfg.min_hz / rate) * FRAME as f64).ceil() as usize).max(1);
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
        for k in min_bin..max_bin {
            let mag = (1.0 + 100.0 * cfg.scale * buf[k].norm()).ln();
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
    if cfg.centred {
        let prefix: Vec<f64> = std::iter::once(0.0).chain(env.iter().scan(0.0f64, |a, &x| { *a += x as f64; Some(*a) })).collect();
        for i in 0..env.len() {
            let (a, b) = (i.saturating_sub(w / 2), (i + w - w / 2).min(env.len()));
            let mean = ((prefix[b] - prefix[a]) / w as f64) as f32;
            out[i] = (env[i] - mean).max(0.0);
        }
        return (out, fps);
    }
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

/// Beat times (envelope frames) by dynamic programming (Ellis 2007): beats
/// sit on strong onsets and are spaced close to `period` frames, so the beat
/// grid can follow small tempo drifts.
fn track_beats(env: &[f32], period: f64) -> Vec<usize> {
    let n = env.len();
    if n < (period * 8.0) as usize || period < 2.0 {
        return vec![];
    }
    let mean = env.iter().sum::<f32>() / n as f32;
    let std = (env.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / n as f32).sqrt().max(1e-9);
    let local: Vec<f64> = env.iter().map(|&x| (x / std) as f64).collect();
    // Strong preference for the given tempo.
    const TIGHTNESS: f64 = 100.0;
    let (lo, hi) = ((period * 0.5).round() as usize, (period * 2.0).round() as usize);
    let mut score = local.clone();
    let mut back = vec![usize::MAX; n];
    for t in lo..n {
        let mut best = (f64::MIN, usize::MAX);
        for gap in lo..=hi.min(t) {
            let tau = t - gap;
            let penalty = TIGHTNESS * ((gap as f64 / period).ln()).powi(2);
            let v = score[tau] - penalty;
            if v > best.0 {
                best = (v, tau);
            }
        }
        if best.1 != usize::MAX && best.0 > 0.0 {
            score[t] = local[t] + best.0;
            back[t] = best.1;
        }
    }
    // Start from the best-scoring frame within the last beat period.
    let tail = n.saturating_sub(period.ceil() as usize);
    let mut t = (tail..n).max_by(|&a, &b| score[a].total_cmp(&score[b])).unwrap_or(n - 1);
    let mut beats = vec![t];
    while back[t] != usize::MAX {
        t = back[t];
        beats.push(t);
    }
    beats.reverse();
    beats
}

/// Strength of each beat: the envelope's peak near it.
fn beat_strengths(env: &[f32], beats: &[usize], radius: usize) -> Vec<f64> {
    beats
        .iter()
        .map(|&b| env[b.saturating_sub(radius)..(b + radius + 1).min(env.len())].iter().fold(0.0f32, |a, &x| a.max(x)) as f64)
        .collect()
}

/// Normalized autocorrelation of a sequence at `lag` (mean removed).
fn seq_autocorr(x: &[f64], lag: usize) -> f64 {
    if x.len() <= lag * 4 {
        return 0.0;
    }
    let m = x.iter().sum::<f64>() / x.len() as f64;
    let var: f64 = x.iter().map(|v| (v - m).powi(2)).sum::<f64>() / x.len() as f64;
    if var <= 1e-12 {
        return 0.0;
    }
    let n = x.len() - lag;
    (0..n).map(|i| (x[i] - m) * (x[i + lag] - m)).sum::<f64>() / n as f64 / var
}

/// Detected meter, and the beat tempo it implies.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Meter {
    /// 3 (triple: waltzes, 6/8 counted in eighths) or 4 (duple: 2/4 and 4/4
    /// count alike for dancing).
    pub beats_per_bar: u8,
    /// The beat tempo: `bpm`, or 1.5x it when the tempo was detected at 2/3 of
    /// a triple song's beat (common in 6/8).
    pub bpm: f64,
}

/// Clear evidence threshold for both measures below.
const METER_MARGIN: f64 = 0.15;

/// Meter of a song, or `None` when unclear. Two measures are combined:
///
/// - *Pulse grouping* (tempo-free): in the onset autocorrelation, find the
///   fastest strong pulse and compare its periodicities at 3x/6x against
///   2x/4x. Robust when the detected tempo is at the wrong level.
/// - *Accent pattern* on a beat grid tracked at `bpm`: do bass and onset
///   accents repeat every 3 (and 6) beats, or every 2 (and 4)?
///
/// The pulse grouping decides when clear; otherwise the accent pattern does.
/// Tuned on a library of dance music (waltzes, 6/8 ballads, polkas, line
/// dances); ambiguous songs are left unknown for the DJ to set.
pub fn detect_meter(mono: &[f32], sr: u32, bpm: f64) -> Option<Meter> {
    let (full, fps) = onset_envelope_band(mono, sr, 5000.0);
    let (bass, _) = onset_envelope_band(mono, sr, 200.0);
    let n = full.len().min(bass.len());
    if n < (fps * 20.0) as usize {
        return None;
    }
    let (full, bass) = (&full[..n], &bass[..n]);
    let grouping = pulse_grouping(full, bass, fps);
    let accent = accent_pattern(full, bass, fps, bpm);
    log::debug!("meter at {bpm:.1} BPM: grouping {grouping:?}, accent {accent:?}");
    let beats_per_bar = match (grouping, accent) {
        (Some(g), _) if g >= METER_MARGIN => 3,
        (Some(g), _) if g <= -METER_MARGIN => 4,
        (_, Some(a)) if a.abs() >= METER_MARGIN => if a > 0.0 { 3 } else { 4 },
        _ => return None,
    };
    let mut beat = bpm;
    // Triple, but the detected grid looks duple: the tempo is probably 2/3 of the beat.
    if beats_per_bar == 3 && accent.is_some_and(|a| a <= -METER_MARGIN) && bpm * 1.5 <= 220.0 {
        if accent_pattern(full, bass, fps, bpm * 1.5).is_some_and(|a| a >= METER_MARGIN) {
            beat = bpm * 1.5;
        }
    }
    Some(Meter { beats_per_bar, bpm: (beat * 10.0).round() / 10.0 })
}

/// Tempo-free triple (+) vs duple (-) evidence from the onset autocorrelation.
fn pulse_grouping(full: &[f32], bass: &[f32], fps: f64) -> Option<f64> {
    let env: Vec<f32> = full.iter().zip(bass).map(|(a, b)| a + b).collect();
    let mean = env.iter().sum::<f32>() / env.len() as f32;
    let env: Vec<f32> = env.iter().map(|x| x - mean).collect();
    let max_lag = (fps * 4.5) as usize;
    let mut ac = autocorr(&env, max_lag);
    let zero = env.iter().map(|x| x * x).sum::<f32>() / env.len() as f32;
    if zero <= 0.0 {
        return None;
    }
    ac.iter_mut().for_each(|v| *v /= zero);
    let (lo, hi) = ((fps * 0.15) as usize, (fps * 0.5) as usize);
    let peaks: Vec<usize> = (lo.max(1)..hi.min(ac.len() - 1)).filter(|&i| ac[i] > ac[i - 1] && ac[i] >= ac[i + 1]).collect();
    let top = peaks.iter().map(|&p| ac[p]).fold(f32::MIN, f32::max);
    if peaks.is_empty() || top <= 0.0 {
        return None;
    }
    // The fastest strong pulse.
    let tatum = *peaks.iter().find(|&&p| ac[p] >= 0.6 * top)? as f64;
    let at = |m: f64| -> f64 {
        let x = (tatum * m) as usize;
        let w = ((tatum * 0.08) as usize).max(1);
        if x + 1 >= ac.len() {
            return 0.0;
        }
        ac[x.saturating_sub(w)..(x + w + 2).min(ac.len())].iter().fold(f32::MIN, |a, &b| a.max(b)) as f64
    };
    Some((at(3.0) + at(6.0)) / 2.0 - (at(2.0) + at(4.0)) / 2.0)
}

/// Triple (+) vs duple (-) accent evidence on a beat grid at `bpm`.
fn accent_pattern(full: &[f32], bass: &[f32], fps: f64, bpm: f64) -> Option<f64> {
    let period = fps * 60.0 / bpm;
    let beats = track_beats(full, period);
    if beats.len() < 32 {
        return None;
    }
    let radius = (period * 0.15).round() as usize;
    let mut score = 0.0;
    for (env, weight) in [(bass, 0.6), (full, 0.4)] {
        let s = beat_strengths(env, &beats, radius);
        let r = |lag| seq_autocorr(&s, lag);
        score += weight * ((r(3) + r(6)) / 2.0 - (r(2) + r(4)) / 2.0);
    }
    Some(score)
}

/// (pulse grouping, accent pattern) evidence, for developer tools.
pub fn meter_evidence(mono: &[f32], sr: u32, bpm: f64) -> (Option<f64>, Option<f64>) {
    let (full, fps) = onset_envelope_band(mono, sr, 5000.0);
    let (bass, _) = onset_envelope_band(mono, sr, 200.0);
    let n = full.len().min(bass.len());
    (pulse_grouping(&full[..n], &bass[..n], fps), accent_pattern(&full[..n], &bass[..n], fps, bpm))
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

    /// Clicks with a bass "oom" on the first beat of every bar of `beats`.
    fn accented(bpm: f64, beats: usize, secs: f64, sr: u32) -> Vec<f32> {
        let n = (secs * sr as f64) as usize;
        let mut v = vec![0.0f32; n];
        let period = 60.0 / bpm * sr as f64;
        let mut k = 0;
        loop {
            let s = (k as f64 * period) as usize;
            if s >= n {
                break;
            }
            let down = k % beats == 0;
            for i in 0..4000.min(n - s) {
                let t = i as f32 / sr as f32;
                // Hi click on every beat; a low thump on downbeats.
                let mut x = (-(i as f32) / 80.0).exp() * (t * 3000.0 * std::f32::consts::TAU).sin() * 0.4;
                if down {
                    x += (-t / 0.08).exp() * (t * 60.0 * std::f32::consts::TAU).sin() * 0.9;
                }
                v[s + i] += x;
            }
            k += 1;
        }
        v
    }

    #[test]
    fn detects_meter() {
        let sr = 44100;
        for &(bpm, beats, want) in &[(150.0, 3, 3), (110.0, 3, 3), (120.0, 4, 4), (128.0, 2, 4)] {
            let audio = accented(bpm, beats, 60.0, sr);
            let got = detect_meter(&audio, sr, bpm).map(|m| m.beats_per_bar);
            assert_eq!(got, Some(want), "{bpm} BPM, {beats} beats per bar");
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

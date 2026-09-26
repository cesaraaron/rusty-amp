//! Offline measurement helpers for the fidelity harness, tests, and examples.
//!
//! Nothing here runs on the audio thread: these functions may allocate and are
//! written for clarity, not realtime. Every function takes the sample rate `sr`
//! explicitly so a corpus or render at any rate can be measured.

// The BS.1770 filter constants below are transcribed verbatim from the
// reference implementation; their digit grouping is not our choice.
#![allow(clippy::inconsistent_digit_grouping)]

use crate::dsp::biquad::Biquad;

/// Decibels for a linear amplitude, floored so silence maps to a finite number.
pub fn db(x: f32) -> f32 {
    20.0 * x.abs().max(1e-12).log10()
}

/// Root-mean-square of a signal (0 for empty input).
pub fn rms(s: &[f32]) -> f32 {
    if s.is_empty() {
        return 0.0;
    }
    (s.iter().map(|&x| x * x).sum::<f32>() / s.len() as f32).sqrt()
}

/// Absolute sample peak.
pub fn peak(s: &[f32]) -> f32 {
    s.iter().fold(0.0f32, |m, &x| m.max(x.abs()))
}

/// Crest factor in dB: peak minus RMS.
pub fn crest_db(s: &[f32]) -> f32 {
    db(peak(s)) - db(rms(s))
}

/// Goertzel single-bin amplitude estimate at `f`.
pub fn goertzel(s: &[f32], f: f32, sr: f32) -> f32 {
    if s.is_empty() {
        return 0.0;
    }
    let w = 2.0 * std::f32::consts::PI * f / sr;
    let c = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f32, 0.0f32);
    for &x in s {
        let s0 = x + c * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    let re = s1 - s2 * w.cos();
    let im = s2 * w.sin();
    (re * re + im * im).sqrt() / (s.len() as f32 / 2.0)
}

/// Long-term average spectrum on a third-octave grid (70 Hz–8 kHz), normalized
/// to its own mean so it measures tone, not level. Each band integrates energy
/// over a semitone grid of probe bins — a single centre bin would land on or
/// between note partials and swing with bin alignment rather than tone.
pub fn ltas_third_octave(s: &[f32], sr: f32) -> Vec<(f32, f32)> {
    let mut f = 70.0f32;
    let mut rows = Vec::new();
    while f < 8000.0 {
        let hi = f * 2.0_f32.powf(1.0 / 3.0);
        let mut probe = f;
        let mut e = 0.0f32;
        while probe < hi {
            e += goertzel(s, probe, sr).powi(2);
            probe *= 2.0_f32.powf(1.0 / 12.0);
        }
        rows.push((f, 10.0 * e.max(1e-18).log10()));
        f = hi;
    }
    if rows.is_empty() {
        return rows;
    }
    let mean = rows.iter().map(|r| r.1).sum::<f32>() / rows.len() as f32;
    rows.into_iter().map(|(f, v)| (f, v - mean)).collect()
}

/// Broadband envelope: `abs` through a one-pole low-pass with the given time
/// constant in milliseconds.
pub fn envelope(s: &[f32], sr: f32, ms: f32) -> Vec<f32> {
    let c = 1.0 - (-1.0 / (sr * ms / 1000.0)).exp();
    let mut env = 0.0f32;
    s.iter()
        .map(|&x| {
            env += c * (x.abs() - env);
            env
        })
        .collect()
}

/// Percentile from a pre-sorted slice; `p` in 0..=1.
pub fn percentile(sorted: &[f32], p: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    sorted[((sorted.len() - 1) as f32 * p.clamp(0.0, 1.0)) as usize]
}

/// Treble modulation depth: envelope std/mean of the 1.5–4 kHz band — how much
/// the top breathes with the playing (intermodulation/growl under a performance).
pub fn treble_mod_depth(s: &[f32], sr: f32) -> f32 {
    let mut hp = Biquad::highpass(sr, 1500.0, 0.707);
    let mut lp = Biquad::lowpass(sr, 4000.0, 0.707);
    let band: Vec<f32> = s.iter().map(|&x| lp.process(hp.process(x))).collect();
    let env = envelope(&band, sr, 8.0);
    let mean = env.iter().sum::<f32>() / env.len() as f32;
    let var = env.iter().map(|&e| (e - mean).powi(2)).sum::<f32>() / env.len() as f32;
    var.sqrt() / mean.max(1e-9)
}

/// Approximate spectral centroid (Hz) from a linearly spaced Goertzel probe
/// bank up to `min(sr/2, 10 kHz)`.
pub fn spectral_centroid(s: &[f32], sr: f32) -> f32 {
    if s.is_empty() {
        return 0.0;
    }
    let top = (sr * 0.5).min(10_000.0);
    let df = 50.0f32;
    let (mut num, mut den) = (0.0f32, 0.0f32);
    let mut f = df;
    while f < top {
        let e = goertzel(s, f, sr).powi(2);
        num += f * e;
        den += e;
        f += df;
    }
    if den <= 1e-20 { 0.0 } else { num / den }
}

/// Pearson correlation of two channels: 1.0 identical, −1.0 inverted, 0.0 for
/// silence or no length overlap.
pub fn stereo_correlation(l: &[f32], r: &[f32]) -> f32 {
    let n = l.len().min(r.len());
    if n == 0 {
        return 0.0;
    }
    let (mut ml, mut mr) = (0.0f32, 0.0f32);
    for i in 0..n {
        ml += l[i];
        mr += r[i];
    }
    ml /= n as f32;
    mr /= n as f32;
    let (mut num, mut dl, mut dr) = (0.0f32, 0.0f32, 0.0f32);
    for i in 0..n {
        let a = l[i] - ml;
        let b = r[i] - mr;
        num += a * b;
        dl += a * a;
        dr += b * b;
    }
    let den = (dl * dr).sqrt();
    if den <= f32::EPSILON { 0.0 } else { num / den }
}

/// Side-to-mid energy ratio in dB (clamped at −120 dB).
pub fn side_to_mid_db(l: &[f32], r: &[f32]) -> f32 {
    let n = l.len().min(r.len());
    if n == 0 {
        return -120.0;
    }
    let (mut mid, mut side) = (0.0f32, 0.0f32);
    for i in 0..n {
        let m = 0.5 * (l[i] + r[i]);
        let s = 0.5 * (l[i] - r[i]);
        mid += m * m;
        side += s * s;
    }
    let mid = mid / n as f32;
    let side = side / n as f32;
    if mid <= 1e-20 {
        return -120.0;
    }
    (10.0 * (side / mid).max(1e-12).log10()).max(-120.0)
}

/// Noise floor: mean RMS of the quietest 5 % of 50 ms windows.
pub fn noise_floor_db(s: &[f32], sr: f32) -> f32 {
    let win = (sr * 0.050).round() as usize;
    if win == 0 || s.len() < win {
        return db(rms(s));
    }
    let mut wins: Vec<f32> = s.chunks(win).map(rms).collect();
    wins.sort_by(f32::total_cmp);
    let k = ((wins.len() as f32 * 0.05).ceil() as usize).clamp(1, wins.len());
    let mean_e = wins[..k].iter().map(|r| r * r).sum::<f32>() / k as f32;
    db(mean_e.sqrt())
}

/// Peak of each non-overlapping window of `ms` milliseconds.
pub fn window_peaks(s: &[f32], sr: f32, ms: f32) -> Vec<f32> {
    let win = ((sr * ms / 1000.0).round() as usize).max(1);
    s.chunks(win)
        .map(|w| w.iter().fold(0.0f32, |m, &x| m.max(x.abs())))
        .collect()
}

// ── Loudness (ITU-R BS.1770-4) ─────────────────────────────────────────────────

/// A transposed-direct-form-II biquad built directly from coefficients.
#[derive(Clone, Copy, Default)]
struct Biquad2 {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad2 {
    fn new(b0: f32, b1: f32, b2: f32, a1: f32, a2: f32) -> Self {
        Self {
            b0,
            b1,
            b2,
            a1,
            a2,
            z1: 0.0,
            z2: 0.0,
        }
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// BS.1770 pre-filter stage 1: high shelf, from the analog-prototype
/// parameters used by libebur128/pyloudnorm (Q ≈ 0.7072 at 1681.97 Hz,
/// +3.9998 dB). `Biquad::high_shelf` is deliberately not reused: it has no Q.
fn k_high_shelf(sr: f32) -> Biquad2 {
    let f0 = 1681.974_450_955_533_f64;
    let g = 3.999_843_853_973_347_f64;
    let q = 0.707_175_236_955_419_6_f64;
    let k = (std::f64::consts::PI * f0 / sr as f64).tan();
    let vh = 10f64.powf(g / 20.0);
    let vb = vh.powf(0.499_666_774_154_541_6);
    let a0 = 1.0 + k / q + k * k;
    Biquad2::new(
        ((vh + vb * k / q + k * k) / a0) as f32,
        (2.0 * (k * k - vh) / a0) as f32,
        ((vh - vb * k / q + k * k) / a0) as f32,
        (2.0 * (k * k - 1.0) / a0) as f32,
        ((1.0 - k / q + k * k) / a0) as f32,
    )
}

/// BS.1770 pre-filter stage 2: RLB high-pass (Q ≈ 0.5003 at 38.135 Hz).
fn k_high_pass(sr: f32) -> Biquad2 {
    let f0 = 38.135_470_876_024_44_f64;
    let q = 0.500_327_037_323_877_3_f64;
    let k = (std::f64::consts::PI * f0 / sr as f64).tan();
    let a0 = 1.0 + k / q + k * k;
    Biquad2::new(
        1.0,
        -2.0,
        1.0,
        (2.0 * (k * k - 1.0) / a0) as f32,
        ((1.0 - k / q + k * k) / a0) as f32,
    )
}

/// Apply the two-stage K-weighting filter to a channel.
fn k_weight(s: &[f32], sr: f32) -> Vec<f32> {
    let mut shelf = k_high_shelf(sr);
    let mut hp = k_high_pass(sr);
    s.iter().map(|&x| hp.process(shelf.process(x))).collect()
}

/// Mean-square sum across the (already K-weighted) channels over one window.
fn window_z(l: &[f32], r: &[f32], start: usize, len: usize) -> f32 {
    let mut z = 0.0f32;
    for &x in &l[start..start + len] {
        z += x * x;
    }
    for &x in &r[start..start + len] {
        z += x * x;
    }
    z / len as f32
}

/// Loudness in LUFS of one window's mean-square sum.
fn loudness(z: f32) -> f32 {
    if z > 0.0 {
        -0.691 + 10.0 * z.log10()
    } else {
        f32::NEG_INFINITY
    }
}

/// Integrated loudness (LUFS) per ITU-R BS.1770-4: 400 ms blocks with 75 %
/// overlap, absolute gate at −70 LUFS, relative gate at −10 LU.
pub fn lufs_integrated(l: &[f32], r: &[f32], sr: f32) -> f32 {
    let n = l.len().min(r.len());
    let block = (sr * 0.400).round() as usize;
    let hop = (sr * 0.100).round() as usize;
    if n < block || block == 0 || hop == 0 {
        return f32::NEG_INFINITY;
    }
    let fl = k_weight(&l[..n], sr);
    let fr = k_weight(&r[..n], sr);

    let mut z: Vec<f32> = Vec::new();
    let mut start = 0;
    while start + block <= n {
        z.push(window_z(&fl, &fr, start, block));
        start += hop;
    }
    let absolute: Vec<f32> = z
        .iter()
        .copied()
        .filter(|&zi| loudness(zi) > -70.0)
        .collect();
    if absolute.is_empty() {
        return f32::NEG_INFINITY;
    }
    let z_abs = absolute.iter().sum::<f32>() / absolute.len() as f32;
    let relative_gate = loudness(z_abs) - 10.0;
    let relative: Vec<f32> = absolute
        .iter()
        .copied()
        .filter(|&zi| loudness(zi) > relative_gate)
        .collect();
    if relative.is_empty() {
        return f32::NEG_INFINITY;
    }
    loudness(relative.iter().sum::<f32>() / relative.len() as f32)
}

/// Maximum short-term loudness (3 s windows, 1 s step), in LUFS.
pub fn lufs_short_term_max(l: &[f32], r: &[f32], sr: f32) -> f32 {
    let n = l.len().min(r.len());
    let win = (sr * 3.0).round() as usize;
    let hop = (sr * 1.0).round() as usize;
    if n < win || win == 0 || hop == 0 {
        return f32::NEG_INFINITY;
    }
    let fl = k_weight(&l[..n], sr);
    let fr = k_weight(&r[..n], sr);
    let mut best = f32::NEG_INFINITY;
    let mut start = 0;
    while start + win <= n {
        let lufs = loudness(window_z(&fl, &fr, start, win));
        if lufs > best {
            best = lufs;
        }
        start += hop;
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn sine(freq: f32, amp: f32, secs: f32, sr: f32) -> Vec<f32> {
        let n = (sr * secs) as usize;
        (0..n)
            .map(|i| amp * (2.0 * PI * freq * i as f32 / sr).sin())
            .collect()
    }

    /// BS.1770-4 calibration: a 997 Hz sine at full scale in both channels reads
    /// 0 LUFS; one channel reads −3.01 LUFS. Holds at 44.1/48/96 kHz.
    #[test]
    fn lufs_matches_bs1770_reference() {
        for sr in [44_100.0f32, 48_000.0, 96_000.0] {
            let both = sine(997.0, 1.0, 10.0, sr);
            let l = both.clone();
            let r = both;
            let stereo = lufs_integrated(&l, &r, sr);
            assert!(
                (stereo - 0.0).abs() < 0.1,
                "stereo 997 Hz @ {sr} Hz = {stereo:.3} LUFS (want 0.0 ± 0.1)"
            );
            let zero = vec![0.0f32; l.len()];
            let mono = lufs_integrated(&l, &zero, sr);
            assert!(
                (mono + 3.01).abs() < 0.1,
                "mono 997 Hz @ {sr} Hz = {mono:.3} LUFS (want -3.01 ± 0.1)"
            );
        }
    }

    #[test]
    fn stereo_correlation_extremes() {
        let s: Vec<f32> = (0..1000).map(|i| (i as f32 * 0.01).sin()).collect();
        let inv: Vec<f32> = s.iter().map(|&x| -x).collect();
        assert!((stereo_correlation(&s, &s) - 1.0).abs() < 1e-6);
        assert!((stereo_correlation(&s, &inv) + 1.0).abs() < 1e-6);
        let silence = vec![0.0f32; 1000];
        assert_eq!(stereo_correlation(&silence, &silence), 0.0);
    }

    #[test]
    fn crest_of_a_sine_is_3db() {
        let s = sine(440.0, 0.5, 1.0, 48_000.0);
        assert!((crest_db(&s) - 3.01).abs() < 0.05, "{}", crest_db(&s));
    }

    #[test]
    fn spectral_centroid_of_a_sine_tracks_its_pitch() {
        let s = sine(1000.0, 0.5, 0.25, 48_000.0);
        let c = spectral_centroid(&s, 48_000.0);
        assert!((c - 1000.0).abs() < 120.0, "centroid {c} Hz");
    }
}

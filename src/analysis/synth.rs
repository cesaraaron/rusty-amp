//! Deterministic synthetic guitar DI performances (Karplus-Strong).
//!
//! Offline only. Every phrase is peak-normalized to the humbucker reference
//! level ([`HUMBUCKER_PEAK`], −3 dBFS) so the synthetic corpus is "calibrated"
//! by construction and reproducible across machines.

/// Reference peak for a humbucker-class DI: −3 dBFS.
pub const HUMBUCKER_PEAK: f32 = 0.7079458;

const E2: f32 = 82.41;
const A2: f32 = 110.0;
const D3: f32 = 146.83;
const G3: f32 = 196.0;
const E5_CHORD: [f32; 3] = [82.41, 123.47, 164.81];

/// A phrase the harness can render. Each maps to a distinct playing gesture so
/// amp/preset changes can be judged across clean, edge-of-breakup and lead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phrase {
    /// Palm-muted low-E chugs.
    Chugs,
    /// A dense run of palm mutes.
    PalmMutes,
    /// An open power chord ringing out.
    Chords,
    /// A single-note lick up the neck.
    Lick,
    /// Open strings picked as an arpeggio, at a clean level.
    CleanArpeggio,
    /// Notes held for ~2 s (sustain/decay).
    SustainedLead,
    /// The same figure at −12, −6 and 0 dB (edge-of-breakup).
    Dynamics,
}

impl Phrase {
    /// Every phrase, for corpus iteration.
    pub const ALL: [Phrase; 7] = [
        Phrase::Chugs,
        Phrase::PalmMutes,
        Phrase::Chords,
        Phrase::Lick,
        Phrase::CleanArpeggio,
        Phrase::SustainedLead,
        Phrase::Dynamics,
    ];

    /// Stable lowercase name used in report keys and file names.
    pub fn name(self) -> &'static str {
        match self {
            Phrase::Chugs => "chugs",
            Phrase::PalmMutes => "palm_mutes",
            Phrase::Chords => "chords",
            Phrase::Lick => "lick",
            Phrase::CleanArpeggio => "clean_arpeggio",
            Phrase::SustainedLead => "sustained_lead",
            Phrase::Dynamics => "dynamics",
        }
    }
}

/// Small deterministic LCG for the Karplus-Strong noise burst.
struct Lcg(u32);

impl Lcg {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / (1 << 24) as f32 * 2.0 - 1.0
    }
}

/// Pluck one string into `out` at `t0` seconds. `mute` raises the loop damping
/// for the palm-muted chug sound.
#[allow(clippy::too_many_arguments)]
fn pluck(out: &mut [f32], t0: f32, f: f32, dur: f32, amp: f32, mute: bool, seed: u32, sr: f32) {
    let period = ((sr / f) as usize).max(2);
    let mut buf: Vec<f32> = Vec::with_capacity(period);
    let mut rng = Lcg(seed);
    for _ in 0..period {
        buf.push(rng.next());
    }
    let damp = if mute { 0.955 } else { 0.9965 };
    let start = (t0 * sr) as usize;
    let n = (dur * sr) as usize;
    let mut idx = 0usize;
    let mut prev = 0.0f32;
    for i in 0..n {
        let cur = buf[idx];
        // KS loop: averaging low-pass + decay.
        buf[idx] = damp * 0.5 * (cur + prev);
        prev = cur;
        idx = (idx + 1) % period;
        if start + i < out.len() {
            // Short attack ramp avoids a click; body is the raw string.
            let env = (i as f32 / 48.0).min(1.0);
            out[start + i] += amp * env * cur;
        }
    }
}

fn normalize_peak(v: &mut [f32], target: f32) {
    let peak = v.iter().fold(0.0f32, |m, &x| m.max(x.abs())).max(1e-9);
    let gain = target / peak;
    for x in v {
        *x *= gain;
    }
}

/// Synthesize one phrase at `sr`, peak-normalized to [`HUMBUCKER_PEAK`].
pub fn phrase(kind: Phrase, sr: f32) -> Vec<f32> {
    let mut out = match kind {
        Phrase::Chugs => {
            let mut v = vec![0.0f32; (sr * 1.7) as usize];
            for (k, &t) in [0.0f32, 0.35, 0.7, 1.05].iter().enumerate() {
                pluck(&mut v, t, E2, 0.30, 0.9, true, 7 + k as u32, sr);
            }
            v
        }
        Phrase::PalmMutes => {
            let mut v = vec![0.0f32; (sr * 1.8) as usize];
            for k in 0..8 {
                let t = k as f32 * 0.2;
                let amp = if k % 2 == 0 { 0.95 } else { 0.6 };
                pluck(&mut v, t, E2, 0.18, amp, true, 200 + k, sr);
            }
            v
        }
        Phrase::Chords => {
            let mut v = vec![0.0f32; (sr * 2.6) as usize];
            for (k, &f) in E5_CHORD.iter().enumerate() {
                pluck(&mut v, 0.1, f, 2.4, 0.55, false, 40 + k as u32, sr);
            }
            v
        }
        Phrase::Lick => {
            let mut v = vec![0.0f32; (sr * 2.2) as usize];
            for (k, &(t, f)) in [
                (0.1f32, 164.81f32),
                (0.45, 196.0),
                (0.8, 220.0),
                (1.15, 261.63),
                (1.5, 196.0),
            ]
            .iter()
            .enumerate()
            {
                pluck(&mut v, t, f, 0.5, 0.7, false, 130 + k as u32, sr);
            }
            v
        }
        Phrase::CleanArpeggio => {
            let mut v = vec![0.0f32; (sr * 2.6) as usize];
            for (k, &f) in [E2, A2, D3, G3, D3, A2].iter().enumerate() {
                pluck(
                    &mut v,
                    0.1 + k as f32 * 0.28,
                    f,
                    1.6,
                    0.4,
                    false,
                    300 + k as u32,
                    sr,
                );
            }
            v
        }
        Phrase::SustainedLead => {
            let mut v = vec![0.0f32; (sr * 4.4) as usize];
            for (k, &(t, f)) in [(0.1f32, 220.0f32), (2.3, 196.0)].iter().enumerate() {
                pluck(&mut v, t, f, 2.0, 0.7, false, 400 + k as u32, sr);
            }
            v
        }
        Phrase::Dynamics => {
            let mut v = Vec::new();
            for (i, gain_db) in [-12.0f32, -6.0, 0.0].iter().enumerate() {
                let mut seg = vec![0.0f32; (sr * 1.0) as usize];
                pluck(&mut seg, 0.0, E2, 0.5, 0.9, true, 500 + i as u32, sr);
                pluck(&mut seg, 0.5, E2 * 1.5, 0.4, 0.7, false, 520 + i as u32, sr);
                let g = 10f32.powf(gain_db / 20.0);
                for x in &mut seg {
                    *x *= g;
                }
                v.extend(seg);
            }
            v
        }
    };
    normalize_peak(&mut out, HUMBUCKER_PEAK);
    out
}

/// Every phrase with its name, each peak-normalized to the humbucker reference.
pub fn corpus(sr: f32) -> Vec<(&'static str, Vec<f32>)> {
    Phrase::ALL
        .iter()
        .map(|&k| (k.name(), phrase(k, sr)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_phrase_is_deterministic_finite_and_calibrated() {
        for &kind in &Phrase::ALL {
            let a = phrase(kind, 48_000.0);
            let b = phrase(kind, 48_000.0);
            assert_eq!(a, b, "{} is not deterministic", kind.name());
            assert!(!a.is_empty(), "{} is empty", kind.name());
            assert!(
                a.iter().all(|x| x.is_finite()),
                "{} has non-finite samples",
                kind.name()
            );
            let peak = a.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
            assert!(
                (peak - HUMBUCKER_PEAK).abs() < 1e-4,
                "{} peak {peak} != humbucker reference",
                kind.name()
            );
        }
    }

    #[test]
    fn corpus_covers_every_phrase() {
        let c = corpus(48_000.0);
        assert_eq!(c.len(), Phrase::ALL.len());
        for (name, samples) in c {
            assert!(!name.is_empty() && !samples.is_empty());
        }
    }
}

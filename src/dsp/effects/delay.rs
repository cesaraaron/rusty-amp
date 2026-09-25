use std::f32::consts::TAU;

use crate::dsp::biquad::Biquad;

/// Stereo delay with two voicings selected by `kind`.
///
/// **Digital** (`kind` low) — tempo-free stereo ping-pong: feedback cross-feeds
/// L↔R so repeats bounce across the stereo field. TIME 0–1 maps to 0–500 ms,
/// FEEDBACK 0–1 maps to 0–85% to prevent runaway.
///
/// **Tape** (`kind` high) — an Echoplex EP-3 style echo: each repeat is
/// high-cut and softly saturated (the tape/head bump and compression), and the
/// transport runs slightly unsteady — a slow wow and a faster flutter modulate
/// the read position, so held notes drift the way tape does. Feedback returns to
/// the *same* channel (a mono tape deck, not a ping-pong), at a lower ceiling so
/// the darker repeats don't build up.
pub struct Delay {
    buf_l: Vec<f32>,
    buf_r: Vec<f32>,
    write: usize,
    sr: f32,
    // Tape transport: wow/flutter LFO phases and the per-channel feedback damping.
    wow_phase: f32,
    flutter_phase: f32,
    damp_l: Biquad,
    damp_r: Biquad,
}

/// Tape speed modulation: a slow wow plus a faster flutter, as fractions of the
/// delay time (±0.25% / ±0.12% — about ±4 cents combined, an audible but musical
/// drift).
const WOW_HZ: f32 = 0.7;
const WOW_DEPTH: f32 = 0.0025;
const FLUTTER_HZ: f32 = 6.3;
const FLUTTER_DEPTH: f32 = 0.0012;
/// Tape feedback repeats lose their top end (the record/play head gap loss).
const TAPE_DAMP_HZ: f32 = 3200.0;
/// Tape saturation: gentle, so repeated echoes thicken rather than distort.
const TAPE_SAT: f32 = 0.8;

impl Delay {
    pub fn new(sr: f32) -> Self {
        let max_samples = (sr * 0.5) as usize + 1; // 500 ms max
        Self {
            buf_l: vec![0.0; max_samples],
            buf_r: vec![0.0; max_samples],
            write: 0,
            sr,
            wow_phase: 0.0,
            flutter_phase: 0.0,
            damp_l: Biquad::lowpass(sr, TAPE_DAMP_HZ, 0.707),
            damp_r: Biquad::lowpass(sr, TAPE_DAMP_HZ, 0.707),
        }
    }

    /// `time`, `feedback`, `mix` 0–1; `kind` 0 = digital ping-pong, 1 = tape.
    #[inline]
    pub fn process(
        &mut self,
        l: f32,
        r: f32,
        time: f32,
        feedback: f32,
        mix: f32,
        kind: f32,
    ) -> (f32, f32) {
        let tape = kind >= 0.5;
        let len = self.buf_l.len();
        let base = time * self.sr * 0.5;

        // Tape wow + flutter modulate the read position; digital is rock steady.
        let delay = if tape {
            self.wow_phase = (self.wow_phase + WOW_HZ / self.sr).fract();
            self.flutter_phase = (self.flutter_phase + FLUTTER_HZ / self.sr).fract();
            let wow = (TAU * self.wow_phase).sin() * WOW_DEPTH;
            let flutter = (TAU * self.flutter_phase).sin() * FLUTTER_DEPTH;
            base * (1.0 + wow + flutter)
        } else {
            base
        };
        let d = delay.clamp(1.0, (len - 2) as f32);
        let i0 = d.floor() as usize;
        let frac = d - i0 as f32;
        let r0 = (self.write + len - i0) % len;
        let r1 = (self.write + len - i0 - 1) % len;
        let delayed_l = self.buf_l[r0] * (1.0 - frac) + self.buf_l[r1] * frac;
        let delayed_r = self.buf_r[r0] * (1.0 - frac) + self.buf_r[r1] * frac;

        if tape {
            let fb = feedback * 0.7;
            let dl = tape_sat(self.damp_l.process(delayed_l));
            let dr = tape_sat(self.damp_r.process(delayed_r));
            self.buf_l[self.write] = l + dl * fb;
            self.buf_r[self.write] = r + dr * fb;
        } else {
            let fb = feedback * 0.85;
            // Cross-fed feedback → ping-pong.
            self.buf_l[self.write] = l + delayed_r * fb;
            self.buf_r[self.write] = r + delayed_l * fb;
        }
        self.write = (self.write + 1) % len;

        let out_l = l * (1.0 - mix) + delayed_l * mix;
        let out_r = r * (1.0 - mix) + delayed_r * mix;
        (out_l, out_r)
    }
}

/// Soft tape saturation on the feedback path — thickens the repeats and tames
/// the runaway without an audible fuzz.
#[inline]
fn tape_sat(x: f32) -> f32 {
    (x * TAPE_SAT).tanh() / TAPE_SAT
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `mix = 0` must pass the dry signal through unchanged, in both voicings.
    #[test]
    fn fully_dry_is_passthrough() {
        for kind in [0.0f32, 1.0] {
            let mut d = Delay::new(48_000.0);
            for n in 0..1000 {
                let x = (n as f32 * 0.02).sin();
                let (l, r) = d.process(x, x, 0.3, 0.5, 0.0, kind);
                assert!((l - x).abs() < 1e-6 && (r - x).abs() < 1e-6);
            }
        }
    }

    /// A single impulse fed only to the left must re-emerge on the *left* one delay
    /// later (the direct tap), then bounce to the *right* after a second delay — the
    /// cross-fed feedback that makes the echoes ping-pong across the stereo field.
    #[test]
    fn impulse_pings_across_channels() {
        let sr = 48_000.0;
        let mut d = Delay::new(sr);
        let time = 0.2; // → 0.2 * sr * 0.5 = 4800 samples
        let delay_samples = (time * sr * 0.5) as usize;

        // Impulse on the left only, then silence; fully wet with feedback.
        d.process(1.0, 0.0, time, 0.6, 1.0, 0.0);
        // `peak` finds the loudest sample index in `[lo, hi)` for one channel.
        let mut left = (0.0f32, 0usize);
        let mut right = (0.0f32, 0usize);
        for n in 1..(delay_samples * 3) {
            let (l, r) = d.process(0.0, 0.0, time, 0.6, 1.0, 0.0);
            assert!(l.is_finite() && r.is_finite());
            if l.abs() > left.0 {
                left = (l.abs(), n);
            }
            if r.abs() > right.0 {
                right = (r.abs(), n);
            }
        }
        // First repeat lands on the left at ~one delay; the bounce lands on the
        // right at ~two delays.
        assert!(left.0 > 0.5, "left echo missing");
        assert!(right.0 > 0.1, "ping-pong bounce to right missing");
        assert!(
            (left.1 as i64 - delay_samples as i64).abs() <= 2,
            "left echo at {}, expected ~{delay_samples}",
            left.1
        );
        assert!(
            (right.1 as i64 - 2 * delay_samples as i64).abs() <= 2,
            "right bounce at {}, expected ~{}",
            right.1,
            2 * delay_samples
        );
    }

    /// Tape voicing does not ping-pong: an impulse on the left stays on the left,
    /// its repeats damped and saturated.
    #[test]
    fn tape_stays_on_the_same_channel() {
        let sr = 48_000.0;
        let mut d = Delay::new(sr);
        let time = 0.2;
        let delay_samples = (time * sr * 0.5) as usize;

        d.process(1.0, 0.0, time, 0.6, 1.0, 1.0);
        let mut left_energy = 0.0f32;
        let mut right_energy = 0.0f32;
        for n in 1..(delay_samples * 3) {
            let (l, r) = d.process(0.0, 0.0, time, 0.6, 1.0, 1.0);
            assert!(l.is_finite() && r.is_finite());
            // Ignore the first few samples where the dry-ish lead-in lives.
            if n > 8 {
                left_energy += l.abs();
                right_energy += r.abs();
            }
        }
        assert!(left_energy > 0.1, "tape echo missing on the left");
        assert!(
            right_energy < left_energy * 0.05,
            "tape should not ping-pong: right {right_energy:.4} vs left {left_energy:.4}"
        );
    }
}

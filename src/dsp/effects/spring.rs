//! Digital spring-tank reverb model (mono).
//!
//! A real spring reverb is not a diffuse room: a transient travels down the
//! spring as a **dispersive** wave, so a click comes back as a downward "boing"
//! chirp (high frequencies propagate faster than low ones) riding a fluttery,
//! band-limited tail. We approximate that with two stages:
//!
//! 1. A cascade of first-order allpass filters with a short delay stands in for
//!    the spring's frequency-dependent propagation delay. Each stage delays low
//!    frequencies more than high ones, so an impulse is smeared into a chirp.
//! 2. Two recirculating delay lines of unequal length (the tank's two springs)
//!    with a one-pole low-pass in the loop give the dark, decaying flutter.
//!
//! The tank passes little low end, so the input and wet output are high-passed;
//! the wet output is also low-passed to the spring's high-frequency limit. The
//! effect is mono in / mono out — the caller mixes dry and wet (see the Fender
//! amp, which places the onboard tank before its bias tremolo and power amp).
use super::param_changed;
use crate::dsp::biquad::Biquad;

/// Allpass stages in the dispersion chain.
const NUM_AP: usize = 20;
/// Per-stage delay (samples at the 44.1 kHz reference rate).
const AP_DELAY: usize = 4;
/// Allpass coefficient; `(1+g)/(1-g)` sets the low/high group-delay spread.
const AP_G: f32 = 0.62;

/// The tank's spring loop lengths (ms) — a long and a short spring, so the two
/// tails decorrelate into flutter rather than a single clean echo.
const SPRING_MS: [f32; 2] = [27.0, 41.0];

const BASE_SR: f32 = 44_100.0;

/// First-order allpass with an integer delay; the dispersive building block.
struct Allpass {
    buf: Vec<f32>,
    pos: usize,
    g: f32,
}

impl Allpass {
    fn new(delay: usize, g: f32) -> Self {
        Self {
            buf: vec![0.0; delay.max(1)],
            pos: 0,
            g,
        }
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let bufout = self.buf[self.pos];
        let y = -self.g * x + bufout;
        self.buf[self.pos] = x + self.g * y;
        self.pos += 1;
        if self.pos == self.buf.len() {
            self.pos = 0;
        }
        y
    }
}

/// One damped recirculating spring.
struct Spring {
    buf: Vec<f32>,
    pos: usize,
    lp: f32,
    fb: f32,
    damp: f32,
}

impl Spring {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            pos: 0,
            lp: 0.0,
            fb: 0.7,
            damp: 0.3,
        }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let y = self.buf[self.pos];
        // One-pole low-pass in the feedback path (spring losses darken each pass).
        self.lp = y * (1.0 - self.damp) + self.lp * self.damp;
        self.buf[self.pos] = input + self.lp * self.fb;
        self.pos += 1;
        if self.pos == self.buf.len() {
            self.pos = 0;
        }
        y
    }

    fn set_params(&mut self, fb: f32, damp: f32) {
        self.fb = fb;
        self.damp = damp;
    }
}

/// Mono spring-tank reverb. `decay` 0–1 sets the tail length; `damp` 0–1 makes
/// the tail darker. Returns the wet signal only.
pub struct SpringReverb {
    ap: Vec<Allpass>,
    springs: Vec<Spring>,
    in_hp: Biquad,
    out_hp: Biquad,
    out_lp: Biquad,
    /// Wet trim that normalizes the loops' `1/(1 - fb)` steady-state gain, so a
    /// long decay does not also mean a loud reverb.
    wet_norm: f32,
    last_decay: f32,
    last_damp: f32,
}

impl SpringReverb {
    pub fn new(sr: f32) -> Self {
        let scale = sr / BASE_SR;
        let ap_delay = ((AP_DELAY as f32 * scale).round() as usize).max(1);
        let ap = (0..NUM_AP).map(|_| Allpass::new(ap_delay, AP_G)).collect();
        let springs = SPRING_MS
            .iter()
            .map(|ms| Spring::new((ms * 0.001 * sr).round() as usize))
            .collect();
        let mut r = Self {
            ap,
            springs,
            // The tank passes little low end (springs are high-pass by nature).
            in_hp: Biquad::highpass(sr, 180.0, 0.707),
            out_hp: Biquad::highpass(sr, 180.0, 0.707),
            // Springs roll off the top end rather than ringing bright.
            out_lp: Biquad::lowpass(sr, 5_000.0, 0.707),
            wet_norm: 1.0,
            last_decay: -1.0,
            last_damp: -1.0,
        };
        r.update_params(0.5, 0.4);
        r
    }

    fn update_params(&mut self, decay: f32, damp: f32) {
        let decay = decay.clamp(0.0, 1.0);
        let damp = damp.clamp(0.0, 1.0);
        let fb = 0.45 + decay * 0.45;
        let d = 0.12 + damp * 0.5;
        for s in &mut self.springs {
            s.set_params(fb, d);
        }
        self.wet_norm = 1.0 - fb;
        self.last_decay = decay;
        self.last_damp = damp;
    }

    /// `decay` 0–1, `damp` 0–1. Mono in, mono wet out; the caller sets dry/wet.
    #[inline]
    pub fn process(&mut self, input: f32, decay: f32, damp: f32) -> f32 {
        if param_changed(decay, self.last_decay) || param_changed(damp, self.last_damp) {
            self.update_params(decay, damp);
        }

        let x = self.in_hp.process(input);
        let mut dispersed = x;
        for ap in &mut self.ap {
            dispersed = ap.process(dispersed);
        }

        let mut wet = 0.0;
        for s in &mut self.springs {
            wet += s.process(dispersed);
        }
        wet /= self.springs.len() as f32;
        wet *= self.wet_norm;

        self.out_lp.process(self.out_hp.process(wet))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tail_energy(rv: &mut SpringReverb, decay: f32, damp: f32, n: usize) -> f64 {
        let mut e = 0.0;
        for _ in 0..n {
            let y = rv.process(0.0, decay, damp);
            assert!(y.is_finite(), "spring reverb produced a non-finite sample");
            e += (y * y) as f64;
        }
        e
    }

    /// Silence in, silence out — and no denormal/NaN surprises.
    #[test]
    fn silence_stays_silent() {
        let mut rv = SpringReverb::new(48_000.0);
        for _ in 0..2_000 {
            assert_eq!(rv.process(0.0, 0.6, 0.4), 0.0);
        }
    }

    /// An impulse must ring out after the input stops, and the tail must decay.
    #[test]
    fn impulse_produces_decaying_tail() {
        let mut rv = SpringReverb::new(48_000.0);
        rv.process(1.0, 0.8, 0.35); // impulse
        let early = tail_energy(&mut rv, 0.8, 0.35, 2_400); // first 50 ms
        let late = tail_energy(&mut rv, 0.8, 0.35, 2_400); // next 50 ms
        assert!(early > 1e-6, "spring produced no tail");
        assert!(late < early, "spring tail did not decay");
    }

    /// The dispersion chain must delay low frequencies more than high ones, so a
    /// transient emerges as a downward chirp. Measure the peak arrival time of a
    /// windowed tone burst: the higher tone must arrive first.
    #[test]
    fn dispersion_delays_lows_more_than_highs() {
        let sr = 48_000.0;
        let peak_delay = |freq: f32| {
            let scale = sr / BASE_SR;
            let d = ((AP_DELAY as f32 * scale).round() as usize).max(1);
            let mut ap: Vec<Allpass> = (0..NUM_AP).map(|_| Allpass::new(d, AP_G)).collect();
            let n = 200usize;
            let (mut peak, mut peak_v) = (0usize, 0.0f32);
            for i in 0..1_500 {
                let x = if i < n {
                    let w = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n as f32).cos();
                    w * (2.0 * std::f32::consts::PI * freq * i as f32 / sr).sin()
                } else {
                    0.0
                };
                let mut v = x;
                for a in &mut ap {
                    v = a.process(v);
                }
                if v.abs() > peak_v {
                    peak_v = v.abs();
                    peak = i;
                }
            }
            peak
        };
        let high = peak_delay(4_000.0);
        let low = peak_delay(400.0);
        assert!(
            high < low,
            "expected highs to arrive first (high {high} vs low {low})"
        );
    }

    /// Full decay must stay bounded — the recirculating loops must not run away.
    #[test]
    fn full_decay_is_stable() {
        let mut rv = SpringReverb::new(48_000.0);
        for n in 0..48_000 {
            let x = if n % 97 == 0 { 0.8 } else { 0.0 };
            let y = rv.process(x, 1.0, 0.0);
            assert!(y.is_finite() && y.abs() < 30.0, "spring unstable: {y}");
        }
    }
}

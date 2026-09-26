//! Input-level calibration: shared lock-free state plus the pure per-sample
//! trim/window loop.
//!
//! The engine reference level fixes what a defined performance (hard open-E
//! strums, volume on 10) should read in the engine, so amp breakup does not
//! depend on the user's interface gain. A trim of 0 dB is the uncalibrated
//! default and is bit-identical to no trim at all.

use atomic_float::AtomicF32;
use std::sync::atomic::AtomicBool;

use crate::dsp::effects::db_to_lin;

/// Length of one measurement window.
pub const CAL_WINDOW_MS: f32 = 10.0;
/// Capacity of the audio → UI window-stats ring.
pub const CAL_RING_CAPACITY: usize = 4096;
/// |x| at or above this counts as raw clipping.
pub const CLIP_THRESHOLD: f32 = 0.999;
/// Time constant of the trim-gain smoother.
pub const TRIM_SMOOTH_MS: f32 = 20.0;

/// Shared, lock-free input-calibration state (control thread ⇄ audio thread).
#[derive(Default)]
pub struct InputCalibration {
    /// Trim applied to the guitar input, in dB. `0.0` = uncalibrated.
    pub trim_db: AtomicF32,
    /// UI sets while the wizard is capturing; the audio thread then pushes stats.
    pub measuring: AtomicBool,
    /// Sticky: a raw (pre-trim) sample reached [`CLIP_THRESHOLD`]. UI clears it.
    pub raw_clip: AtomicBool,
    /// Sticky: the stats ring was full and a window was dropped. UI clears it.
    pub stats_overflow: AtomicBool,
}

impl InputCalibration {
    pub fn new() -> Self {
        Self::default()
    }
}

/// One accumulated measurement window on the **raw** (pre-trim) signal.
#[derive(Clone, Copy, Debug, Default)]
pub struct WindowStat {
    pub peak_raw: f32,
    pub sum_sq_raw: f32,
    pub frames: u32,
}

/// The audio thread's trim smoother state.
#[derive(Clone, Copy, Debug)]
pub struct TrimState {
    /// Current (smoothed) linear gain.
    pub gain: f32,
    /// One-pole coefficient for [`TRIM_SMOOTH_MS`] at the engine rate.
    pub coeff: f32,
}

impl TrimState {
    /// Build from the loaded trim so there is no startup ramp.
    pub fn new(sr: f32, trim_db: f32) -> Self {
        Self {
            gain: db_to_lin(trim_db),
            coeff: 1.0 - (-1.0 / (sr * TRIM_SMOOTH_MS / 1000.0)).exp(),
        }
    }
}

/// Sticky flags raised by one processed block.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Flags {
    pub clipped: bool,
    pub overflow: bool,
}

/// Apply the smoothed trim to `buf` in place, reporting raw clipping and, while
/// `measuring`, accumulating windows of `win_len` frames into `sink`.
///
/// `sink` returns `true` when the window was accepted, `false` when the ring was
/// full (the window is still reset). This is the whole input-conditioning hot
/// path; it never allocates. When not measuring, `win` is kept clear.
pub fn condition_block(
    buf: &mut [f32],
    state: &mut TrimState,
    win: &mut WindowStat,
    win_len: u32,
    target: f32,
    measuring: bool,
    mut sink: impl FnMut(WindowStat) -> bool,
) -> Flags {
    let mut flags = Flags::default();
    if !measuring {
        *win = WindowStat::default();
    }
    for x in buf.iter_mut() {
        let raw = *x;
        if raw.abs() >= CLIP_THRESHOLD {
            flags.clipped = true;
        }
        if measuring {
            win.peak_raw = win.peak_raw.max(raw.abs());
            win.sum_sq_raw += raw * raw;
            win.frames += 1;
            if win.frames >= win_len {
                if !sink(*win) {
                    flags.overflow = true;
                }
                *win = WindowStat::default();
            }
        }
        // One-pole toward target; exact 1.0 when target and gain are both 1.0.
        state.gain += state.coeff * (target - state.gain);
        *x = raw * state.gain;
    }
    flags
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn state(sr: f32, db: f32) -> TrimState {
        TrimState::new(sr, db)
    }

    /// Uncalibrated (0 dB) is bit-identical: gain settles at exactly 1.0 and
    /// `x * 1.0 == x`.
    #[test]
    fn zero_db_is_bit_identical() {
        let sr = 48_000.0;
        let mut st = state(sr, 0.0);
        let mut win = WindowStat::default();
        let original: Vec<f32> = (0..1000)
            .map(|i| (2.0 * PI * 120.0 * i as f32 / sr).sin() * 0.5)
            .collect();
        let mut buf = original.clone();
        let flags = condition_block(&mut buf, &mut st, &mut win, 480, 1.0, false, |_| true);
        assert_eq!(buf, original, "0 dB trim altered the signal");
        assert!(!flags.clipped && !flags.overflow);
        assert_eq!(st.gain, 1.0);
    }

    /// A step to +6 dB converges to 2.0× without overshoot.
    #[test]
    fn plus_six_db_converges_without_overshoot() {
        let sr = 48_000.0;
        let target = db_to_lin(6.0);
        let mut st = state(sr, 0.0);
        let mut win = WindowStat::default();
        let n = (sr * (5.0 * TRIM_SMOOTH_MS / 1000.0)) as usize;
        let mut buf = vec![1.0f32; n];
        let mut over = 0.0f32;
        // Process in small blocks, watching the gain.
        for chunk in buf.chunks_mut(64) {
            condition_block(chunk, &mut st, &mut win, 480, target, false, |_| true);
            over = over.max(st.gain);
        }
        assert!(
            (st.gain - target).abs() / target < 0.01,
            "gain {} did not converge to {target}",
            st.gain
        );
        assert!(over <= target * 1.001, "overshoot to {over}");
    }

    /// Window stats sum to the expected energy of a known sine.
    #[test]
    fn window_stats_match_a_known_sine() {
        let sr = 48_000.0;
        let amp = 0.25f32;
        let freq = 1000.0;
        let n = 4800usize; // 100 ms
        let mut st = state(sr, 0.0);
        let mut win = WindowStat::default();
        let mut buf: Vec<f32> = (0..n)
            .map(|i| amp * (2.0 * PI * freq * i as f32 / sr).sin())
            .collect();
        let mut windows = Vec::new();
        condition_block(&mut buf, &mut st, &mut win, 480, 1.0, true, |w| {
            windows.push(w);
            true
        });
        assert_eq!(windows.len(), 10, "expected ten 10 ms windows");
        for w in windows {
            assert_eq!(w.frames, 480);
            assert!((w.peak_raw - amp).abs() < 1e-3, "peak {}", w.peak_raw);
            let expected = w.sum_sq_raw;
            let ideal = 0.5 * amp * amp * 480.0;
            assert!(
                (expected - ideal).abs() / ideal < 0.02,
                "energy {expected} vs {ideal}"
            );
        }
    }

    /// Raw clipping is flagged even when the trim is negative.
    #[test]
    fn clipping_flagged_from_raw_with_negative_trim() {
        let mut st = state(48_000.0, -12.0);
        let mut win = WindowStat::default();
        let mut buf = vec![0.0f32, 1.0, 0.0];
        let flags = condition_block(
            &mut buf,
            &mut st,
            &mut win,
            480,
            db_to_lin(-12.0),
            false,
            |_| true,
        );
        assert!(flags.clipped, "a raw full-scale sample must flag clipping");
        // The trimmed output is below full scale.
        assert!(buf[1].abs() < 0.5);
    }

    /// A full ring raises the overflow flag rather than blocking.
    #[test]
    fn full_ring_reports_overflow() {
        let mut st = state(48_000.0, 0.0);
        let mut win = WindowStat::default();
        let mut buf = vec![0.1f32; 960];
        let flags = condition_block(&mut buf, &mut st, &mut win, 480, 1.0, true, |_| false);
        assert!(flags.overflow && !flags.clipped);
    }
}

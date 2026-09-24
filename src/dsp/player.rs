//! Practice / jam-along playback voice: mixes one or two decoded stereo tracks
//! (a backing track and your own recorded take) into the monitor output along a
//! shared timeline cursor, with a loop region.
//!
//! The tracks are decoded, resampled and placed on the timeline off the audio
//! thread; the finished [`PlayerTrack`]s are handed to the realtime callback as
//! boxed values and installed lock-free (see [`crate::audio`]). Everything on the
//! audio side is O(1) and allocation-free; the voice never frees a track itself —
//! displaced tracks are shipped back to the control thread for disposal.

/// A decoded, rate-matched stereo track placed on the practice timeline. Owned by
/// the audio thread once installed.
#[derive(Debug)]
pub struct PlayerTrack {
    pub l: Vec<f32>,
    pub r: Vec<f32>,
    /// Timeline frame where this track begins. `0` for a backing track; for a take
    /// it is the captured backing playhead, so it lines up with what was playing.
    pub start: usize,
}

impl PlayerTrack {
    /// Number of frames the track actually carries (channels may differ by one in
    /// pathological files, so take the shorter).
    #[inline]
    pub fn frames(&self) -> usize {
        self.l.len().min(self.r.len())
    }
}

/// A snapshot of the shared transport, read from atomics once per audio callback
/// so the per-sample loop only touches plain fields.
#[derive(Clone, Copy)]
pub struct Transport {
    pub playing: bool,
    /// A pending seek (frames), consumed by [`PlayerVoice::begin`].
    pub seek: Option<usize>,
    pub loop_enabled: bool,
    pub loop_start: usize,
    pub loop_end: usize,
    pub backing_muted: bool,
    pub backing_gain: f32,
    pub record_muted: bool,
    pub record_gain: f32,
}

impl Default for Transport {
    fn default() -> Self {
        Self {
            playing: false,
            seek: None,
            loop_enabled: false,
            loop_start: 0,
            loop_end: 0,
            backing_muted: false,
            backing_gain: 1.0,
            record_muted: false,
            record_gain: 1.0,
        }
    }
}

/// The audio-thread playback mixer. Holds the two tracks and one shared timeline
/// cursor; both tracks are indexed against the same cursor, so they stay locked.
pub struct PlayerVoice {
    backing: Option<PlayerTrack>,
    record: Option<PlayerTrack>,
    cursor: usize,
}

impl Default for PlayerVoice {
    fn default() -> Self {
        Self::new()
    }
}

impl PlayerVoice {
    pub fn new() -> Self {
        Self {
            backing: None,
            record: None,
            cursor: 0,
        }
    }

    /// Install (`Some`) or clear (`None`) the backing track, returning the track it
    /// displaced (if any) so the caller can drop it off the audio thread.
    pub fn set_backing(&mut self, track: Option<PlayerTrack>) -> Option<PlayerTrack> {
        std::mem::replace(&mut self.backing, track)
    }

    /// Install (`Some`) or clear (`None`) the take track, returning the displaced one.
    pub fn set_record(&mut self, track: Option<PlayerTrack>) -> Option<PlayerTrack> {
        std::mem::replace(&mut self.record, track)
    }

    pub fn has_backing(&self) -> bool {
        self.backing.is_some()
    }

    pub fn has_record(&self) -> bool {
        self.record.is_some()
    }

    /// The shared timeline cursor, in frames. The caller stores this for the UI.
    #[inline]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Apply a block-level seek. Call once per callback before the sample loop.
    #[inline]
    pub fn begin(&mut self, t: &Transport) {
        if let Some(pos) = t.seek {
            self.cursor = pos;
        }
    }

    /// Produce the next stereo frame and advance the cursor by one. Returns silence
    /// while stopped, so the rig can be played over a paused track.
    #[inline]
    pub fn next_frame(&mut self, t: &Transport) -> (f32, f32) {
        if !t.playing {
            return (0.0, 0.0);
        }
        // Loop wrap: only meaningful for a well-formed region.
        if t.loop_enabled && t.loop_end > t.loop_start && self.cursor >= t.loop_end {
            self.cursor = t.loop_start;
        }
        let (bl, br) = track_frame(
            self.backing.as_ref(),
            self.cursor,
            t.backing_muted,
            t.backing_gain,
        );
        let (rl, rr) = track_frame(
            self.record.as_ref(),
            self.cursor,
            t.record_muted,
            t.record_gain,
        );
        self.cursor = self.cursor.saturating_add(1);
        (bl + rl, br + rr)
    }
}

/// Read one timeline frame from `track` (accounting for its start offset), or
/// silence when muted / out of range / absent.
#[inline]
fn track_frame(track: Option<&PlayerTrack>, cursor: usize, muted: bool, gain: f32) -> (f32, f32) {
    if muted {
        return (0.0, 0.0);
    }
    let Some(t) = track else {
        return (0.0, 0.0);
    };
    let Some(idx) = cursor.checked_sub(t.start) else {
        return (0.0, 0.0);
    };
    if idx >= t.frames() {
        return (0.0, 0.0);
    }
    (t.l[idx] * gain, t.r[idx] * gain)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(n: usize, start: usize) -> PlayerTrack {
        PlayerTrack {
            l: (0..n).map(|i| i as f32).collect(),
            r: (0..n).map(|i| -(i as f32)).collect(),
            start,
        }
    }

    #[test]
    fn stopped_is_silent_and_does_not_advance() {
        let mut v = PlayerVoice::new();
        v.set_backing(Some(track(100, 0)));
        let t = Transport::default(); // playing = false
        for _ in 0..10 {
            assert_eq!(v.next_frame(&t), (0.0, 0.0));
        }
        assert_eq!(v.cursor(), 0);
    }

    #[test]
    fn plays_and_advances_in_order() {
        let mut v = PlayerVoice::new();
        v.set_backing(Some(track(100, 0)));
        let t = Transport {
            playing: true,
            ..Transport::default()
        };
        assert_eq!(v.next_frame(&t), (0.0, -0.0));
        assert_eq!(v.next_frame(&t), (1.0, -1.0));
        assert_eq!(v.next_frame(&t), (2.0, -2.0));
        assert_eq!(v.cursor(), 3);
    }

    #[test]
    fn seek_jumps_the_cursor() {
        let mut v = PlayerVoice::new();
        v.set_backing(Some(track(100, 0)));
        let t = Transport {
            playing: true,
            seek: Some(10),
            ..Transport::default()
        };
        v.begin(&t);
        assert_eq!(v.next_frame(&t), (10.0, -10.0));
        assert_eq!(v.cursor(), 11);
    }

    #[test]
    fn loop_region_wraps_back() {
        let mut v = PlayerVoice::new();
        v.set_backing(Some(track(100, 0)));
        let t = Transport {
            playing: true,
            loop_enabled: true,
            loop_start: 4,
            loop_end: 6,
            ..Transport::default()
        };
        let got: Vec<f32> = (0..6).map(|_| v.next_frame(&t).0).collect();
        // Starts at 0, then wraps 4→5→4→5 after reaching the end.
        assert_eq!(got, vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0]);
        assert_eq!(v.next_frame(&t).0, 4.0, "cursor must wrap to loop_start");
        assert_eq!(v.next_frame(&t).0, 5.0);
    }

    #[test]
    fn start_offset_aligns_a_take() {
        let mut v = PlayerVoice::new();
        // A take that begins on the timeline at frame 5.
        v.set_record(Some(track(3, 5)));
        let t = Transport {
            playing: true,
            seek: Some(5),
            ..Transport::default()
        };
        v.begin(&t);
        assert_eq!(v.next_frame(&t), (0.0, -0.0)); // record[0]
        assert_eq!(v.next_frame(&t), (1.0, -1.0)); // record[1]
        assert_eq!(v.next_frame(&t), (2.0, -2.0)); // record[2]
        assert_eq!(v.next_frame(&t), (0.0, 0.0)); // past the take's end
    }

    #[test]
    fn mute_silences_but_still_advances() {
        let mut v = PlayerVoice::new();
        v.set_backing(Some(track(100, 0)));
        let t = Transport {
            playing: true,
            backing_muted: true,
            ..Transport::default()
        };
        assert_eq!(v.next_frame(&t), (0.0, 0.0));
        assert_eq!(v.cursor(), 1, "a muted track still advances the timeline");
    }

    #[test]
    fn gain_scales_the_output() {
        let mut v = PlayerVoice::new();
        v.set_backing(Some(track(100, 0)));
        let t = Transport {
            playing: true,
            seek: Some(3),
            backing_gain: 0.5,
            ..Transport::default()
        };
        v.begin(&t);
        assert_eq!(v.next_frame(&t), (1.5, -1.5));
    }

    #[test]
    fn displaced_track_is_returned_for_off_thread_drop() {
        let mut v = PlayerVoice::new();
        assert!(v.set_backing(Some(track(1, 0))).is_none());
        let old = v.set_backing(None);
        assert!(old.is_some(), "the displaced track must be handed back");
    }
}

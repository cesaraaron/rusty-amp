//! Practice / jam-along: shared transport state and offline track decoding.
//!
//! [`Practice`] is the lock-free handshake between the UI thread (which writes the
//! transport and hands over decoded tracks) and the audio thread (which reads a
//! [`Transport`] snapshot per callback and mixes the tracks into the monitor).
//! Decoding a backing file or a recorded take happens here, entirely off the
//! audio thread; the finished [`PlayerTrack`] is installed into the engine
//! lock-free (see [`crate::audio`]).

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering::Relaxed};

use anyhow::{Context, Result, anyhow};
use atomic_float::AtomicF32;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::dsp::player::{PlayerTrack, Transport};
use crate::dsp::resample::resample;

/// Shared practice transport. The UI writes every field except `position`; the
/// audio thread reads them and writes `position`.
pub struct Practice {
    /// UI → audio: whether the timeline is playing.
    pub playing: AtomicBool,
    /// Audio → UI: current timeline frame.
    pub position: AtomicU64,
    /// UI → audio: a pending seek in frames; `-1` means none. Consumed on read.
    pub seek: AtomicI64,
    pub loop_enabled: AtomicBool,
    pub loop_start: AtomicU64,
    pub loop_end: AtomicU64,
    pub backing_muted: AtomicBool,
    pub backing_gain: AtomicF32,
    pub record_muted: AtomicBool,
    pub record_gain: AtomicF32,
    /// UI-side knowledge of what is installed (set when an install succeeds).
    pub backing_loaded: AtomicBool,
    pub record_loaded: AtomicBool,
    /// Track lengths in frames (set when installed), for the UI timeline.
    pub backing_len: AtomicU64,
    pub record_len: AtomicU64,
}

impl Default for Practice {
    fn default() -> Self {
        Self::new()
    }
}

impl Practice {
    pub fn new() -> Self {
        Self {
            playing: AtomicBool::new(false),
            position: AtomicU64::new(0),
            seek: AtomicI64::new(-1),
            loop_enabled: AtomicBool::new(false),
            loop_start: AtomicU64::new(0),
            loop_end: AtomicU64::new(0),
            backing_muted: AtomicBool::new(false),
            backing_gain: AtomicF32::new(1.0),
            record_muted: AtomicBool::new(false),
            record_gain: AtomicF32::new(1.0),
            backing_loaded: AtomicBool::new(false),
            record_loaded: AtomicBool::new(false),
            backing_len: AtomicU64::new(0),
            record_len: AtomicU64::new(0),
        }
    }

    /// Read a consistent-enough transport snapshot for one audio callback, consuming
    /// any pending seek. All loads are relaxed: the worst case is one buffer of
    /// staleness on a knob move, which is inaudible.
    pub fn snapshot(&self) -> Transport {
        let seek = self.seek.swap(-1, Relaxed);
        Transport {
            playing: self.playing.load(Relaxed),
            seek: if seek >= 0 { Some(seek as usize) } else { None },
            loop_enabled: self.loop_enabled.load(Relaxed),
            loop_start: self.loop_start.load(Relaxed) as usize,
            loop_end: self.loop_end.load(Relaxed) as usize,
            backing_muted: self.backing_muted.load(Relaxed),
            backing_gain: self.backing_gain.load(Relaxed),
            record_muted: self.record_muted.load(Relaxed),
            record_gain: self.record_gain.load(Relaxed),
        }
    }

    /// Clear the transport back to its startup state. Called when a fresh engine
    /// starts (including a device change), since the decoded tracks live with the
    /// old engine and are gone.
    pub fn reset(&self) {
        self.playing.store(false, Relaxed);
        self.position.store(0, Relaxed);
        self.seek.store(-1, Relaxed);
        self.loop_enabled.store(false, Relaxed);
        self.loop_start.store(0, Relaxed);
        self.loop_end.store(0, Relaxed);
        self.backing_loaded.store(false, Relaxed);
        self.record_loaded.store(false, Relaxed);
        self.backing_len.store(0, Relaxed);
        self.record_len.store(0, Relaxed);
    }

    pub fn store_position(&self, frame: usize) {
        self.position.store(frame as u64, Relaxed);
    }

    pub fn position(&self) -> usize {
        self.position.load(Relaxed) as usize
    }

    pub fn request_seek(&self, frame: usize) {
        self.seek.store(frame as i64, Relaxed);
    }

    /// Longest timeline extent: the backing length, or the end of the take
    /// (its start offset included), whichever is longer.
    pub fn timeline_len(&self) -> usize {
        let backing = self.backing_len.load(Relaxed) as usize;
        let record = self.record_len.load(Relaxed) as usize;
        backing.max(record)
    }
}

// ── Decoding ─────────────────────────────────────────────────────────────────

/// Decode an audio file (MP3 / WAV / FLAC) to a stereo [`PlayerTrack`] at the
/// engine rate, starting at timeline frame 0. Runs off the audio thread — file
/// IO, decode and an offline resample.
pub fn decode_track(path: impl AsRef<Path>, target_sr: f32) -> Result<PlayerTrack> {
    let path = path.as_ref();
    let file = std::fs::File::open(path)
        .with_context(|| format!("opening audio file {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .with_context(|| format!("unsupported or corrupt audio file {}", path.display()))?;
    let mut format = probed.format;

    let track = format
        .default_track()
        .ok_or_else(|| anyhow!("audio file has no playable track"))?;
    let track_id = track.id;
    let src_sr = track.codec_params.sample_rate.unwrap_or(44_100) as f32;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .with_context(|| format!("no decoder for audio file {}", path.display()))?;

    let mut sample_buf: Option<SampleBuffer<f32>> = None;
    let mut l: Vec<f32> = Vec::new();
    let mut r: Vec<f32> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            // Normal end of stream.
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(e) => return Err(e).context("reading audio packet"),
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                let capacity = decoded.capacity();
                // (Re)allocate the scratch buffer only when the codec asks for more.
                if sample_buf.as_ref().is_none_or(|b| b.capacity() < capacity) {
                    sample_buf = Some(SampleBuffer::<f32>::new(capacity as u64, spec));
                }
                let buf = sample_buf
                    .as_mut()
                    .ok_or_else(|| anyhow!("sample buffer missing"))?;
                buf.copy_interleaved_ref(decoded);

                let channels = spec.channels.count().max(1);
                for frame in buf.samples().chunks(channels) {
                    let c0 = frame.first().copied().unwrap_or(0.0);
                    let c1 = frame.get(1).copied().unwrap_or(c0);
                    l.push(c0);
                    r.push(c1);
                }
            }
            // A corrupt packet is skipped rather than aborting the whole load.
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(e) => return Err(e).context("decoding audio packet"),
        }
    }

    if l.is_empty() {
        return Err(anyhow!("audio file decoded to no samples"));
    }

    if (src_sr - target_sr).abs() > 0.5 {
        let ratio = target_sr / src_sr;
        l = resample(&l, ratio);
        r = resample(&r, ratio);
    }

    Ok(PlayerTrack { l, r, start: 0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seek_is_consumed_once() {
        let p = Practice::new();
        assert_eq!(p.snapshot().seek, None);
        p.request_seek(1234);
        assert_eq!(p.snapshot().seek, Some(1234));
        assert_eq!(p.snapshot().seek, None, "a seek must not fire twice");
    }

    #[test]
    fn timeline_len_tracks_the_longest_installed_source() {
        let p = Practice::new();
        assert_eq!(p.timeline_len(), 0);
        p.backing_len.store(1000, Relaxed);
        assert_eq!(p.timeline_len(), 1000);
        p.record_len.store(2500, Relaxed);
        assert_eq!(p.timeline_len(), 2500);
    }

    #[test]
    fn decode_rejects_a_missing_file() {
        let err = decode_track("/nonexistent/rusty-amp-nope.mp3", 48_000.0).unwrap_err();
        assert!(
            err.to_string().contains("opening"),
            "unexpected error: {err}"
        );
    }
}

//! Offline guitar-take export.
//!
//! Renders the timeline's **unmuted raw takes** — and only those — through a
//! fresh, deterministic instance of the rig to a stereo 32-bit float WAV at the
//! project sample rate. Imports, the metronome, live input and the click are
//! excluded by construction.
//!
//! The render runs on a worker thread with its own [`DspChain`], built from a
//! **snapshot** of the rig settings (`Preset`), so knob moves during a render do
//! not affect it. External third-party processors cannot be cloned faithfully yet
//! (no state capture), so the UI refuses to export while an AU amp or CLAP insert
//! is loaded; the external **IR** is supported and re-loaded at the export rate.
//!
//! See [`timeline-sessions-plan.md`](../../timeline-sessions-plan.md) §6.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use std::sync::mpsc::{self, Receiver};

use anyhow::{Context, Result, bail};

use crate::dsp::cab::{ExternalIrCab, MAX_IR_LEN, load_ir};
use crate::dsp::{DspChain, Params, StereoInsert};
use crate::practice::decode_track;
use crate::preset::Preset;

/// Frames per render block. Must stay ≤ [`crate::audio::MAX_BLOCK`].
const BLOCK: usize = 512;
/// Maximum effect tail rendered after the last take, in seconds.
const TAIL_CAP_SECS: f32 = 12.0;
/// A tail block is considered silent below this peak.
const TAIL_THRESHOLD: f32 = 1.0e-4;
/// How long the tail must stay silent before the render stops, in seconds.
const TAIL_HOLD_SECS: f32 = 0.25;

/// One raw take to place on the export timeline. `path` is decoded/resampled at
/// the export rate off-thread.
#[derive(Clone)]
pub struct ExportClip {
    pub path: PathBuf,
    pub start_ticks: u64,
    pub gain: f32,
}

/// Where an external processor belongs in the export chain.
pub enum ExternalPlacement {
    /// Post-rack stereo insert.
    Insert,
    /// Amp-position override. `amp_only` keeps the built-in cab/IR in the path.
    Amp {
        amp_only: bool,
        latency_frames: usize,
    },
}

/// A freshly built external processor plus a token that must stay alive for the
/// whole render (a CLAP `LoadedPlugin` unloads the bundle when dropped).
pub struct ExternalInstance {
    pub insert: Box<dyn StereoInsert>,
    pub placement: ExternalPlacement,
    pub keepalive: Box<dyn std::any::Any>,
}

/// Builder run **on the export worker** so a plugin instance is created and owned
/// on that thread. The closure captures only `Send` identity/state data.
pub type BuildExternal = Box<dyn FnOnce() -> anyhow::Result<ExternalInstance> + Send>;

/// A frozen export request.
pub struct ExportJob {
    pub dest: PathBuf,
    pub sample_rate: u32,
    pub project_sample_rate: u32,
    pub clips: Vec<ExportClip>,
    /// Rig snapshot applied to a private [`Params`] instance.
    pub rig: Preset,
    /// External IR source, re-loaded at the export rate when set.
    pub ir_path: Option<PathBuf>,
    pub ir_active: bool,
    /// CLAP insert, re-instantiated with captured state on the worker.
    pub insert: Option<BuildExternal>,
    /// AU amp override, re-instantiated with captured state on the worker.
    pub amp: Option<BuildExternal>,
}

/// Handle to a running export on a worker thread.
pub struct ExportHandle {
    pub rx: Receiver<Result<PathBuf>>,
    pub cancel: Arc<AtomicBool>,
    /// Progress in per-mille (0..=1000).
    pub progress: Arc<AtomicU32>,
}

impl ExportHandle {
    /// Current progress as a percentage.
    pub fn percent(&self) -> u32 {
        self.progress.load(Relaxed) / 10
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Relaxed);
    }
}

/// Spawn the render worker and return a handle to poll.
pub fn spawn(job: ExportJob) -> ExportHandle {
    let cancel = Arc::new(AtomicBool::new(false));
    let progress = Arc::new(AtomicU32::new(0));
    let (tx, rx) = mpsc::channel();
    let worker_cancel = Arc::clone(&cancel);
    let worker_progress = Arc::clone(&progress);
    std::thread::spawn(move || {
        let result = run(job, &worker_progress, &worker_cancel);
        let _ = tx.send(result);
    });
    ExportHandle {
        rx,
        cancel,
        progress,
    }
}

fn run(mut job: ExportJob, progress: &AtomicU32, cancel: &AtomicBool) -> Result<PathBuf> {
    if job.clips.is_empty() {
        bail!("no unmuted raw takes to export");
    }
    let sr = job.sample_rate as f32;

    // A private, deterministic rig built from the frozen snapshot.
    let params = Arc::new(Params::new());
    job.rig.apply(&params);
    params.cab_external_loaded.store(false, Relaxed);
    params.cab_external_active.store(false, Relaxed);
    let mut chain = DspChain::new(sr, Arc::clone(&params));

    // External CLAP insert / AU amp, built on this worker thread from captured
    // state. Both keep-alives are held for the whole render.
    let _insert_keepalive = if let Some(build) = job.insert.take() {
        let instance = build()?;
        chain.set_insert(Some(instance.insert));
        Some(instance.keepalive)
    } else {
        None
    };
    let _amp_keepalive = if let Some(build) = job.amp.take() {
        let instance = build()?;
        let (amp_only, latency_frames) = match instance.placement {
            ExternalPlacement::Amp {
                amp_only,
                latency_frames,
            } => (amp_only, latency_frames),
            ExternalPlacement::Insert => (false, 0),
        };
        params.amp_external_loaded.store(true, Relaxed);
        params.amp_external_active.store(true, Relaxed);
        params.amp_external_amp_only.store(amp_only, Relaxed);
        params.amp_external_latency.store(latency_frames, Relaxed);
        chain.set_ext_amp(Some(instance.insert));
        Some(instance.keepalive)
    } else {
        None
    };

    if let Some(path) = &job.ir_path {
        match load_ir(path, sr, MAX_IR_LEN) {
            Ok(loaded) => {
                if job.ir_active {
                    params.cab_external_loaded.store(true, Relaxed);
                    params.cab_external_active.store(true, Relaxed);
                    let _ =
                        chain.replace_external_cab(Some(Box::new(ExternalIrCab::new(sr, loaded))));
                }
            }
            // A selected IR that cannot be re-loaded would silently render through
            // the built-in cab, which is not a faithful export.
            Err(e) if job.ir_active => {
                bail!("loading external IR {}: {e}", path.display());
            }
            Err(_) => {}
        }
    }
    render_with_chain(&job, progress, cancel, chain)
}

/// Decode every clip, then stream the mixed take bus through `chain`.
fn render_with_chain(
    job: &ExportJob,
    progress: &AtomicU32,
    cancel: &AtomicBool,
    mut chain: DspChain,
) -> Result<PathBuf> {
    let sr = job.sample_rate as f32;
    let rate = f64::from(job.project_sample_rate);

    let mut decoded: Vec<DecodedClip> = Vec::with_capacity(job.clips.len());
    for clip in &job.clips {
        let sample = decode_track(&clip.path, sr)
            .with_context(|| format!("decoding {}", clip.path.display()))?;
        let l = sample.track.l;
        let r = sample.track.r;
        let n = l.len().min(r.len());
        // Raw takes are mono (duplicated to stereo on install); sum back to mono.
        let mono: Vec<f32> = (0..n).map(|i| 0.5 * (l[i] + r[i])).collect();
        let start = if rate > 0.0 {
            (clip.start_ticks as f64 * f64::from(sr) / rate).round() as usize
        } else {
            clip.start_ticks as usize
        };
        decoded.push(DecodedClip {
            start,
            mono,
            gain: clip.gain,
        });
    }

    let program = decoded
        .iter()
        .map(|c| c.start + c.mono.len())
        .max()
        .unwrap_or(0);
    let tail_cap = (sr * TAIL_CAP_SECS) as usize;
    let total_estimate = program.saturating_add(tail_cap).max(1);

    // Write to a temp file beside the destination so a failure never leaves a
    // partial file where the user expects the finished WAV.
    let tmp = job.dest.with_extension("wav.tmp");
    if let Some(parent) = job.dest.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: job.sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(&tmp, spec)
        .with_context(|| format!("creating {}", tmp.display()))?;

    let mut mono = vec![0.0f32; BLOCK];
    let mut left = vec![0.0f32; BLOCK];
    let mut right = vec![0.0f32; BLOCK];
    let mut written = 0usize;
    let hold_frames = (sr * TAIL_HOLD_SECS) as usize;
    let mut silent_for = 0usize;

    while written < program.saturating_add(tail_cap) {
        if cancel.load(Relaxed) {
            drop(writer);
            let _ = std::fs::remove_file(&tmp);
            bail!("export cancelled");
        }
        let remaining = program.saturating_add(tail_cap).saturating_sub(written);
        let n = BLOCK.min(remaining);
        mono[..n].fill(0.0);
        for clip in &decoded {
            // Overlap-add this clip's contribution to the block.
            let clip_end = clip.start + clip.mono.len();
            let block_start = written;
            let block_end = written + n;
            let lo = block_start.max(clip.start);
            let hi = block_end.min(clip_end);
            if lo < hi {
                for out in lo..hi {
                    mono[out - block_start] += clip.mono[out - clip.start] * clip.gain;
                }
            }
        }

        chain.process_block(&mono[..n], &mut left[..n], &mut right[..n]);

        for i in 0..n {
            writer.write_sample(left[i])?;
            writer.write_sample(right[i])?;
        }

        if written >= program {
            let peak = left[..n]
                .iter()
                .chain(right[..n].iter())
                .fold(0.0f32, |m, &x| m.max(x.abs()));
            if peak < TAIL_THRESHOLD {
                silent_for += n;
                if silent_for >= hold_frames {
                    break;
                }
            } else {
                silent_for = 0;
            }
        }

        written += n;
        let permille = (written.saturating_mul(1000) / total_estimate).min(1000) as u32;
        progress.store(permille, Relaxed);
    }

    writer.finalize().context("finalizing export WAV")?;
    std::fs::rename(&tmp, &job.dest)
        .with_context(|| format!("moving export into {}", job.dest.display()))?;
    progress.store(1000, Relaxed);
    Ok(job.dest.clone())
}

struct DecodedClip {
    start: usize,
    mono: Vec<f32>,
    gain: f32,
}

/// Build a snapshot [`Preset`] for a job from live parameters. Kept here so the
/// UI can freeze the rig without knowing the preset internals.
pub fn snapshot_rig(params: &Params) -> Preset {
    Preset::from_params("export".to_owned(), None, params)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_mono_wav(path: &std::path::Path, sr: u32, samples: &[f32]) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: sr,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(path, spec).expect("wav");
        for &s in samples {
            w.write_sample(s).expect("sample");
        }
        w.finalize().expect("finalize");
    }

    #[test]
    fn exports_a_take_to_stereo_float() {
        let dir =
            std::env::temp_dir().join(format!("rusty-amp-export-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let take = dir.join("take.wav");
        // A short decaying tone so the rig produces something.
        let samples: Vec<f32> = (0..4800).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        write_mono_wav(&take, 48_000, &samples);

        let params = Params::new();
        let job = ExportJob {
            dest: dir.join("out.wav"),
            sample_rate: 48_000,
            project_sample_rate: 48_000,
            clips: vec![ExportClip {
                path: take,
                start_ticks: 0,
                gain: 1.0,
            }],
            rig: snapshot_rig(&params),
            ir_path: None,
            ir_active: false,
            insert: None,
            amp: None,
        };
        let cancel = AtomicBool::new(false);
        let progress = AtomicU32::new(0);
        let out = run(job, &progress, &cancel).expect("export");
        assert!(out.exists());

        let mut reader = hound::WavReader::open(&out).expect("open out");
        let spec = reader.spec();
        assert_eq!(spec.channels, 2);
        assert_eq!(spec.sample_rate, 48_000);
        assert!(spec.bits_per_sample == 32);
        let frames = reader.samples::<f32>().count() / 2;
        assert!(frames >= samples.len(), "export shorter than the take");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_job_is_rejected() {
        let dir =
            std::env::temp_dir().join(format!("rusty-amp-export-empty-{}", std::process::id()));
        let params = Params::new();
        let job = ExportJob {
            dest: dir.join("out.wav"),
            sample_rate: 48_000,
            project_sample_rate: 48_000,
            clips: Vec::new(),
            rig: snapshot_rig(&params),
            ir_path: None,
            ir_active: false,
            insert: None,
            amp: None,
        };
        let cancel = AtomicBool::new(false);
        let progress = AtomicU32::new(0);
        let err = run(job, &progress, &cancel).unwrap_err();
        assert!(err.to_string().contains("no unmuted"), "{err}");
    }
}

use anyhow::{Result, anyhow};
use cpal::{
    Device, FromSample, Sample, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use rtrb::{Consumer, Producer, RingBuffer};
use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;

use crate::dsp::cab::ExternalIrCab;
use crate::dsp::metronome::{Metronome, MetronomeVoice};
use crate::dsp::player::{PlayerTrack, PlayerVoice};
use crate::dsp::tuner::{Tuner, TunerDetector};
use crate::dsp::{DspChain, Levels, Params, StereoInsert};
use crate::practice::Practice;
use crate::recording::RecordingState;

/// A swappable plugin insert handed to the audio thread (`Some` to install, `None`
/// to clear). Boxed so the audio thread only ever moves a pointer.
type InsertCommand = Option<Box<dyn StereoInsert>>;

/// A swappable external-IR cab handed to the audio thread (`Some` to install, `None`
/// to clear). Boxed for the same reason as [`InsertCommand`].
type ExtCabCommand = Option<Box<ExternalIrCab>>;

/// A swappable external amp (a hosted plugin) handed to the audio thread (`Some` to
/// install, `None` to clear). Boxed like [`InsertCommand`].
type ExtAmpCommand = Option<Box<dyn StereoInsert>>;

/// How many pending insert swaps / disposals the lock-free rings can hold. Swaps
/// are rare (a user loading/clearing a plugin), so a small buffer is plenty.
const INSERT_QUEUE_CAP: usize = 8;

/// Largest block (in frames) the audio thread will ever process at once. Scratch
/// buffers are pre-sized to this, and plugin inserts are activated with it as
/// their maximum block size.
pub const MAX_BLOCK: usize = 4096;
/// Frames per audio callback we request from the OS. 256 ≈ 5.3 ms at 48 kHz:
/// tight enough that playing feels connected (stock DAWs run 64–256), loose
/// enough that a release build never underruns on Apple Silicon. Without an
/// explicit request CoreAudio may hand us 1024+ frames, which feels spongy and
/// makes even a clean DI sound dull and distant. Always test audio on
/// `cargo run --release` — debug builds can underrun at this size.
const LIVE_BUFFER_FRAMES: u32 = 256;

/// ALSA-specific period size. Unlike CoreAudio, cpal's `BufferSize::Fixed` on
/// ALSA sets the transfer *period* (the per-callback chunk), not a total buffer,
/// and cpal's ALSA thread runs without realtime priority — so a 256-frame period
/// (~5.3 ms) is easily missed under load and the capture stream overruns
/// continuously ("A buffer underrun or overrun occurred"). A larger period is
/// far more forgiving and still feels live.
#[cfg(target_os = "linux")]
const ALSA_PERIOD_FRAMES: u32 = 1024;

/// The fixed period to request for this host's streams.
fn requested_frames(host: &cpal::Host) -> u32 {
    #[cfg(target_os = "linux")]
    if host.id() == cpal::HostId::Alsa {
        return ALSA_PERIOD_FRAMES;
    }
    let _ = host;
    LIVE_BUFFER_FRAMES
}

/// Appends one timestamped line to `~/.config/rusty-amp/audio.log`.
///
/// Diagnostics must go here, not stderr: the TUI runs in the terminal's
/// alternate screen, so anything printed while it is active scrolls into a
/// hidden buffer that is discarded on quit and the user never sees it.
/// Logging is best-effort — it must never fail audio startup.
pub fn log_line(msg: &str) {
    let path = dirs::home_dir().map(|h| h.join(".config/rusty-amp/audio.log"));
    if let Some(path) = path {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            use std::io::Write as _;
            let _ = writeln!(f, "[{secs}] {msg}");
        }
    }
}

pub struct AudioEngine {
    _input_stream: Stream,
    _output_stream: Stream,
    /// Negotiated sample rate of the running streams (Hz).
    sample_rate: f32,
    /// Sends insert swaps to the audio thread (consumed at the top of its callback).
    insert_tx: Producer<InsertCommand>,
    /// Receives inserts the audio thread displaced, so they are dropped here on a
    /// non-audio thread rather than freed in the realtime callback.
    dropped_rx: Consumer<Box<dyn StereoInsert>>,
    /// Sends external-IR cab swaps to the audio thread.
    ext_cab_tx: Producer<ExtCabCommand>,
    /// Receives external-IR cabs the audio thread displaced, for off-thread disposal.
    ext_dropped_rx: Consumer<Box<ExternalIrCab>>,
    /// Sends external-amp swaps to the audio thread.
    ext_amp_tx: Producer<ExtAmpCommand>,
    /// Receives external amps the audio thread displaced, for off-thread disposal.
    ext_amp_dropped_rx: Consumer<Box<dyn StereoInsert>>,
    /// Sends practice backing-track swaps to the audio thread.
    backing_tx: Producer<Option<PlayerTrack>>,
    /// Receives backing tracks the audio thread displaced, for off-thread disposal.
    backing_dropped_rx: Consumer<PlayerTrack>,
    /// Sends practice take-track swaps to the audio thread.
    record_tx: Producer<Option<PlayerTrack>>,
    /// Receives take tracks the audio thread displaced, for off-thread disposal.
    record_dropped_rx: Consumer<PlayerTrack>,
}

impl AudioEngine {
    /// The sample rate (Hz) the engine negotiated and is running at.
    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// Install (`Some`) or clear (`None`) the third-party plugin insert.
    ///
    /// Call this from the UI/control thread, never the audio thread. The actual
    /// swap happens lock-free inside the audio callback; any previously installed
    /// insert is disposed of here, on the caller's thread.
    pub fn set_plugin_insert(&mut self, insert: InsertCommand) -> Result<()> {
        // Dispose of anything the audio thread has handed back since last time.
        while let Ok(old) = self.dropped_rx.pop() {
            drop(old);
        }
        self.insert_tx
            .push(insert)
            .map_err(|_| anyhow!("plugin-insert command queue is full"))
    }

    /// Install (`Some`) or clear (`None`) the external-IR cab.
    ///
    /// Call from the UI/control thread. The swap happens lock-free in the audio
    /// callback; any displaced cab is disposed of here, on the caller's thread, so
    /// its IR/FFT buffers are never freed in the realtime path. Build the
    /// [`ExternalIrCab`] (decode + resample) before calling — that work is offline.
    pub fn set_external_cab(&mut self, cab: ExtCabCommand) -> Result<()> {
        while let Ok(old) = self.ext_dropped_rx.pop() {
            drop(old);
        }
        self.ext_cab_tx
            .push(cab)
            .map_err(|_| anyhow!("external-cab command queue is full"))
    }

    /// Install (`Some`) or clear (`None`) the external amp override (a hosted plugin).
    ///
    /// Call from the UI/control thread. The swap happens lock-free in the audio
    /// callback; any displaced amp is disposed of here, on the caller's thread, so a
    /// plugin is never freed in the realtime path. Build the plugin (via the `host`
    /// module) before calling.
    pub fn set_external_amp(&mut self, amp: ExtAmpCommand) -> Result<()> {
        while let Ok(old) = self.ext_amp_dropped_rx.pop() {
            drop(old);
        }
        self.ext_amp_tx
            .push(amp)
            .map_err(|_| anyhow!("external-amp command queue is full"))
    }

    /// Install (`Some`) or clear (`None`) the practice backing track.
    ///
    /// Decode and rate-match the file first (see [`crate::practice::decode_track`]) —
    /// that work is offline; this call only hands the finished buffers to the audio
    /// thread lock-free. The displaced track is disposed of here, on the caller's
    /// thread, so its sample buffer is never freed in the realtime callback.
    pub fn set_backing_track(&mut self, track: Option<PlayerTrack>) -> Result<()> {
        while let Ok(old) = self.backing_dropped_rx.pop() {
            drop(old);
        }
        self.backing_tx
            .push(track)
            .map_err(|_| anyhow!("backing-track command queue is full"))
    }

    /// Install (`Some`) or clear (`None`) the practice take track. Same contract as
    /// [`Self::set_backing_track`].
    pub fn set_record_track(&mut self, track: Option<PlayerTrack>) -> Result<()> {
        while let Ok(old) = self.record_dropped_rx.pop() {
            drop(old);
        }
        self.record_tx
            .push(track)
            .map_err(|_| anyhow!("record-track command queue is full"))
    }
}

pub struct InputInfo {
    pub name: String,
    pub channels: usize,
}

pub struct DeviceInfo {
    pub inputs: Vec<InputInfo>,
    pub outputs: Vec<String>,
}

/// True for the ALSA `null` sink/source, which cpal lists as
/// "Discard all samples (playback) or generate zero samples (capture)".
fn is_null_device(name: &str) -> bool {
    name.contains("Discard all samples") || name.eq_ignore_ascii_case("null")
}

/// Enumerates input devices, dropping the `null` pseudo-device and collapsing
/// entries that share a `(name, channels)` pair. The raw ALSA surface exposes
/// every PCM (`sysdefault`, `front`, `surround*`, …) under the same human name;
/// showing them all is noise, and after filtering the indices stay stable
/// between this list and [`start`].
fn collect_inputs(host: &cpal::Host) -> Result<Vec<(Device, InputInfo)>> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (i, d) in host.input_devices()?.enumerate() {
        let name = d
            .description()
            .map(|desc| desc.name().to_owned())
            .unwrap_or_else(|_| format!("device-{i}"));
        if is_null_device(&name) {
            continue;
        }
        let channels = d
            .default_input_config()
            .map(|c| c.channels() as usize)
            .unwrap_or(1);
        if seen.insert((name.clone(), channels)) {
            out.push((d, InputInfo { name, channels }));
        }
    }
    Ok(out)
}

/// Output counterpart to [`collect_inputs`]: drops `null` and de-duplicates by
/// device name.
fn collect_outputs(host: &cpal::Host) -> Result<Vec<(Device, String)>> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (i, d) in host.output_devices()?.enumerate() {
        let name = d
            .description()
            .map(|desc| desc.name().to_owned())
            .unwrap_or_else(|_| format!("device-{i}"));
        if is_null_device(&name) {
            continue;
        }
        if seen.insert(name.clone()) {
            out.push((d, name));
        }
    }
    Ok(out)
}

pub fn list_devices() -> Result<DeviceInfo> {
    let host = cpal::default_host();
    let inputs = collect_inputs(&host)?
        .into_iter()
        .map(|(_, info)| info)
        .collect();
    let outputs = collect_outputs(&host)?
        .into_iter()
        .map(|(_, name)| name)
        .collect();
    Ok(DeviceInfo { inputs, outputs })
}

/// Where the last good device selection is remembered.
fn selection_path() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|h| h.join(".config/rusty-amp/audio.conf"))
}

/// Persist the chosen devices by *name* rather than index, so the selection
/// survives a USB interface re-enumerating at a different position. Best-effort:
/// a failed write must never block startup.
pub fn save_selection(devices: &DeviceInfo, input_idx: usize, guitar_ch: usize, output_idx: usize) {
    let Some(path) = selection_path() else { return };
    let Some(input) = devices.inputs.get(input_idx) else {
        return;
    };
    let Some(output) = devices.outputs.get(output_idx) else {
        return;
    };
    let body = format!(
        "# rusty-amp device selection — delete this file (or launch with \
         RUSTY_AMP_DEVICE_PROMPT=1) to be prompted again\n\
         input_name = {}\n\
         input_channels = {}\n\
         guitar_channel = {}\n\
         output_name = {}\n",
        input.name, input.channels, guitar_ch, output,
    );
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, body);
}

/// Resolve a saved selection against the current device list. Returns `None`
/// when nothing is saved, the file is malformed, the devices are gone, or the
/// saved channel no longer exists — the caller then shows the interactive
/// picker instead.
pub fn load_selection(devices: &DeviceInfo) -> Option<(usize, usize, usize)> {
    let text = std::fs::read_to_string(selection_path()?).ok()?;

    let mut input_name: Option<String> = None;
    let mut input_channels: Option<usize> = None;
    let mut guitar_channel: Option<usize> = None;
    let mut output_name: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "input_name" => input_name = Some(value.to_owned()),
            "input_channels" => input_channels = value.parse::<usize>().ok(),
            "guitar_channel" => guitar_channel = value.parse::<usize>().ok(),
            "output_name" => output_name = Some(value.to_owned()),
            _ => {}
        }
    }

    let input_idx = devices.inputs.iter().position(|d| {
        Some(d.name.as_str()) == input_name.as_deref() && Some(d.channels) == input_channels
    })?;
    let output_idx = devices
        .outputs
        .iter()
        .position(|n| Some(n.as_str()) == output_name.as_deref())?;
    let guitar_channel = guitar_channel?;
    if guitar_channel >= devices.inputs[input_idx].channels {
        return None;
    }
    Some((input_idx, guitar_channel, output_idx))
}

#[allow(clippy::too_many_arguments)]
pub fn start(
    input_idx: usize,
    guitar_ch: usize,
    output_idx: usize,
    params: Arc<Params>,
    levels: Arc<Levels>,
    recording: Arc<RecordingState>,
    tuner: Arc<Tuner>,
    metronome: Arc<Metronome>,
    practice: Arc<Practice>,
) -> Result<AudioEngine> {
    let host = cpal::default_host();

    let input_device = collect_inputs(&host)?
        .into_iter()
        .nth(input_idx)
        .map(|(d, _)| d)
        .ok_or_else(|| anyhow!("Input device index {input_idx} not found"))?;

    let output_device = collect_outputs(&host)?
        .into_iter()
        .nth(output_idx)
        .map(|(d, _)| d)
        .ok_or_else(|| anyhow!("Output device index {output_idx} not found"))?;

    let (input_cfg, output_cfg, sr, in_fmt, out_fmt) =
        negotiate_configs(&input_device, &output_device)?;

    let in_channels = input_cfg.channels as usize;
    let out_channels = output_cfg.channels as usize;

    let input_name = input_device
        .description()
        .map(|desc| desc.name().to_owned())
        .unwrap_or_else(|_| format!("input-{input_idx}"));
    let output_name = output_device
        .description()
        .map(|desc| desc.name().to_owned())
        .unwrap_or_else(|_| format!("output-{output_idx}"));
    let shown_ch = guitar_ch + 1;
    let shown_sr = sr as u32;
    let frames = requested_frames(&host);
    let msg = format!(
        "Audio: in '{input_name}' ch {shown_ch}/{in_channels} ({in_fmt}) -> out '{output_name}' ch {out_channels} ({out_fmt}), {shown_sr} Hz, requesting buffer {frames} frames",
    );
    // Both: stderr for pre-TUI failures, log file for everything after
    // (stderr is invisible once the alternate screen is up).
    eprintln!("{msg}");
    log_line(&msg);

    // Ask both directions for a small callback first; a device that rejects the
    // fixed size (common with PipeWire's ALSA plugin) gets a second chance with
    // the backend default rather than failing startup outright.
    let fixed = cpal::BufferSize::Fixed(frames);
    match build_engine(
        &input_device,
        with_buffer(&input_cfg, fixed),
        in_channels,
        guitar_ch,
        in_fmt,
        &output_device,
        with_buffer(&output_cfg, fixed),
        out_channels,
        out_fmt,
        sr,
        Arc::clone(&params),
        Arc::clone(&levels),
        Arc::clone(&recording),
        Arc::clone(&tuner),
        Arc::clone(&metronome),
        Arc::clone(&practice),
    ) {
        Ok(engine) => Ok(engine),
        Err(err) => {
            let msg = format!(
                "Audio: stream build with a {frames}-frame request failed ({err}); retrying with backend default buffer",
            );
            eprintln!("{msg}");
            log_line(&msg);
            build_engine(
                &input_device,
                with_buffer(&input_cfg, cpal::BufferSize::Default),
                in_channels,
                guitar_ch,
                in_fmt,
                &output_device,
                with_buffer(&output_cfg, cpal::BufferSize::Default),
                out_channels,
                out_fmt,
                sr,
                params,
                levels,
                recording,
                tuner,
                metronome,
                practice,
            )
        }
    }
}

fn negotiate_configs(
    input: &Device,
    output: &Device,
) -> Result<(
    StreamConfig,
    StreamConfig,
    f32,
    cpal::SampleFormat,
    cpal::SampleFormat,
)> {
    let in_sup = input.default_input_config()?;
    let in_sr = in_sup.sample_rate();
    let in_fmt = in_sup.sample_format();

    // Prefer the output's *default* config when it runs at the input's rate: it is
    // the format the device (or its host) actually wants — e.g. f32 through
    // PipeWire, or native i32 on a raw ALSA `hw` PCM. Falling through to the
    // supported-range list and picking by channel count alone can otherwise land
    // on an exotic format like f64, forcing an expensive plug-layer conversion.
    let out_default = output.default_output_config().ok();
    let preferred_channels = out_default.as_ref().map(|c| c.channels());
    let out_sup = match out_default {
        Some(d) if d.sample_rate() == in_sr => d,
        _ => {
            // ALSA advertises a config range per channel count and the first match
            // is often mono, which would silently collapse the rig's stereo image.
            // Prefer the default channel count at the input's rate, then a
            // widely-supported sample format, then the widest config.
            let chosen = output
                .supported_output_configs()?
                .filter(|r| r.min_sample_rate() <= in_sr && r.max_sample_rate() >= in_sr)
                .max_by_key(|r| {
                    (
                        Some(r.channels()) == preferred_channels,
                        format_rank(r.sample_format()),
                        r.channels(),
                    )
                });
            match chosen {
                Some(range) => range.with_sample_rate(in_sr),
                None => {
                    let default = output.default_output_config().map_err(|e| {
                        anyhow!(
                            "output has no supported config for {in_sr} Hz and its default config is unavailable: {e}"
                        )
                    })?;
                    let fallback_sr = default.sample_rate();
                    let msg = format!(
                        "Audio: output does not support {in_sr} Hz; falling back to its default {fallback_sr} Hz"
                    );
                    eprintln!("{msg}");
                    log_line(&msg);
                    default
                }
            }
        }
    };
    let out_fmt = out_sup.sample_format();

    // Buffer size is chosen by the caller: `start` first requests the small
    // [`LIVE_BUFFER_FRAMES`] size and falls back to the backend default if the
    // device rejects it.
    let in_cfg: StreamConfig = in_sup.into();
    let out_cfg: StreamConfig = out_sup.into();

    Ok((in_cfg, out_cfg, in_sr as f32, in_fmt, out_fmt))
}

/// Rank sample formats by how cheaply and universally they convert: prefer native
/// integer and f32 formats over f64 (which forces an ALSA plug-layer conversion).
fn format_rank(f: cpal::SampleFormat) -> u8 {
    match f {
        cpal::SampleFormat::F32 => 5,
        cpal::SampleFormat::I32 => 4,
        cpal::SampleFormat::I16 => 3,
        cpal::SampleFormat::U16 => 2,
        cpal::SampleFormat::I24 => 1,
        cpal::SampleFormat::U24 => 1,
        _ => 0,
    }
}

/// Returns a copy of `cfg` with its requested buffer size replaced.
fn with_buffer(cfg: &StreamConfig, buffer_size: cpal::BufferSize) -> StreamConfig {
    StreamConfig {
        buffer_size,
        ..*cfg
    }
}

/// All state owned by the audio *input* callback.
///
/// Bundling it in one struct lets the same block-processing code be compiled for
/// whichever sample format the device requires — some interfaces (e.g. a Focusrite
/// Scarlett Solo) expose `S32_LE` capture rather than `f32`, so hard-coding an f32
/// stream fails to open at all. `on_input` converts the device's samples into the
/// engine's `f32` domain on the way in.
struct InputState {
    chain: DspChain,
    tuner_detector: TunerDetector,
    metro_voice: MetronomeVoice,
    player: PlayerVoice,
    attack: f32,
    release: f32,
    in_env: f32,
    out_env: f32,
    in_channels: usize,
    out_channels: usize,
    guitar_ch: usize,
    in_buf: Vec<f32>,
    out_l: Vec<f32>,
    out_r: Vec<f32>,
    insert_rx: Consumer<InsertCommand>,
    dropped_tx: Producer<Box<dyn StereoInsert>>,
    ext_cab_rx: Consumer<ExtCabCommand>,
    ext_dropped_tx: Producer<Box<ExternalIrCab>>,
    ext_amp_rx: Consumer<ExtAmpCommand>,
    ext_amp_dropped_tx: Producer<Box<dyn StereoInsert>>,
    backing_rx: Consumer<Option<PlayerTrack>>,
    backing_dropped_tx: Producer<PlayerTrack>,
    record_rx: Consumer<Option<PlayerTrack>>,
    record_dropped_tx: Producer<PlayerTrack>,
    producer: Producer<f32>,
    levels: Arc<Levels>,
    recording: Arc<RecordingState>,
    tuner: Arc<Tuner>,
    metronome: Arc<Metronome>,
    practice: Arc<Practice>,
}

impl InputState {
    /// Deinterleave, process and fan the input block back out to the ring buffer
    /// the output callback drains. Samples arrive as the device's `T` and are
    /// converted to `f32` on the way in.
    fn on_input<T>(&mut self, data: &[T])
    where
        T: Sample,
        f32: FromSample<T>,
    {
        // Apply any pending insert swaps before processing this block. The old
        // insert is shipped back to the control thread for disposal; if that
        // queue is somehow full we drop it here as a last resort.
        while let Ok(cmd) = self.insert_rx.pop() {
            if let Some(old) = self.chain.replace_insert(cmd) {
                let _ = self.dropped_tx.push(old);
            }
        }
        // Same lock-free discipline for external-IR cab swaps.
        while let Ok(cmd) = self.ext_cab_rx.pop() {
            if let Some(old) = self.chain.replace_external_cab(cmd) {
                let _ = self.ext_dropped_tx.push(old);
            }
        }
        // ...and for external-amp swaps.
        while let Ok(cmd) = self.ext_amp_rx.pop() {
            if let Some(old) = self.chain.replace_ext_amp(cmd) {
                let _ = self.ext_amp_dropped_tx.push(old);
            }
        }
        // Practice backing/take tracks: install the decoded buffers, ship the
        // displaced ones back for off-thread disposal.
        while let Ok(cmd) = self.backing_rx.pop() {
            if let Some(old) = self.player.set_backing(cmd) {
                let _ = self.backing_dropped_tx.push(old);
            }
        }
        while let Ok(cmd) = self.record_rx.pop() {
            if let Some(old) = self.player.set_record(cmd) {
                let _ = self.record_dropped_tx.push(old);
            }
        }

        let frames = data.len() / self.in_channels;
        if self.out_l.len() < frames {
            self.out_l.resize(frames, 0.0);
            self.out_r.resize(frames, 0.0);
        }

        // Deinterleave the guitar channel into the mono input block, converting
        // from the device's sample type to the engine's f32 domain.
        self.in_buf.clear();
        self.in_buf
            .extend(data.chunks(self.in_channels).map(|frame| {
                f32::from_sample(frame.get(self.guitar_ch).copied().unwrap_or(T::EQUILIBRIUM))
            }));

        if self.tuner.active.load(Relaxed) {
            // Bypass the whole rig: clean dry guitar to both channels, and
            // analyse the same signal for pitch and spectrum.
            self.tuner_detector.process(&self.in_buf, &self.tuner);
            for ((dst_l, dst_r), &x) in self
                .out_l
                .iter_mut()
                .zip(self.out_r.iter_mut())
                .zip(self.in_buf.iter())
            {
                *dst_l = x;
                *dst_r = x;
            }
        } else {
            self.chain
                .process_block(&self.in_buf, &mut self.out_l, &mut self.out_r);
        }

        let metro_active = self.metronome.active.load(Relaxed);
        let metro_bpm = self.metronome.bpm.load(Relaxed);

        // One transport snapshot per block (and any pending seek applied here).
        let transport = self.practice.snapshot();
        self.player.begin(&transport);

        for ((&sample, &l), &r) in self
            .in_buf
            .iter()
            .zip(self.out_l.iter())
            .zip(self.out_r.iter())
        {
            let a = sample.abs();
            self.in_env += if a > self.in_env {
                self.attack
            } else {
                self.release
            } * (a - self.in_env);

            let mono = 0.5 * (l + r);

            let a = mono.abs();
            self.out_env += if a > self.out_env {
                self.attack
            } else {
                self.release
            } * (a - self.out_env);

            if self.recording.active.load(Relaxed)
                && let Ok(mut buf) = self.recording.buffer.try_lock()
            {
                // Interleaved stereo (L, R) — captured before the metronome
                // click is added, so an active metronome never lands in the WAV.
                buf.push(l);
                buf.push(r);
            }

            // Metronome click and the practice player are mixed into the monitor
            // path only (post-record), so neither ever lands in the WAV.
            let click = self.metro_voice.next_sample(metro_active, metro_bpm);
            let (pl, pr) = self.player.next_frame(&transport);
            let (out_left, out_right) = (l + click + pl, r + click + pr);
            let out_mono = mono + click + 0.5 * (pl + pr);

            // Fan the stereo pair out to the device channels: L→0, R→1, any extra
            // channels get the mono sum; a mono device gets the sum.
            for ch in 0..self.out_channels {
                let s = if self.out_channels == 1 {
                    out_mono
                } else {
                    match ch {
                        0 => out_left,
                        1 => out_right,
                        _ => out_mono,
                    }
                };
                let _ = self.producer.push(s);
            }
        }
        // Publish the timeline cursor for the UI. Stored after the loop so a
        // block's worth of playback shows as one position.
        self.practice.store_position(self.player.cursor());
        self.levels.input.store(self.in_env, Relaxed);
        self.levels.output.store(self.out_env, Relaxed);
    }
}

/// Build the input stream for a concrete device sample type.
fn build_input_stream<T>(
    device: &Device,
    cfg: StreamConfig,
    mut state: InputState,
) -> Result<Stream>
where
    T: cpal::SizedSample,
    f32: FromSample<T>,
{
    let mut err_count = 0u64;
    device
        .build_input_stream(
            cfg,
            move |data: &[T], _| state.on_input(data),
            move |e| {
                // An XRUN is transient and cpal recovers; logging every one floods
                // the log (and stderr) during a bad patch, so report the first and
                // then only occasionally.
                err_count += 1;
                if err_count == 1 || err_count.is_multiple_of(200) {
                    let msg = format!("input error: {e} (occurrence {err_count})");
                    eprintln!("{msg}");
                    log_line(&msg);
                }
            },
            None,
        )
        .map_err(|e| anyhow!("input stream: {e}"))
}

/// Build the output stream for a concrete device sample type.
fn build_output_stream<T>(
    device: &Device,
    cfg: StreamConfig,
    mut consumer: Consumer<f32>,
) -> Result<Stream>
where
    T: cpal::SizedSample + FromSample<f32>,
{
    let mut err_count = 0u64;
    device
        .build_output_stream(
            cfg,
            move |data: &mut [T], _| {
                for s in data.iter_mut() {
                    *s = T::from_sample(consumer.pop().unwrap_or(0.0));
                }
            },
            move |e| {
                err_count += 1;
                if err_count == 1 || err_count.is_multiple_of(200) {
                    let msg = format!("output error: {e} (occurrence {err_count})");
                    eprintln!("{msg}");
                    log_line(&msg);
                }
            },
            None,
        )
        .map_err(|e| anyhow!("output stream: {e}"))
}

#[allow(clippy::too_many_arguments)]
fn build_engine(
    input_device: &Device,
    input_cfg: StreamConfig,
    in_channels: usize,
    guitar_ch: usize,
    in_fmt: cpal::SampleFormat,
    output_device: &Device,
    output_cfg: StreamConfig,
    out_channels: usize,
    out_fmt: cpal::SampleFormat,
    sr: f32,
    params: Arc<Params>,
    levels: Arc<Levels>,
    recording: Arc<RecordingState>,
    tuner: Arc<Tuner>,
    metronome: Arc<Metronome>,
    practice: Arc<Practice>,
) -> Result<AudioEngine> {
    recording.sample_rate.store(sr as u32, Relaxed);

    let buf_samples = (sr as usize) / 5 * out_channels * 2;
    let (producer, consumer) = RingBuffer::<f32>::new(buf_samples);

    let chain = DspChain::new(sr, Arc::clone(&params));

    // Tuner: when engaged, the rig is bypassed and the dry guitar feeds both the
    // output (a clean signal to tune against) and the pitch/spectrum detector.
    let tuner_detector = TunerDetector::new(sr);

    // Metronome: when engaged, a click is mixed into the monitor output only —
    // added *after* the recording tap so it is never captured in the WAV.
    let metro_voice = MetronomeVoice::new(sr);

    // Practice player: backing track + recorded take, both mixed into the monitor
    // output only (post-record), like the metronome.
    let player = PlayerVoice::new();

    // Lock-free handoff for swapping the plugin insert in/out without touching the
    // running stream: commands flow UI → audio, displaced inserts flow back to be
    // dropped off the audio thread.
    let (insert_tx, insert_rx) = RingBuffer::<InsertCommand>::new(INSERT_QUEUE_CAP);
    let (dropped_tx, dropped_rx) = RingBuffer::<Box<dyn StereoInsert>>::new(INSERT_QUEUE_CAP);
    let (ext_cab_tx, ext_cab_rx) = RingBuffer::<ExtCabCommand>::new(INSERT_QUEUE_CAP);
    let (ext_dropped_tx, ext_dropped_rx) = RingBuffer::<Box<ExternalIrCab>>::new(INSERT_QUEUE_CAP);
    let (ext_amp_tx, ext_amp_rx) = RingBuffer::<ExtAmpCommand>::new(INSERT_QUEUE_CAP);
    let (ext_amp_dropped_tx, ext_amp_dropped_rx) =
        RingBuffer::<Box<dyn StereoInsert>>::new(INSERT_QUEUE_CAP);
    // Practice-track handoff: decoded tracks flow UI → audio, displaced tracks flow
    // back to the control thread so their buffers are never freed in the callback.
    let (backing_tx, backing_rx) = RingBuffer::<Option<PlayerTrack>>::new(INSERT_QUEUE_CAP);
    let (backing_dropped_tx, backing_dropped_rx) = RingBuffer::<PlayerTrack>::new(INSERT_QUEUE_CAP);
    let (record_tx, record_rx) = RingBuffer::<Option<PlayerTrack>>::new(INSERT_QUEUE_CAP);
    let (record_dropped_tx, record_dropped_rx) = RingBuffer::<PlayerTrack>::new(INSERT_QUEUE_CAP);

    let attack = 1.0 - (-1.0 / (0.001 * sr)).exp();
    let release = 1.0 - (-1.0 / (0.300 * sr)).exp();

    // Reusable scratch buffers for block processing. Pre-sized generously so the
    // audio thread never reallocates for normal device buffer sizes; the `resize`
    // below only grows them on the rare callback that asks for a larger block.
    let in_buf: Vec<f32> = Vec::with_capacity(MAX_BLOCK);
    let out_l: Vec<f32> = vec![0.0; MAX_BLOCK];
    let out_r: Vec<f32> = vec![0.0; MAX_BLOCK];

    let state = InputState {
        chain,
        tuner_detector,
        metro_voice,
        player,
        attack,
        release,
        in_env: 0.0,
        out_env: 0.0,
        in_channels,
        out_channels,
        guitar_ch,
        in_buf,
        out_l,
        out_r,
        insert_rx,
        dropped_tx,
        ext_cab_rx,
        ext_dropped_tx,
        ext_amp_rx,
        ext_amp_dropped_tx,
        backing_rx,
        backing_dropped_tx,
        record_rx,
        record_dropped_tx,
        producer,
        levels,
        recording,
        tuner,
        metronome,
        practice,
    };

    // Build each stream in the sample format the device actually supports. ALSA
    // exposes the Scarlett's capture as S32_LE, so an f32 stream would fail to
    // open; `on_input`/`build_output_stream` convert to and from the engine's f32
    // domain. Unsupported formats are reported clearly rather than silently
    // mis-configured.
    macro_rules! build_in {
        ($t:ty) => {
            build_input_stream::<$t>(input_device, input_cfg, state)?
        };
    }
    let input_stream = match in_fmt {
        cpal::SampleFormat::F32 => build_in!(f32),
        cpal::SampleFormat::F64 => build_in!(f64),
        cpal::SampleFormat::I8 => build_in!(i8),
        cpal::SampleFormat::I16 => build_in!(i16),
        cpal::SampleFormat::I24 => build_in!(cpal::I24),
        cpal::SampleFormat::I32 => build_in!(i32),
        cpal::SampleFormat::U8 => build_in!(u8),
        cpal::SampleFormat::U16 => build_in!(u16),
        cpal::SampleFormat::U24 => build_in!(cpal::U24),
        cpal::SampleFormat::U32 => build_in!(u32),
        other => return Err(anyhow!("input sample format {other} is not supported")),
    };

    macro_rules! build_out {
        ($t:ty) => {
            build_output_stream::<$t>(output_device, output_cfg, consumer)?
        };
    }
    let output_stream = match out_fmt {
        cpal::SampleFormat::F32 => build_out!(f32),
        cpal::SampleFormat::F64 => build_out!(f64),
        cpal::SampleFormat::I8 => build_out!(i8),
        cpal::SampleFormat::I16 => build_out!(i16),
        cpal::SampleFormat::I24 => build_out!(cpal::I24),
        cpal::SampleFormat::I32 => build_out!(i32),
        cpal::SampleFormat::U8 => build_out!(u8),
        cpal::SampleFormat::U16 => build_out!(u16),
        cpal::SampleFormat::U24 => build_out!(cpal::U24),
        cpal::SampleFormat::U32 => build_out!(u32),
        other => return Err(anyhow!("output sample format {other} is not supported")),
    };

    input_stream.play()?;
    output_stream.play()?;

    Ok(AudioEngine {
        _input_stream: input_stream,
        _output_stream: output_stream,
        sample_rate: sr,
        insert_tx,
        dropped_rx,
        ext_cab_tx,
        ext_dropped_rx,
        ext_amp_tx,
        ext_amp_dropped_rx,
        backing_tx,
        backing_dropped_rx,
        record_tx,
        record_dropped_rx,
    })
}

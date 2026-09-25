//! Session timeline pane and track browser.
//!
//! The pane is a scrollable list: a transport line followed by one row per
//! timeline track (name, kind, mute, gain, start/end and a mini waveform scaled
//! to the shared session extent). Everything editable is owned by the
//! control-thread [`Session`]; decoded playback buffers are caches installed
//! into the audio engine.
//!
//! The browser imports a file as a **new** track at the current playhead. Raw
//! takes are captured dry through the take-bus pipeline (see
//! [`crate::recording`]) and re-amped live by the current rig.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};

use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};
use std::sync::atomic::Ordering::Relaxed;

use super::styles::{ACCENT, AMBER, CHROME, DIM, HOT, SAFE, WARN};
use crate::audio::AudioEngine;
use crate::dsp::player::{MAX_TRACKS, TrackKind as PlayerKind};
use crate::practice::{DecodedTrack, Practice, decode_track, peaks};
use crate::recording::{CaptureCommand, CaptureResult, CaptureState, spawn_capture_worker};
use crate::session::{AssetRef, Session, TrackId, TrackKind, TrackLifecycle};

/// How many waveform buckets we keep per track for the mini display (mapped to
/// the pane width each frame). Enough detail for a glance without recomputing.
pub(super) const PEAK_BUCKETS: usize = 512;

/// One discovered audio file in the browser.
struct TrackFile {
    path: PathBuf,
    label: String,
    detail: String,
}

/// A background decode in flight, targeting a specific row id.
struct PendingDecode {
    id: TrackId,
    generation: u64,
    kind: TrackKind,
    path: PathBuf,
    rx: Receiver<Result<DecodedTrack, String>>,
}

/// Which row owns the pane cursor.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Selection {
    Transport,
    Track(TrackId),
}

/// All practice UI state, owned by the UI thread. Persists across a device
/// change: the [`Session`] and its recovery paths are the source of truth, and
/// decoded caches are rebuilt.
pub(super) struct PracticeUi {
    pub(super) browser_open: bool,
    browser_cursor: usize,
    files: Vec<TrackFile>,
    /// Typed path alternative to browsing.
    path_input: String,
    /// 0 = file list, 1 = path field.
    field: usize,
    message: Option<String>,
    sample_rate: f32,
    decodes: Vec<PendingDecode>,

    /// Canonical project state.
    pub(super) session: Session,
    selection: Selection,

    /// Capture plumbing (attached per engine).
    capture_cmd: Option<Sender<CaptureCommand>>,
    capture_result: Option<Receiver<CaptureResult>>,
    recording_id: Option<TrackId>,
    /// Small modal editing a track's gain.
    gain_edit: Option<TrackId>,
    next_generation: u64,
    /// The project time base is adopted from the first engine and kept across
    /// later device changes, so clip offsets never drift.
    rate_adopted: bool,
}

impl PracticeUi {
    pub(super) fn new() -> Self {
        Self {
            browser_open: false,
            browser_cursor: 0,
            files: Vec::new(),
            path_input: String::new(),
            field: 0,
            message: None,
            sample_rate: 48_000.0,
            decodes: Vec::new(),
            session: Session::new(48_000),
            selection: Selection::Transport,
            capture_cmd: None,
            capture_result: None,
            recording_id: None,
            gain_edit: None,
            next_generation: 1,
            rate_adopted: false,
        }
    }

    fn generation(&mut self) -> u64 {
        let g = self.next_generation;
        self.next_generation = self.next_generation.saturating_add(1);
        g
    }

    /// Adopt the running engine's rate and (re)attach the capture writer. Called
    /// once per engine start; keeps the session across device changes.
    pub(super) fn attach(&mut self, engine: &mut AudioEngine) {
        self.sample_rate = engine.sample_rate();
        // Adopt the project time base once; later device changes keep it.
        if !self.rate_adopted {
            self.session
                .set_project_sample_rate(self.sample_rate as u32);
            self.rate_adopted = true;
        }
        if let Some(consumer) = engine.take_capture_consumer() {
            self.capture_cmd = Some(spawn_capture_worker(consumer));
        }
        // Any decode that was in flight was resampled for the *old* rate; reissue
        // it at the new rate rather than installing a mismatched buffer.
        let inflight = std::mem::take(&mut self.decodes);
        self.reinstall_ready_tracks();
        for pending in inflight {
            self.start_decode(pending.id, pending.kind, pending.path);
        }
    }

    /// Re-decode every ready track from its asset into the fresh engine (device
    /// change). Missing files become error rows rather than silently vanishing.
    fn reinstall_ready_tracks(&mut self) {
        let ids: Vec<TrackId> = self
            .session
            .tracks()
            .iter()
            .filter(|t| t.is_ready() && t.asset.is_some())
            .map(|t| t.id)
            .collect();
        for id in ids {
            let Some(track) = self.session.track(id) else {
                continue;
            };
            let Some(asset) = track.asset.clone() else {
                continue;
            };
            self.start_decode(id, track.kind, asset.path);
        }
    }

    /// Open the browser, rescanning the standard practice locations.
    pub(super) fn open_browser(&mut self) {
        self.files = scan();
        self.browser_cursor = 0;
        self.field = 0;
        self.message = None;
        self.browser_open = true;
    }

    /// Start a background decode of `path`, targeting `id`.
    fn start_decode(&mut self, id: TrackId, kind: TrackKind, path: PathBuf) {
        let sr = self.sample_rate;
        let (tx, rx) = std::sync::mpsc::channel();
        let decode_path = path.clone();
        std::thread::spawn(move || {
            let result = decode_track(&decode_path, sr).map_err(|e| e.to_string());
            let _ = tx.send(result);
        });
        let generation = self.generation();
        self.decodes.push(PendingDecode {
            id,
            generation,
            kind,
            path,
            rx,
        });
    }

    /// Create a new import row at the current playhead and start decoding.
    fn import_at_playhead(&mut self, path: PathBuf, practice: &Practice) {
        let id = self.session.alloc_id();
        let start_ticks = self
            .session
            .frames_to_ticks(practice.position(), self.sample_rate);
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("import")
            .to_owned();
        self.session.push(
            id,
            name,
            TrackKind::Import,
            None,
            start_ticks,
            0,
            TrackLifecycle::Loading,
        );
        self.selection = Selection::Track(id);
        self.start_decode(id, TrackKind::Import, path);
        self.message = Some("Loading…".to_owned());
    }

    /// Poll background decodes and capture results once per UI tick. Returns
    /// `true` if the engine was touched.
    pub(super) fn poll(
        &mut self,
        engine: &mut AudioEngine,
        practice: &Practice,
        capture: &CaptureState,
    ) -> bool {
        let mut touched = false;
        touched |= self.poll_decodes(engine);
        touched |= self.poll_capture(engine);
        touched |= self.poll_acks(engine);
        // Auto-stopped at a loop out-point: finalize on the UI side.
        if self.recording_id.is_some()
            && capture.auto_stop.load(Relaxed)
            && capture.active.load(Relaxed)
        {
            self.finalize_capture(capture);
            let _ = practice;
        }
        touched
    }

    fn poll_decodes(&mut self, engine: &mut AudioEngine) -> bool {
        let mut touched = false;
        // Move the queue out so `self` methods (which touch `self.decodes`) can
        // be called while iterating.
        let mut queue = std::mem::take(&mut self.decodes);
        let mut keep = Vec::with_capacity(queue.len());
        for pending in queue.drain(..) {
            match pending.rx.try_recv() {
                Ok(Ok(decoded)) => {
                    self.finish_decode(engine, &pending, decoded);
                    touched = true;
                }
                Ok(Err(e)) => {
                    self.set_error(pending.id, e);
                    touched = true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => keep.push(pending),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.set_error(pending.id, "decode worker disconnected".to_owned());
                    touched = true;
                }
            }
        }
        self.decodes = keep;
        touched
    }

    fn finish_decode(
        &mut self,
        engine: &mut AudioEngine,
        pending: &PendingDecode,
        decoded: DecodedTrack,
    ) {
        let frames = decoded.track.frames();
        let Some((start_ticks, gain, muted, name)) = self
            .session
            .track(pending.id)
            .map(|t| (t.start_ticks, t.gain, t.muted, t.name.clone()))
        else {
            return;
        };
        let start_frames = self.session.ticks_to_frames(start_ticks, self.sample_rate);
        let length_ticks = self.session.frames_to_ticks(frames, self.sample_rate);
        let mut player_track = decoded.track;
        player_track.start = start_frames;
        let pk = peaks(&player_track, PEAK_BUCKETS);

        let asset = AssetRef {
            path: pending.path.clone(),
            source_sample_rate: decoded.source_sample_rate,
            source_channels: decoded.source_channels,
        };
        if let Some(track) = self.session.track_mut(pending.id) {
            track.asset = Some(asset);
            track.length_ticks = length_ticks;
            track.peaks = pk;
            track.generation = pending.generation;
            track.lifecycle = TrackLifecycle::Ready;
        }

        let kind = to_player_kind(pending.kind);
        if let Err(e) = engine.install_track(
            pending.id,
            pending.generation,
            kind,
            player_track,
            gain,
            muted,
        ) {
            self.set_error(pending.id, e.to_string());
            return;
        }
        self.message = Some(format!("Loaded {name}"));
    }

    fn poll_acks(&mut self, engine: &mut AudioEngine) -> bool {
        let acks = engine.poll_track_acks();
        let mut touched = false;
        for ack in acks {
            if !ack.installed
                && let Some(err) = ack.error
            {
                self.set_error(ack.id, err);
                touched = true;
            }
        }
        touched
    }

    fn poll_capture(&mut self, engine: &mut AudioEngine) -> bool {
        let Some(rx) = &self.capture_result else {
            return false;
        };
        let Ok(result) = rx.try_recv() else {
            return false;
        };
        self.capture_result = None;
        let current = self.recording_id;
        if current != Some(result.generation) {
            return false;
        }
        self.recording_id = None;
        let mut touched = false;
        if let Some(err) = result.error.clone() {
            self.set_error(result.generation, err);
            return touched;
        }
        let Some(player_track) = result.track else {
            self.session.remove(result.generation);
            self.selection = Selection::Transport;
            self.message = Some("Empty take discarded".to_owned());
            return touched;
        };
        let frames = player_track.frames();
        let start_ticks = self
            .session
            .frames_to_ticks(result.start_frame as usize, self.sample_rate);
        let length_ticks = self.session.frames_to_ticks(frames, self.sample_rate);
        let generation = self.generation();
        let (gain, muted) = self
            .session
            .track(result.generation)
            .map_or((1.0, false), |t| (t.gain, t.muted));
        if let Some(track) = self.session.track_mut(result.generation) {
            track.asset = Some(AssetRef {
                path: result.path,
                source_sample_rate: self.sample_rate as u32,
                source_channels: 1,
            });
            track.start_ticks = start_ticks;
            track.length_ticks = length_ticks;
            track.peaks = result.peaks.clone();
            track.generation = generation;
            track.lifecycle = TrackLifecycle::Ready;
        }
        if let Err(e) = engine.install_track(
            result.generation,
            generation,
            PlayerKind::RawTake,
            player_track,
            gain,
            muted,
        ) {
            self.set_error(result.generation, e.to_string());
            return touched;
        }
        touched = true;
        let note = if result.overflowed {
            " (incomplete: capture overflowed)"
        } else {
            ""
        };
        self.message = Some(format!("Take ready{note}"));
        touched
    }

    fn set_error(&mut self, id: TrackId, msg: String) {
        if let Some(track) = self.session.track_mut(id) {
            track.lifecycle = TrackLifecycle::Error(msg.clone());
        }
        self.message = Some(msg);
    }

    // ── Focused-control edits (called from the key handler) ─────────────────────

    pub(super) fn toggle_play(&self, practice: &Practice) {
        let now = !practice.playing.load(Relaxed);
        practice.playing.store(now, Relaxed);
    }

    pub(super) fn toggle_loop(&self, practice: &Practice) {
        let now = !practice.loop_enabled.load(Relaxed);
        practice.loop_enabled.store(now, Relaxed);
    }

    /// Move the selection between the transport and the track rows.
    pub(super) fn move_selection(&mut self, forward: bool) {
        match self.selection {
            Selection::Transport => {
                if forward && let Some(id) = self.session.tracks().first().map(|t| t.id) {
                    self.selection = Selection::Track(id);
                    self.session.select(id);
                }
            }
            Selection::Track(id) => {
                let idx = self.session.index_of(id);
                let last = self.session.len().saturating_sub(1);
                match idx {
                    Some(i) if forward && i >= last => {
                        self.selection = Selection::Transport;
                    }
                    Some(0) if !forward => {
                        self.selection = Selection::Transport;
                    }
                    Some(_) => {
                        self.session.select_next(forward);
                        if let Some(next) = self.session.selected() {
                            self.selection = Selection::Track(next);
                        }
                    }
                    None => self.selection = Selection::Transport,
                }
            }
        }
    }

    pub(super) fn seek_by(&self, practice: &Practice, direction: i32) {
        let step = f64::from(self.session.seek_seconds());
        let delta = (step * f64::from(self.sample_rate)) as i64;
        let total =
            self.session
                .ticks_to_frames(self.session.extent_ticks(), self.sample_rate) as i64;
        let cur = practice.position() as i64;
        let next = (cur + delta * i64::from(direction)).clamp(0, total.max(0));
        practice.request_seek(next as usize);
    }

    pub(super) fn cycle_seek_step(&mut self, direction: i32) {
        let step = self.session.cycle_seek_step(direction);
        self.message = Some(format!("Seek step: {step}s"));
    }

    /// Set the loop in-point at the playhead, opening a one-second region if the
    /// current out-point is not ahead of it.
    pub(super) fn set_loop_start(&self, practice: &Practice) {
        let pos = practice.position() as u64;
        practice.loop_start.store(pos, Relaxed);
        if practice.loop_end.load(Relaxed) <= pos {
            practice
                .loop_end
                .store(pos + self.sample_rate as u64, Relaxed);
        }
        practice.loop_enabled.store(true, Relaxed);
    }

    /// Set the loop out-point at the playhead, pulling the in-point back if needed.
    pub(super) fn set_loop_end(&self, practice: &Practice) {
        let pos = practice.position() as u64;
        practice.loop_end.store(pos, Relaxed);
        if practice.loop_start.load(Relaxed) >= pos {
            practice.loop_start.store(0, Relaxed);
        }
        practice.loop_enabled.store(true, Relaxed);
    }

    pub(super) fn toggle_selected_mute(&mut self, engine: &mut AudioEngine, practice: &Practice) {
        let _ = practice;
        let Some(id) = self.selected_track() else {
            return;
        };
        let Some(track) = self.session.track_mut(id) else {
            return;
        };
        track.muted = !track.muted;
        let muted = track.muted;
        let _ = engine.set_track_mute(id, muted);
    }

    pub(super) fn delete_selected(
        &mut self,
        engine: &mut AudioEngine,
        practice: &Practice,
        capture: &CaptureState,
    ) {
        let _ = practice;
        let Some(id) = self.selected_track() else {
            return;
        };
        // Removing the row that is currently recording also stops the writer.
        if self.recording_id == Some(id) {
            self.abort_capture(capture);
        }
        let _ = engine.remove_track(id);
        self.session.remove(id);
        self.selection = Selection::Transport;
    }

    fn selected_track(&self) -> Option<TrackId> {
        match self.selection {
            Selection::Transport => None,
            Selection::Track(id) => Some(id),
        }
    }

    /// True while the transport row (not a track) owns the cursor.
    pub(super) fn on_transport(&self) -> bool {
        matches!(self.selection, Selection::Transport)
    }

    // ── Capture lifecycle ───────────────────────────────────────────────────────

    /// Handle `R`: arm a fresh raw-take row, or stop the active take.
    pub(super) fn arm_or_stop(
        &mut self,
        engine: &mut AudioEngine,
        practice: &Practice,
        capture: &CaptureState,
    ) {
        let _ = engine;
        if self.recording_id.is_some() {
            // Stop: silence the callback, ask the writer to finalize.
            self.finalize_capture(capture);
            return;
        }
        self.arm(capture, practice);
    }

    fn arm(&mut self, capture: &CaptureState, practice: &Practice) {
        let ready = self
            .session
            .tracks()
            .iter()
            .filter(|t| t.is_ready())
            .count();
        if ready >= MAX_TRACKS {
            self.message = Some(format!("Timeline is full ({MAX_TRACKS} tracks)"));
            return;
        }
        let Some(cmd) = &self.capture_cmd else {
            self.message = Some("Capture writer unavailable".to_owned());
            return;
        };
        let id = self.session.alloc_id();
        let start_ticks = self
            .session
            .frames_to_ticks(practice.position(), self.sample_rate);
        let count = self
            .session
            .tracks()
            .iter()
            .filter(|t| t.kind == TrackKind::RawTake)
            .count()
            + 1;
        self.session.push(
            id,
            format!("Take {count}"),
            TrackKind::RawTake,
            None,
            start_ticks,
            0,
            TrackLifecycle::Recording,
        );
        self.selection = Selection::Track(id);

        let path = self
            .session
            .recovery_dir()
            .map(|d| d.join(format!("take-{id}.wav")))
            .unwrap_or_else(|| PathBuf::from(format!("take-{id}.wav")));
        let (tx, rx) = std::sync::mpsc::channel();
        if cmd
            .send(CaptureCommand::Begin {
                generation: id,
                path,
                sample_rate: self.sample_rate as u32,
                result: tx,
            })
            .is_err()
        {
            self.message = Some("Capture writer unavailable".to_owned());
            self.session.remove(id);
            self.selection = Selection::Transport;
            return;
        }
        self.capture_result = Some(rx);
        self.recording_id = Some(id);
        capture.arm(id);
        // Start transport if paused so the take follows the playhead.
        practice.playing.store(true, Relaxed);
        self.message = Some("Recording…".to_owned());
    }

    fn finalize_capture(&mut self, capture: &CaptureState) {
        capture.disarm();
        if let Some(tx) = &self.capture_cmd {
            let _ = tx.send(CaptureCommand::End);
        }
        self.message = Some("Finalizing take…".to_owned());
    }

    /// Abort an in-flight take (device change / quit).
    pub(super) fn abort_capture(&mut self, capture: &CaptureState) {
        capture.disarm();
        if let Some(tx) = &self.capture_cmd {
            let _ = tx.send(CaptureCommand::Abort);
        }
        if let Some(id) = self.recording_id.take() {
            self.session.remove(id);
        }
        self.capture_result = None;
        self.selection = Selection::Transport;
    }

    pub(super) fn is_recording(&self) -> bool {
        self.recording_id.is_some()
    }

    // ── Browser / gain modal input ──────────────────────────────────────────────

    pub(super) fn handle_browser_key(&mut self, code: KeyCode, practice: &Practice) -> bool {
        match code {
            KeyCode::Up if self.field == 0 => {
                self.browser_cursor = self.browser_cursor.saturating_sub(1);
            }
            KeyCode::Down if self.field == 0 => {
                self.browser_cursor =
                    (self.browser_cursor + 1).min(self.files.len().saturating_sub(1));
            }
            KeyCode::Tab => self.field = 1 - self.field,
            KeyCode::Enter => {
                if self.field == 1 {
                    let p = self.path_input.trim().to_owned();
                    if p.is_empty() {
                        self.message = Some("Type a file path first".to_owned());
                    } else {
                        self.import_at_playhead(PathBuf::from(p), practice);
                        self.browser_open = false;
                    }
                } else if let Some(file) = self.files.get(self.browser_cursor) {
                    let path = file.path.clone();
                    self.import_at_playhead(path, practice);
                    self.browser_open = false;
                }
            }
            KeyCode::Backspace if self.field == 1 => {
                self.path_input.pop();
            }
            KeyCode::Char(c) if self.field == 1 => {
                self.path_input.push(c);
            }
            KeyCode::Esc | KeyCode::Char('b') | KeyCode::Char('B') => {
                self.browser_open = false;
            }
            _ => {}
        }
        false
    }

    pub(super) fn open_gain_edit(&mut self) {
        if let Some(id) = self.selected_track() {
            self.gain_edit = Some(id);
        }
    }

    pub(super) fn handle_gain_key(&mut self, code: KeyCode, engine: &mut AudioEngine) {
        let Some(id) = self.gain_edit else {
            return;
        };
        match code {
            KeyCode::Left | KeyCode::Down | KeyCode::Char('-') => {
                self.nudge_gain(engine, id, -0.05)
            }
            KeyCode::Right | KeyCode::Up | KeyCode::Char('+') | KeyCode::Char('=') => {
                self.nudge_gain(engine, id, 0.05);
            }
            KeyCode::Char('r') | KeyCode::Char('R') => self.set_gain(engine, id, 1.0),
            KeyCode::Enter | KeyCode::Esc => self.gain_edit = None,
            _ => {}
        }
    }

    fn nudge_gain(&mut self, engine: &mut AudioEngine, id: TrackId, delta: f32) {
        let current = self.session.track(id).map_or(1.0, |t| t.gain);
        self.set_gain(engine, id, (current + delta).clamp(0.0, 2.0));
    }

    fn set_gain(&mut self, engine: &mut AudioEngine, id: TrackId, value: f32) {
        if let Some(track) = self.session.track_mut(id) {
            track.gain = value;
        }
        let _ = engine.set_track_gain(id, value);
    }

    pub(super) fn gain_open(&self) -> bool {
        self.gain_edit.is_some()
    }

    // ── Rendering ───────────────────────────────────────────────────────────────

    /// Render the timeline pane. `focused` is true while the pane owns focus;
    /// `recording` drives the transport REC lamp.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render(
        &self,
        f: &mut Frame,
        area: Rect,
        practice: &Practice,
        focused: bool,
        blink: bool,
        recording: bool,
        takes_builtin: bool,
    ) {
        if area.height < 2 || area.width < 4 {
            return;
        }
        let focused_glyph = if focused { ACCENT } else { shade(ACCENT, 0.55) };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(border_style(focused))
            .title(Line::from(Span::styled(
                " T I M E L I N E ",
                Style::default()
                    .fg(focused_glyph)
                    .add_modifier(Modifier::BOLD),
            )))
            .style(Style::default().bg(Color::Black));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // transport
                Constraint::Min(1),    // track list
                Constraint::Length(1), // hint/message
            ])
            .split(inner);

        self.render_transport(f, rows[0], practice, focused, blink, recording);
        self.render_tracks(f, rows[1], practice, focused);
        self.render_hint(f, rows[2], focused, takes_builtin);
    }

    fn render_transport(
        &self,
        f: &mut Frame,
        area: Rect,
        practice: &Practice,
        focused: bool,
        blink: bool,
        recording: bool,
    ) {
        let playing = practice.playing.load(Relaxed);
        let total = self
            .session
            .ticks_to_frames(self.session.extent_ticks(), self.sample_rate);
        let position = practice.position().min(total.max(1));
        let state = if playing {
            Span::styled(
                " ▶ ",
                Style::default().fg(SAFE).add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(" ⏸ ", Style::default().fg(DIM))
        };
        let cursor = if focused && self.selection == Selection::Transport && blink {
            "▌"
        } else {
            " "
        };
        let loop_on = practice.loop_enabled.load(Relaxed);
        let mut transport = vec![
            Span::styled(cursor.to_owned(), Style::default().fg(ACCENT)),
            state,
            if recording && blink {
                Span::styled(
                    "●REC ",
                    Style::default().fg(HOT).add_modifier(Modifier::BOLD),
                )
            } else if recording {
                Span::styled("○REC ", Style::default().fg(HOT))
            } else {
                Span::raw("     ")
            },
            Span::styled(
                format!(
                    "{} / {}  ",
                    mmss(position, self.sample_rate),
                    mmss(total, self.sample_rate)
                ),
                Style::default().fg(CHROME),
            ),
            Span::styled(
                format!("{}s ", self.session.seek_seconds()),
                Style::default().fg(AMBER),
            ),
            Span::styled(
                if loop_on { "LOOP " } else { "loop " },
                Style::default()
                    .fg(if loop_on { HOT } else { DIM })
                    .add_modifier(if loop_on {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
        ];
        if loop_on {
            let a = practice.loop_start.load(Relaxed) as usize;
            let b = practice.loop_end.load(Relaxed) as usize;
            transport.push(Span::styled(
                format!(
                    "[{} ▸ {}]  ",
                    mmss(a, self.sample_rate),
                    mmss(b, self.sample_rate)
                ),
                Style::default().fg(HOT),
            ));
        }
        if let Some(msg) = &self.message {
            transport.push(Span::styled(msg.clone(), Style::default().fg(WARN)));
        }
        f.render_widget(Paragraph::new(Line::from(transport)), area);
    }

    fn render_tracks(&self, f: &mut Frame, area: Rect, practice: &Practice, focused: bool) {
        let tracks = self.session.tracks();
        if tracks.is_empty() {
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("no tracks — press ", Style::default().fg(DIM)),
                    Span::styled("B", Style::default().fg(AMBER)),
                    Span::styled(" to import, ", Style::default().fg(DIM)),
                    Span::styled("R", Style::default().fg(AMBER)),
                    Span::styled(" to record a raw take", Style::default().fg(DIM)),
                ])),
                area,
            );
            return;
        }
        let visible = area.height as usize;
        let selected_idx = match self.selection {
            Selection::Transport => None,
            Selection::Track(id) => self.session.index_of(id),
        };
        let offset = match selected_idx {
            Some(idx) => idx.saturating_sub(visible.saturating_sub(1)),
            None => 0,
        };
        let total = self
            .session
            .ticks_to_frames(self.session.extent_ticks(), self.sample_rate)
            .max(1);
        let position = practice.position().min(total);
        let width = area.width as usize;

        let mut lines: Vec<Line> = Vec::with_capacity(visible);
        for (i, track) in tracks.iter().enumerate().skip(offset).take(visible) {
            let focused_row = focused && selected_idx == Some(i);
            lines.push(self.track_line(track, focused_row, practice, position, total, width));
        }
        f.render_widget(Paragraph::new(lines), area);
    }

    #[allow(clippy::too_many_arguments)]
    fn track_line<'a>(
        &self,
        track: &crate::session::Track,
        focused: bool,
        practice: &Practice,
        position: usize,
        total: usize,
        width: usize,
    ) -> Line<'a> {
        let start = self
            .session
            .ticks_to_frames(track.start_ticks, self.sample_rate);
        let frames = self
            .session
            .ticks_to_frames(track.length_ticks, self.sample_rate);
        let loaded = track.is_ready() && frames > 0;
        let (tag, tag_color) = match track.kind {
            TrackKind::Import => ("IMP ", CHROME),
            TrackKind::RawTake => ("TAKE", CHROME),
        };
        let led = if track.muted {
            Span::styled("○ ", Style::default().fg(DIM))
        } else if loaded {
            Span::styled("● ", Style::default().fg(SAFE))
        } else if matches!(track.lifecycle, TrackLifecycle::Recording) {
            Span::styled("◉ ", Style::default().fg(HOT))
        } else {
            Span::styled("· ", Style::default().fg(DIM))
        };
        let name_w = 14usize;
        let gain = format!("{:>3}%", (track.gain * 100.0).round() as i32);
        let mut spans = vec![
            Span::styled(
                if focused { "▌" } else { " " }.to_owned(),
                Style::default().fg(ACCENT),
            ),
            led,
            Span::styled(tag.to_owned(), Style::default().fg(tag_color)),
            Span::raw(" "),
            Span::styled(
                format!("{:<name_w$}", truncate(&track.name, name_w)),
                Style::default().fg(if loaded { CHROME } else { DIM }),
            ),
            Span::styled(gain, Style::default().fg(AMBER)),
            Span::raw(" "),
        ];
        let used = 1 + 2 + 4 + 1 + name_w + 4 + 1;
        let wave_w = width.saturating_sub(used);
        if !loaded || wave_w == 0 || total == 0 || track.peaks.is_empty() {
            if let TrackLifecycle::Error(e) = &track.lifecycle {
                spans.push(Span::styled(
                    format!(" {}", truncate(e, wave_w.max(4))),
                    Style::default().fg(WARN),
                ));
            }
            return Line::from(spans);
        }

        let loop_on = practice.loop_enabled.load(Relaxed);
        let (la, lb) = (
            practice.loop_start.load(Relaxed) as usize,
            practice.loop_end.load(Relaxed) as usize,
        );
        let pos_col = position * wave_w.saturating_sub(1) / total;
        let bars = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        for col in 0..wave_w {
            let frame = col * total / wave_w.max(1);
            if col == pos_col {
                spans.push(Span::styled(
                    "│",
                    Style::default().fg(HOT).add_modifier(Modifier::BOLD),
                ));
                continue;
            }
            let in_loop = loop_on && lb > la && frame >= la && frame < lb;
            let inside = frame >= start && frame - start < frames;
            let ch = if !inside {
                if in_loop { '·' } else { ' ' }
            } else {
                let rel = frame - start;
                let bucket = rel * track.peaks.len() / frames.max(1);
                let amp = track
                    .peaks
                    .get(bucket.min(track.peaks.len() - 1))
                    .map(|(lo, hi)| (hi - lo).max(0.0))
                    .unwrap_or(0.0);
                bars[(amp.sqrt() * 8.0).round() as usize % bars.len()]
            };
            spans.push(Span::styled(
                ch.to_string(),
                Style::default().fg(if in_loop { HOT } else { CHROME }),
            ));
        }
        Line::from(spans)
    }

    fn render_hint(&self, f: &mut Frame, area: Rect, focused: bool, takes_builtin: bool) {
        if takes_builtin {
            let note = Line::from(vec![
                Span::styled("⚠ ", Style::default().fg(WARN)),
                Span::styled(
                    "takes monitor through the built-in amp/cab (external rig is not mirrored yet)",
                    Style::default().fg(WARN),
                ),
            ]);
            f.render_widget(Paragraph::new(note).alignment(Alignment::Left), area);
            return;
        }
        let hint = if focused {
            Line::from(vec![
                Span::styled("Space", Style::default().fg(AMBER)),
                Span::styled(" play/mute  ", Style::default().fg(DIM)),
                Span::styled("←/→", Style::default().fg(AMBER)),
                Span::styled(" seek  ", Style::default().fg(DIM)),
                Span::styled("+/-", Style::default().fg(AMBER)),
                Span::styled(" step  ", Style::default().fg(DIM)),
                Span::styled("G", Style::default().fg(AMBER)),
                Span::styled(" gain  ", Style::default().fg(DIM)),
                Span::styled("[ ] L", Style::default().fg(AMBER)),
                Span::styled(" loop  ", Style::default().fg(DIM)),
                Span::styled("Del", Style::default().fg(AMBER)),
                Span::styled(" remove  ", Style::default().fg(DIM)),
                Span::styled("R", Style::default().fg(AMBER)),
                Span::styled(" rec  ", Style::default().fg(DIM)),
                Span::styled("B", Style::default().fg(AMBER)),
                Span::styled(" import", Style::default().fg(DIM)),
            ])
        } else {
            Line::from(vec![
                Span::styled("3", Style::default().fg(AMBER)),
                Span::styled(" focus the timeline  ·  ", Style::default().fg(DIM)),
                Span::styled("B", Style::default().fg(AMBER)),
                Span::styled(" import  ·  ", Style::default().fg(DIM)),
                Span::styled("R", Style::default().fg(AMBER)),
                Span::styled(" record", Style::default().fg(DIM)),
            ])
        };
        f.render_widget(Paragraph::new(hint).alignment(Alignment::Left), area);
    }

    /// Render the import browser modal.
    pub(super) fn render_browser(&self, f: &mut Frame) {
        let area = centered_rect(62, f.area());
        f.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(ACCENT))
            .title(Span::styled(
                " I M P O R T   T R A C K ",
                Style::default().fg(AMBER).add_modifier(Modifier::BOLD),
            ))
            .style(Style::default().bg(Color::Black));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // intro
                Constraint::Length(1), // path field
                Constraint::Min(1),    // file list
                Constraint::Length(1), // hint
                Constraint::Length(1), // message
            ])
            .split(inner);

        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Enter", Style::default().fg(AMBER)),
                Span::styled(
                    " adds the file as a new track at the playhead",
                    Style::default().fg(DIM),
                ),
            ])),
            rows[0],
        );

        let path_focus = if self.field == 1 { "▌" } else { " " };
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("path ", Style::default().fg(DIM)),
                Span::styled(
                    if self.path_input.is_empty() {
                        "(type a file path…)".to_owned()
                    } else {
                        self.path_input.clone()
                    },
                    Style::default().fg(if self.path_input.is_empty() {
                        DIM
                    } else {
                        CHROME
                    }),
                ),
                Span::styled(path_focus.to_owned(), Style::default().fg(ACCENT)),
            ])),
            rows[1],
        );

        let mut lines: Vec<Line> = Vec::with_capacity(self.files.len());
        for (i, file) in self.files.iter().enumerate() {
            let selected = i == self.browser_cursor && self.field == 0;
            lines.push(file_entry(file, selected));
        }
        if self.files.is_empty() {
            lines.push(Line::from(Span::styled(
                "  (no audio files found — type a path above, or set RUSTY_AMP_PRACTICE_DIR)",
                Style::default().fg(DIM),
            )));
        }
        let visible = rows[2].height as usize;
        let offset = self
            .browser_cursor
            .saturating_sub(visible.saturating_sub(1));
        f.render_widget(
            Paragraph::new(lines.into_iter().skip(offset).collect::<Vec<_>>()),
            rows[2],
        );

        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("↑/↓", Style::default().fg(AMBER)),
                Span::styled(" files  ", Style::default().fg(DIM)),
                Span::styled("Tab", Style::default().fg(AMBER)),
                Span::styled(" path  ", Style::default().fg(DIM)),
                Span::styled("Esc / B", Style::default().fg(AMBER)),
                Span::styled(" close", Style::default().fg(DIM)),
            ]))
            .alignment(Alignment::Center),
            rows[3],
        );

        let msg = self.message.clone().unwrap_or_default();
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(msg, Style::default().fg(WARN))))
                .alignment(Alignment::Center),
            rows[4],
        );
    }

    /// Render the per-track gain modal.
    pub(super) fn render_gain_modal(&self, f: &mut Frame) {
        let Some(id) = self.gain_edit else {
            return;
        };
        let Some(track) = self.session.track(id) else {
            return;
        };
        let area = centered_box(46, 7, f.area());
        f.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(ACCENT))
            .title(Span::styled(
                " T R A C K   G A I N ",
                Style::default().fg(AMBER).add_modifier(Modifier::BOLD),
            ))
            .style(Style::default().bg(Color::Black));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let kind = match track.kind {
            TrackKind::Import => "monitor volume",
            TrackKind::RawTake => "pre-rig level (drives the amp)",
        };
        let pct = (track.gain * 100.0).round() as i32;
        let bar_w = inner.width.saturating_sub(2) as usize;
        let filled = ((track.gain / 2.0).clamp(0.0, 1.0) * bar_w as f32).round() as usize;
        let bar = format!(
            "{}{}",
            "█".repeat(filled),
            "·".repeat(bar_w.saturating_sub(filled))
        );
        let text = vec![
            Line::from(Span::styled(
                truncate(&track.name, inner.width as usize),
                Style::default().fg(CHROME),
            )),
            Line::from(Span::styled(kind.to_owned(), Style::default().fg(DIM))),
            Line::from(Span::styled(bar, Style::default().fg(ACCENT))),
            Line::from(vec![
                Span::styled(
                    format!("{pct}%  "),
                    Style::default().fg(AMBER).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "←/→ adjust · R reset · Enter/Esc close",
                    Style::default().fg(DIM),
                ),
            ]),
        ];
        f.render_widget(Paragraph::new(text), inner);
    }
}

fn to_player_kind(kind: TrackKind) -> PlayerKind {
    match kind {
        TrackKind::Import => PlayerKind::Import,
        TrackKind::RawTake => PlayerKind::RawTake,
    }
}

fn file_entry(file: &TrackFile, selected: bool) -> Line<'static> {
    let (prefix, style) = if selected {
        (
            "▶ ",
            Style::default()
                .fg(ACCENT)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED),
        )
    } else {
        ("  ", Style::default().fg(CHROME))
    };
    Line::from(vec![
        Span::styled(
            prefix.to_owned(),
            Style::default().fg(if selected { ACCENT } else { DIM }),
        ),
        Span::styled(file.label.clone(), style),
        Span::styled(format!("  {}", file.detail), Style::default().fg(DIM)),
    ])
}

/// `mm:ss` for a frame count at `sr`.
pub(super) fn mmss(frames: usize, sr: f32) -> String {
    let secs = if sr > 0.0 {
        (frames as f32 / sr) as u64
    } else {
        0
    };
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_owned()
    } else {
        s.chars().take(n.saturating_sub(1)).collect::<String>() + "…"
    }
}

/// Standard practice locations, scanned for audio files.
fn scan() -> Vec<TrackFile> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(dir) = std::env::var("RUSTY_AMP_PRACTICE_DIR") {
        roots.push(PathBuf::from(dir));
    }
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join("Music"));
        roots.push(home.join("Desktop"));
    }
    roots.push(PathBuf::from("."));

    let mut out: Vec<TrackFile> = Vec::new();
    for root in roots {
        collect_audio(&root, 3, &mut out);
    }
    out.sort_by(|a, b| a.label.cmp(&b.label).then(a.path.cmp(&b.path)));
    out.dedup_by(|a, b| a.path == b.path);
    out
}

fn collect_audio(dir: &Path, depth: usize, out: &mut Vec<TrackFile>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth > 0 {
                collect_audio(&path, depth - 1, out);
            }
        } else if is_audio(&path) {
            let label = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("track")
                .to_owned();
            let detail = path
                .parent()
                .and_then(|p| p.to_str())
                .unwrap_or_default()
                .to_owned();
            out.push(TrackFile {
                path,
                label,
                detail,
            });
        }
    }
}

fn is_audio(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        e.eq_ignore_ascii_case("mp3")
            || e.eq_ignore_ascii_case("wav")
            || e.eq_ignore_ascii_case("flac")
    })
}

fn centered_rect(percent_x: u16, area: Rect) -> Rect {
    let width = (area.width * percent_x / 100).max(30).min(area.width);
    let height = (area.height * 70 / 100).max(10);
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    Rect {
        x: area.x + x,
        y: area.y + y,
        width,
        height,
    }
}

fn centered_box(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    Rect {
        x: area.x + x,
        y: area.y + y,
        width,
        height,
    }
}

fn shade(c: Color, factor: f32) -> Color {
    match c {
        Color::Rgb(r, g, b) => Color::Rgb(
            (r as f32 * factor) as u8,
            (g as f32 * factor) as u8,
            (b as f32 * factor) as u8,
        ),
        other => other,
    }
}

fn border_style(active: bool) -> Style {
    Style::default().fg(if active { ACCENT } else { shade(ACCENT, 0.5) })
}

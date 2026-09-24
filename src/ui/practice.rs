//! Practice / jam-along timeline pane and backing-track browser.
//!
//! The pane is a fixed 10-row strip under the amp panel: a transport line
//! and three rows per track (backing + recorded take) with a tall waveform,
//! the playhead and a shaded loop region. The browser modal loads a backing file
//! (MP3 / WAV / FLAC) via [`crate::practice::decode_track`], decoding on a worker
//! thread so the 30 ms UI loop never stalls.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

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
use crate::dsp::player::PlayerTrack;
use crate::practice::{Practice, decode_track};

/// How many waveform buckets we keep per track for the mini display (mapped to the
/// pane width each frame). Enough detail for a glance without recomputing peaks.
pub(super) const PEAK_BUCKETS: usize = 512;

/// One discovered audio file in the browser.
struct TrackFile {
    path: PathBuf,
    label: String,
    detail: String,
}

/// A finished background decode: the ready-to-install track and its display peaks.
struct Decoded {
    track: PlayerTrack,
    peaks: Vec<(f32, f32)>,
    name: String,
}

/// Which install slot a browser selection targets.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Slot {
    Backing,
    Take,
}

/// All practice UI state, owned by the UI thread.
pub(super) struct PracticeUi {
    pub(super) browser_open: bool,
    browser_cursor: usize,
    files: Vec<TrackFile>,
    /// Slot the browser loads into (toggled with Tab).
    slot: Slot,
    /// Typed path alternative to browsing.
    path_input: String,
    /// 0 = file list, 1 = path field.
    field: usize,
    message: Option<String>,
    sample_rate: f32,
    pending: Option<Receiver<Result<Decoded, String>>>,
    loading: bool,

    pub backing_name: Option<String>,
    backing_peaks: Vec<(f32, f32)>,
    backing_frames: usize,
    pub record_name: Option<String>,
    record_peaks: Vec<(f32, f32)>,
    record_frames: usize,

    /// Focused control inside the pane: 0 = transport, 1 = backing, 2 = take.
    pub selected: usize,
    /// Backing playhead captured when recording started (take alignment).
    pub record_offset: usize,
}

impl PracticeUi {
    pub(super) fn new(sample_rate: f32) -> Self {
        Self {
            browser_open: false,
            browser_cursor: 0,
            files: Vec::new(),
            slot: Slot::Backing,
            path_input: String::new(),
            field: 0,
            message: None,
            sample_rate,
            pending: None,
            loading: false,
            backing_name: None,
            backing_peaks: Vec::new(),
            backing_frames: 0,
            record_name: None,
            record_peaks: Vec::new(),
            record_frames: 0,
            selected: 0,
            record_offset: 0,
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

    /// Start a background decode of `path`, targeting `slot`.
    fn start_decode(&mut self, path: PathBuf, slot: Slot) {
        if self.pending.is_some() {
            self.message = Some("A track is already loading…".to_owned());
            return;
        }
        let sr = self.sample_rate;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = decode_track(&path, sr)
                .map(|track| {
                    let peaks = peaks(&track, PEAK_BUCKETS);
                    let name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("track")
                        .to_owned();
                    Decoded { track, peaks, name }
                })
                .map_err(|e| e.to_string());
            let _ = tx.send(result);
        });
        self.pending = Some(rx);
        self.loading = true;
        self.slot = slot;
        self.message = Some("Loading…".to_owned());
    }

    /// Install a background decode result if one has arrived. Called once per UI
    /// loop tick. Returns `true` if the engine was touched (so the caller may want
    /// to refresh anything derived from it).
    pub(super) fn poll_decode(&mut self, engine: &mut AudioEngine, practice: &Practice) -> bool {
        let Some(rx) = &self.pending else {
            return false;
        };
        let Ok(result) = rx.try_recv() else {
            return false;
        };
        self.pending = None;
        self.loading = false;
        match result {
            Ok(decoded) => {
                let frames = decoded.track.frames();
                let install = match self.slot {
                    Slot::Backing => engine.set_backing_track(Some(decoded.track)),
                    Slot::Take => engine.set_record_track(Some(decoded.track)),
                };
                if let Err(e) = install {
                    self.message = Some(format!("Load failed: {e}"));
                    return false;
                }
                match self.slot {
                    Slot::Backing => {
                        practice.backing_loaded.store(true, Relaxed);
                        practice.backing_len.store(frames as u64, Relaxed);
                        self.backing_name = Some(decoded.name.clone());
                        self.backing_peaks = decoded.peaks;
                        self.backing_frames = frames;
                    }
                    Slot::Take => {
                        // A take loaded from the browser starts at the top of the
                        // timeline; only a live take carries a captured offset.
                        self.record_offset = 0;
                        practice.record_loaded.store(true, Relaxed);
                        practice.record_len.store(frames as u64, Relaxed);
                        self.record_name = Some(decoded.name.clone());
                        self.record_peaks = decoded.peaks;
                        self.record_frames = frames;
                    }
                }
                self.message = Some(format!("Loaded {}", decoded.name));
                true
            }
            Err(e) => {
                self.message = Some(format!("Load failed: {e}"));
                false
            }
        }
    }

    /// Clear the backing track (UI + engine + shared state).
    pub(super) fn clear_backing(&mut self, engine: &mut AudioEngine, practice: &Practice) {
        let _ = engine.set_backing_track(None);
        practice.backing_loaded.store(false, Relaxed);
        practice.backing_len.store(0, Relaxed);
        self.backing_name = None;
        self.backing_peaks.clear();
        self.backing_frames = 0;
    }

    /// Clear the take track (UI + engine + shared state).
    pub(super) fn clear_take(&mut self, engine: &mut AudioEngine, practice: &Practice) {
        let _ = engine.set_record_track(None);
        practice.record_loaded.store(false, Relaxed);
        practice.record_len.store(0, Relaxed);
        self.record_name = None;
        self.record_peaks.clear();
        self.record_frames = 0;
    }

    /// Place a just-recorded take on the timeline: build the stereo track at the
    /// captured backing offset, compute its display peaks and install it. The
    /// samples are interleaved stereo `(L, R)` at the engine rate.
    pub(super) fn place_take(
        &mut self,
        engine: &mut AudioEngine,
        practice: &Practice,
        samples: &[f32],
        offset: usize,
    ) {
        let frames = samples.len() / 2;
        let mut l = Vec::with_capacity(frames);
        let mut r = Vec::with_capacity(frames);
        for f in 0..frames {
            l.push(samples.get(2 * f).copied().unwrap_or(0.0));
            r.push(samples.get(2 * f + 1).copied().unwrap_or(0.0));
        }
        let track = PlayerTrack {
            l,
            r,
            start: offset,
        };
        let peaks = peaks(&track, PEAK_BUCKETS);
        let _ = engine.set_record_track(Some(track));
        practice.record_loaded.store(true, Relaxed);
        practice.record_len.store((offset + frames) as u64, Relaxed);
        self.record_name = Some(format!("take {}", mmss(offset, self.sample_rate)));
        self.record_peaks = peaks;
        self.record_frames = frames;
        self.record_offset = offset;
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

    pub(super) fn seek_by(&self, practice: &Practice, secs: f32) {
        let total = practice.timeline_len() as i64;
        let cur = practice.position() as i64;
        let delta = (secs * self.sample_rate) as i64;
        practice.request_seek((cur + delta).clamp(0, total) as usize);
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

    pub(super) fn toggle_selected_mute(&self, practice: &Practice) {
        let flag = match self.selected {
            1 => &practice.backing_muted,
            2 => &practice.record_muted,
            _ => return,
        };
        let now = !flag.load(Relaxed);
        flag.store(now, Relaxed);
    }

    pub(super) fn delete_selected(&mut self, engine: &mut AudioEngine, practice: &Practice) {
        match self.selected {
            1 => self.clear_backing(engine, practice),
            2 => self.clear_take(engine, practice),
            _ => {}
        }
    }

    /// Handle a key while the browser modal is open. Enter on the path field loads
    /// the typed path; Enter on the list loads the highlighted file.
    pub(super) fn handle_browser_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Tab => {
                self.field = 1 - self.field;
                self.slot = if self.slot == Slot::Backing {
                    Slot::Take
                } else {
                    Slot::Backing
                };
            }
            KeyCode::Up if self.field == 0 => {
                self.browser_cursor = self.browser_cursor.saturating_sub(1);
            }
            KeyCode::Down if self.field == 0 => {
                self.browser_cursor =
                    (self.browser_cursor + 1).min(self.files.len().saturating_sub(1));
            }
            KeyCode::Enter => {
                if self.field == 1 {
                    let p = self.path_input.trim().to_owned();
                    if p.is_empty() {
                        self.message = Some("Type a file path first".to_owned());
                    } else {
                        self.start_decode(PathBuf::from(p), self.slot);
                    }
                } else if let Some(file) = self.files.get(self.browser_cursor) {
                    let path = file.path.clone();
                    self.start_decode(path, self.slot);
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
    }

    /// Render the timeline pane. `focused` is true while the pane owns focus.
    pub(super) fn render(
        &self,
        f: &mut Frame,
        area: Rect,
        practice: &Practice,
        focused: bool,
        blink: bool,
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
                " P R A C T I C E ",
                Style::default()
                    .fg(focused_glyph)
                    .add_modifier(Modifier::BOLD),
            )))
            .style(Style::default().bg(Color::Black));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let playing = practice.playing.load(Relaxed);
        let total = practice.timeline_len();
        let position = practice.position().min(total.max(1));
        let has_any = self.backing_name.is_some() || self.record_name.is_some();

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // transport
                Constraint::Length(3), // backing (3-row waveform)
                Constraint::Length(3), // take (3-row waveform)
                Constraint::Min(1),    // hint/message (absorbs leftover)
            ])
            .split(inner);

        if !has_any {
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("  no track — press ", Style::default().fg(DIM)),
                    Span::styled("B", Style::default().fg(AMBER)),
                    Span::styled(" to load a backing track or take", Style::default().fg(DIM)),
                    if self.loading {
                        Span::styled("   (loading…)", Style::default().fg(HOT))
                    } else {
                        Span::raw("")
                    },
                ])),
                rows[0],
            );
            if let Some(msg) = &self.message {
                f.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        msg.clone(),
                        Style::default().fg(WARN),
                    )))
                    .alignment(Alignment::Center),
                    rows[3],
                );
            }
            return;
        }

        // ── transport line ──────────────────────────────────────────────────────
        let state = if playing {
            Span::styled(
                " ▶ ",
                Style::default().fg(SAFE).add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(" ⏸ ", Style::default().fg(DIM))
        };
        let cursor = if focused && self.selected == 0 && blink {
            "▌"
        } else {
            " "
        };
        let loop_on = practice.loop_enabled.load(Relaxed);
        let mut transport = vec![
            Span::styled(cursor.to_owned(), Style::default().fg(ACCENT)),
            state,
            Span::styled(
                format!(
                    "{} / {}  ",
                    mmss(position, self.sample_rate),
                    mmss(total, self.sample_rate)
                ),
                Style::default().fg(CHROME),
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
        f.render_widget(Paragraph::new(Line::from(transport)), rows[0]);

        // ── track blocks (3 rows each so the 10-row pane is all waveform) ────
        let width = inner.width as usize;
        f.render_widget(
            Paragraph::new(self.track_rows(
                "BACK",
                self.backing_name.as_deref(),
                &self.backing_peaks,
                self.backing_frames,
                0,
                practice.backing_muted.load(Relaxed),
                focused && self.selected == 1,
                practice,
                position,
                total,
                width,
                rows[1].height as usize,
            )),
            rows[1],
        );
        f.render_widget(
            Paragraph::new(self.track_rows(
                "TAKE",
                self.record_name.as_deref(),
                &self.record_peaks,
                self.record_frames,
                self.record_offset,
                practice.record_muted.load(Relaxed),
                focused && self.selected == 2,
                practice,
                position,
                total,
                width,
                rows[2].height as usize,
            )),
            rows[2],
        );

        // ── hint line ───────────────────────────────────────────────────────────
        let hint = if focused {
            Line::from(vec![
                Span::styled("↑/↓", Style::default().fg(AMBER)),
                Span::styled(" select  ", Style::default().fg(DIM)),
                Span::styled("Space", Style::default().fg(AMBER)),
                Span::styled(" play/mute  ", Style::default().fg(DIM)),
                Span::styled("←/→", Style::default().fg(AMBER)),
                Span::styled(" seek  ", Style::default().fg(DIM)),
                Span::styled("[ ]", Style::default().fg(AMBER)),
                Span::styled(" loop  ", Style::default().fg(DIM)),
                Span::styled("L", Style::default().fg(AMBER)),
                Span::styled(" on/off  ", Style::default().fg(DIM)),
                Span::styled("Del", Style::default().fg(AMBER)),
                Span::styled(" delete  ", Style::default().fg(DIM)),
                Span::styled("B", Style::default().fg(AMBER)),
                Span::styled(" browser", Style::default().fg(DIM)),
            ])
        } else {
            Line::from(vec![
                Span::styled("Tab", Style::default().fg(AMBER)),
                Span::styled(" to focus the timeline  ·  ", Style::default().fg(DIM)),
                Span::styled("B", Style::default().fg(AMBER)),
                Span::styled(" load a track", Style::default().fg(DIM)),
            ])
        };
        f.render_widget(Paragraph::new(hint).alignment(Alignment::Left), rows[3]);
    }

    /// Build one track's rows: focus cursor, mute LED, name, then a tall waveform
    /// with the playhead and loop shading. `track_frames` is the track's own length
    /// and `start` its timeline offset (0 for backing, the captured playhead for a
    /// take), so both align on the shared cursor. `height` is the rows available
    /// (3 in the 10-row pane); the first row carries the label and the wave cells
    /// repeat below it so the waveform reads tall instead of single-line.
    #[allow(clippy::too_many_arguments)]
    fn track_rows<'a>(
        &self,
        tag: &str,
        name: Option<&str>,
        peaks: &[(f32, f32)],
        track_frames: usize,
        start: usize,
        muted: bool,
        focused: bool,
        practice: &Practice,
        position: usize,
        total: usize,
        width: usize,
        height: usize,
    ) -> Vec<Line<'a>> {
        let loaded = name.is_some() && track_frames > 0;
        let led = if muted {
            Span::styled("○ ", Style::default().fg(DIM))
        } else if loaded {
            Span::styled("● ", Style::default().fg(SAFE))
        } else {
            Span::styled("· ", Style::default().fg(DIM))
        };
        let name = name.unwrap_or("empty");
        let label = format!("{tag:<4} ");
        let name_w = 16usize;
        let mut first = vec![
            Span::styled(
                if focused { "▌" } else { " " }.to_owned(),
                Style::default().fg(ACCENT),
            ),
            led,
            Span::styled(
                label.clone(),
                Style::default().fg(if loaded { CHROME } else { DIM }),
            ),
            Span::styled(
                format!("{:<name_w$}", truncate(name, name_w)),
                Style::default().fg(if loaded { CHROME } else { DIM }),
            ),
            Span::raw(" "),
        ];

        let used = 1 + 2 + label.len() + name_w + 1;
        let wave_w = width.saturating_sub(used);
        let height = height.max(1);
        if !loaded || wave_w == 0 || total == 0 || peaks.is_empty() || track_frames == 0 {
            let mut out = vec![Line::from(first)];
            while out.len() < height {
                out.push(Line::from(Span::raw("")));
            }
            return out;
        }

        let loop_on = practice.loop_enabled.load(Relaxed);
        let (la, lb) = (
            practice.loop_start.load(Relaxed) as usize,
            practice.loop_end.load(Relaxed) as usize,
        );
        let pos_col = position * (wave_w.saturating_sub(1)) / total;
        let bars = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        // One styled cell per waveform column, shared by all rows.
        let mut cells: Vec<Span<'a>> = Vec::with_capacity(wave_w);
        for col in 0..wave_w {
            let frame = col * total / wave_w.max(1);
            let in_loop = loop_on && lb > la && frame >= la && frame < lb;
            if col == pos_col {
                cells.push(Span::styled(
                    "│",
                    Style::default().fg(HOT).add_modifier(Modifier::BOLD),
                ));
                continue;
            }
            let inside = frame >= start && frame - start < track_frames;
            let ch = if !inside {
                if in_loop { '·' } else { ' ' }
            } else {
                let rel = frame - start;
                let bucket = rel * peaks.len() / track_frames.max(1);
                let amp = peaks
                    .get(bucket.min(peaks.len() - 1))
                    .map(|(lo, hi)| (hi - lo).max(0.0))
                    .unwrap_or(0.0);
                let level = (amp.sqrt() * 8.0).round() as usize;
                bars[level.min(bars.len() - 1)]
            };
            let color = if in_loop { HOT } else { CHROME };
            cells.push(Span::styled(ch.to_string(), Style::default().fg(color)));
        }
        first.extend(cells.iter().cloned());
        let mut out = vec![Line::from(first)];
        // Continuation rows: blank label gutter so the wave aligns under row 0,
        // same cells so the playhead reads as a vertical line.
        let gutter = Span::raw(" ".repeat(used));
        while out.len() < height {
            let mut spans = vec![gutter.clone()];
            spans.extend(cells.iter().cloned());
            out.push(Line::from(spans));
        }
        out
    }

    /// Render the backing/track browser modal.
    pub(super) fn render_browser(&self, f: &mut Frame) {
        let area = centered_rect(62, f.area());
        f.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(ACCENT))
            .title(Span::styled(
                " P R A C T I C E   T R A C K S ",
                Style::default().fg(AMBER).add_modifier(Modifier::BOLD),
            ))
            .style(Style::default().bg(Color::Black));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // slot line
                Constraint::Length(1), // path field
                Constraint::Min(1),    // file list
                Constraint::Length(1), // hint
                Constraint::Length(1), // message
            ])
            .split(inner);

        let slot_label = match self.slot {
            Slot::Backing => "Loading into: BACKING",
            Slot::Take => "Loading into: TAKE",
        };
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Tab", Style::default().fg(AMBER)),
                Span::styled(" switch target  ", Style::default().fg(DIM)),
                Span::styled(slot_label, Style::default().fg(CHROME)),
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

        // File list (highlight when the list has focus).
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
                Span::styled("Enter", Style::default().fg(AMBER)),
                Span::styled(" load  ", Style::default().fg(DIM)),
                Span::styled("Esc / B", Style::default().fg(AMBER)),
                Span::styled(" close", Style::default().fg(DIM)),
            ]))
            .alignment(Alignment::Center),
            rows[3],
        );

        let msg = if self.loading {
            "Loading…".to_owned()
        } else {
            self.message.clone().unwrap_or_default()
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(msg, Style::default().fg(WARN))))
                .alignment(Alignment::Center),
            rows[4],
        );
    }
}

/// Peak envelope over `buckets` buckets: (min, max) of the mono sum per bucket.
pub(super) fn peaks(track: &PlayerTrack, buckets: usize) -> Vec<(f32, f32)> {
    let n = track.frames();
    if n == 0 || buckets == 0 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(buckets);
    for b in 0..buckets {
        let start = b * n / buckets;
        let end = (((b + 1) * n) / buckets).max(start + 1).min(n);
        let (mut lo, mut hi) = (0.0f32, 0.0f32);
        for i in start..end {
            let s = 0.5 * (track.l[i] + track.r[i]);
            lo = lo.min(s);
            hi = hi.max(s);
        }
        out.push((lo, hi));
    }
    out
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

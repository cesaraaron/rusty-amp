//! Session (project) browser modal: New / Save / Save As / Load / Delete for
//! portable session folders under `~/.config/rusty-amp/sessions/`.

use std::path::PathBuf;

use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use super::styles::{ACCENT, AMBER, CHROME, DIM, HOT, SAFE};
use crate::project::{self, SessionEntry};

/// Which page is showing.
#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    /// Pick a saved session or an action.
    List,
    /// Type a name for Save As.
    NameInput,
}

/// What the UI loop should do in response to a keypress.
pub(super) enum Action {
    None,
    New,
    /// Save to the current session folder, or prompt when it has none.
    Save,
    /// Save As the typed name.
    SaveAs(String),
    Load(PathBuf),
    Delete(PathBuf),
}

/// Session-browser state, kept on the UI thread.
pub(super) struct SessionBrowser {
    pub open: bool,
    view: View,
    cursor: usize,
    entries: Vec<SessionEntry>,
    name_input: String,
    pub message: Option<String>,
}

impl SessionBrowser {
    pub(super) fn new() -> Self {
        Self {
            open: false,
            view: View::List,
            cursor: 0,
            entries: Vec::new(),
            name_input: String::new(),
            message: None,
        }
    }

    /// Open the modal, rescanning the saved sessions.
    pub(super) fn open(&mut self) {
        self.entries = project::list_sessions();
        self.cursor = 0;
        self.view = View::List;
        self.name_input.clear();
        self.message = None;
        self.open = true;
    }

    /// Switch to the name prompt for Save As (or Save without a folder).
    pub(super) fn prompt_name(&mut self, initial: &str) {
        self.view = View::NameInput;
        self.name_input = initial.to_owned();
    }

    /// Return to the session list view.
    pub(super) fn view_list(&mut self) {
        self.view = View::List;
    }

    /// Rescan after a save/delete.
    pub(super) fn refresh(&mut self) {
        self.entries = project::list_sessions();
        self.cursor = self.cursor.min(self.entries.len().saturating_sub(1));
    }

    pub(super) fn handle_key(&mut self, code: KeyCode) -> Action {
        match self.view {
            View::List => self.handle_list_key(code),
            View::NameInput => self.handle_name_key(code),
        }
    }

    fn handle_list_key(&mut self, code: KeyCode) -> Action {
        match code {
            KeyCode::Up => {
                self.cursor = self.cursor.saturating_sub(1);
                Action::None
            }
            KeyCode::Down => {
                self.cursor = (self.cursor + 1).min(self.entries.len().saturating_sub(1));
                Action::None
            }
            KeyCode::Enter => {
                if let Some(entry) = self.entries.get(self.cursor) {
                    return Action::Load(entry.dir.clone());
                }
                Action::None
            }
            KeyCode::Char('n') | KeyCode::Char('N') => Action::New,
            KeyCode::Char('s') | KeyCode::Char('S') => Action::Save,
            KeyCode::Char('a') | KeyCode::Char('A') => {
                self.prompt_name("");
                Action::None
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                if let Some(entry) = self.entries.get(self.cursor) {
                    Action::Delete(entry.dir.clone())
                } else {
                    Action::None
                }
            }
            KeyCode::Esc | KeyCode::Char('j') | KeyCode::Char('J') => {
                self.open = false;
                Action::None
            }
            _ => Action::None,
        }
    }

    fn handle_name_key(&mut self, code: KeyCode) -> Action {
        match code {
            KeyCode::Esc => {
                self.view = View::List;
                Action::None
            }
            KeyCode::Backspace => {
                self.name_input.pop();
                Action::None
            }
            KeyCode::Char(c) => {
                self.name_input.push(c);
                Action::None
            }
            KeyCode::Enter => {
                let name = self.name_input.trim().to_owned();
                if name.is_empty() {
                    self.message = Some("Session name cannot be empty".to_owned());
                    Action::None
                } else {
                    Action::SaveAs(name)
                }
            }
            _ => Action::None,
        }
    }

    /// Render the modal.
    pub(super) fn render(&self, f: &mut Frame) {
        let area = centered_rect(60, f.area());
        f.render_widget(Clear, area);
        let title = match self.view {
            View::List => " S E S S I O N S ",
            View::NameInput => " S A V E   S E S S I O N ",
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(ACCENT))
            .title(Span::styled(
                title,
                Style::default().fg(AMBER).add_modifier(Modifier::BOLD),
            ))
            .style(Style::default().bg(Color::Black));
        let inner = block.inner(area);
        f.render_widget(block, area);

        match self.view {
            View::List => self.render_list(f, inner),
            View::NameInput => self.render_name(f, inner),
        }
    }

    fn render_list(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(2)])
            .split(area);

        let mut lines: Vec<Line> = Vec::with_capacity(self.entries.len() + 1);
        if self.entries.is_empty() {
            lines.push(Line::from(Span::styled(
                "  (no saved sessions yet — press S to save the current one)",
                Style::default().fg(DIM),
            )));
        }
        for (i, e) in self.entries.iter().enumerate() {
            let selected = i == self.cursor;
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
            lines.push(Line::from(vec![
                Span::styled(prefix.to_owned(), Style::default().fg(ACCENT)),
                Span::styled(format!("{:<24}", e.name), style),
                Span::styled(
                    format!(
                        "  {} track{}",
                        e.tracks,
                        if e.tracks == 1 { "" } else { "s" }
                    ),
                    Style::default().fg(DIM),
                ),
            ]));
        }
        f.render_widget(Paragraph::new(lines), rows[0]);

        let footer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(rows[1]);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("↑/↓", Style::default().fg(AMBER)),
                Span::styled(" navigate  ", Style::default().fg(DIM)),
                Span::styled("Enter", Style::default().fg(AMBER)),
                Span::styled(" load  ", Style::default().fg(DIM)),
                Span::styled("N", Style::default().fg(AMBER)),
                Span::styled(" new  ", Style::default().fg(DIM)),
                Span::styled("S", Style::default().fg(AMBER)),
                Span::styled(" save  ", Style::default().fg(DIM)),
                Span::styled("A", Style::default().fg(AMBER)),
                Span::styled(" save as  ", Style::default().fg(DIM)),
                Span::styled("D", Style::default().fg(HOT)),
                Span::styled(" delete  ", Style::default().fg(DIM)),
                Span::styled("Esc / J", Style::default().fg(AMBER)),
                Span::styled(" close", Style::default().fg(DIM)),
            ]))
            .alignment(Alignment::Center),
            footer[0],
        );
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                self.message.clone().unwrap_or_default(),
                Style::default().fg(SAFE),
            )))
            .alignment(Alignment::Center),
            footer[1],
        );
    }

    fn render_name(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(2),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(area);
        f.render_widget(
            Paragraph::new(Span::styled(
                "Session name:",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            )),
            rows[0],
        );
        f.render_widget(
            Paragraph::new(Span::styled(
                format!("{}█", self.name_input),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ))
            .block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::default().fg(ACCENT)),
            ),
            rows[1],
        );
        if let Some(msg) = &self.message {
            f.render_widget(
                Paragraph::new(Span::styled(msg.clone(), Style::default().fg(HOT))),
                rows[2],
            );
        }
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Enter", Style::default().fg(AMBER)),
                Span::styled(" save  ", Style::default().fg(DIM)),
                Span::styled("Esc", Style::default().fg(AMBER)),
                Span::styled(" back", Style::default().fg(DIM)),
            ]))
            .alignment(Alignment::Center),
            rows[3],
        );
    }
}

fn centered_rect(percent_x: u16, area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    let width = area.width * percent_x / 100;
    let x = (area.width - width) / 2;
    let height = (area.height * 70 / 100).max(6);
    let y = (area.height - height) / 2;
    ratatui::layout::Rect {
        x: area.x + x,
        y: area.y + y,
        width,
        height,
    }
}

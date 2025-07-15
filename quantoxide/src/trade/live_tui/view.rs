use std::{
    fs::File,
    io::Write,
    sync::{Arc, Mutex, MutexGuard},
};

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use super::error::{LiveTuiError, Result};

pub trait LiveTuiLogger: Sync + Send + 'static {
    fn add_log_entry(&self, entry: String) -> Result<()>;
}

#[derive(Debug, PartialEq)]
enum ActivePane {
    StatePane,
    LogPane,
}

struct LiveTuiViewState {
    log_file: Option<File>,
    active_pane: ActivePane,

    log_entries: Vec<String>,
    log_max_line_width: usize,
    log_rect: Rect,
    log_v_scroll: usize,
    log_h_scroll: usize,

    state_lines: Vec<String>,
    state_max_line_width: usize,
    state_rect: Rect,
    state_v_scroll: usize,
    state_h_scroll: usize,
}

pub struct LiveTuiView {
    max_tui_log_len: usize,
    state: Mutex<LiveTuiViewState>,
}

impl LiveTuiView {
    pub fn new(max_tui_log_len: usize, log_file: Option<File>) -> Arc<Self> {
        Arc::new(Self {
            max_tui_log_len,
            state: Mutex::new(LiveTuiViewState {
                log_file,
                active_pane: ActivePane::StatePane,

                log_entries: Vec::new(),
                log_max_line_width: 0,
                log_rect: Rect::default(),
                log_v_scroll: 0,
                log_h_scroll: 0,

                state_lines: vec!["Initializing...".to_string()],
                state_max_line_width: 0,
                state_rect: Rect::default(),
                state_v_scroll: 0,
                state_h_scroll: 0,
            }),
        })
    }

    fn get_state(&self) -> MutexGuard<'_, LiveTuiViewState> {
        self.state
            .lock()
            .expect("`LiveTuiContent` mutex can't be poisoned")
    }

    fn max_scroll_down(rect: &Rect, entries_len: usize) -> usize {
        let visible_height = rect.height.saturating_sub(2) as usize; // Subtract borders
        entries_len.saturating_sub(visible_height)
    }

    // pub fn update_live_state(&self, state: String) {
    //     let mut state_guard = self
    //         .state
    //         .lock()
    //         .expect("`LiveTuiContent` mutex can't be poisoned");

    //     let mut new_lines = Vec::new();

    //     // Split the state into lines for display
    //     for line in state.lines() {
    //         state_guard.state_max_line_width = state_guard.state_max_line_width.max(line.len());
    //         new_lines.push(line.to_string());
    //     }

    //     new_lines.push("".to_string());

    //     if new_lines.len() != state_guard.state_lines.len() {
    //         if state_guard.state_v_scroll >= new_lines.len() && new_lines.len() > 0 {
    //             state_guard.state_v_scroll = new_lines.len().saturating_sub(1);
    //         }
    //     }

    //     state_guard.state_lines = new_lines;
    // }

    pub fn scroll_up(&self) {
        let mut state_guard = self.get_state();

        match state_guard.active_pane {
            ActivePane::StatePane => {
                state_guard.state_v_scroll = state_guard.state_v_scroll.saturating_sub(1)
            }
            ActivePane::LogPane => {
                state_guard.log_v_scroll = state_guard.log_v_scroll.saturating_sub(1)
            }
        }
    }

    pub fn scroll_down(&self) {
        let mut state_guard = self.get_state();

        match state_guard.active_pane {
            ActivePane::StatePane => {
                let max =
                    Self::max_scroll_down(&state_guard.state_rect, state_guard.state_lines.len());
                if state_guard.state_v_scroll < max {
                    state_guard.state_v_scroll += 1;
                }
            }
            ActivePane::LogPane => {
                let max =
                    Self::max_scroll_down(&state_guard.log_rect, state_guard.log_entries.len());
                if state_guard.log_v_scroll < max {
                    state_guard.log_v_scroll += 1;
                }
            }
        }
    }

    pub fn scroll_left(&self) {
        let mut state_guard = self.get_state();

        match state_guard.active_pane {
            ActivePane::StatePane => {
                state_guard.state_h_scroll = state_guard.state_h_scroll.saturating_sub(1);
            }
            ActivePane::LogPane => {
                state_guard.log_h_scroll = state_guard.log_h_scroll.saturating_sub(1);
            }
        }
    }

    fn max_scroll_right(rect: &Rect, max_line_width: usize) -> usize {
        let visible_width = rect.width.saturating_sub(4) as usize; // Subtract borders and padding
        max_line_width.saturating_sub(visible_width)
    }

    pub fn scroll_right(&self) {
        let mut state_guard = self.get_state();

        match state_guard.active_pane {
            ActivePane::StatePane => {
                let max = Self::max_scroll_right(
                    &state_guard.state_rect,
                    state_guard.state_max_line_width,
                );
                if state_guard.state_h_scroll < max {
                    state_guard.state_h_scroll += 1;
                }
            }
            ActivePane::LogPane => {
                let max =
                    Self::max_scroll_right(&state_guard.log_rect, state_guard.log_max_line_width);
                if state_guard.log_h_scroll < max {
                    state_guard.log_h_scroll += 1;
                }
            }
        }
    }

    pub fn reset_scroll(&self) {
        let mut state_guard = self.get_state();

        match state_guard.active_pane {
            ActivePane::StatePane => {
                state_guard.state_v_scroll = 0;
                state_guard.state_h_scroll = 0;
            }
            ActivePane::LogPane => {
                state_guard.log_v_scroll = 0;
                state_guard.log_h_scroll = 0;
            }
        }
    }

    pub fn scroll_to_bottom(&self) {
        let mut state_guard = self.get_state();

        match state_guard.active_pane {
            ActivePane::StatePane => {
                state_guard.state_v_scroll =
                    Self::max_scroll_down(&state_guard.state_rect, state_guard.state_lines.len());
                state_guard.state_h_scroll = 0;
            }
            ActivePane::LogPane => {
                state_guard.log_v_scroll =
                    Self::max_scroll_down(&state_guard.log_rect, state_guard.log_entries.len());
                state_guard.log_h_scroll = 0;
            }
        }
    }

    pub fn switch_pane(&self) {
        let mut state_guard = self.get_state();

        state_guard.active_pane = match state_guard.active_pane {
            ActivePane::StatePane => ActivePane::LogPane,
            ActivePane::LogPane => ActivePane::StatePane,
        };
    }

    fn get_list<'a>(
        title: &'a str,
        items: &'a [String],
        v_scroll: usize,
        h_scroll: usize,
        is_active: bool,
    ) -> List<'a> {
        let list_items: Vec<ListItem> = items
            .iter()
            .skip(v_scroll)
            .map(|item| {
                let content = if h_scroll >= item.len() {
                    String::new()
                } else {
                    item.chars().skip(h_scroll).collect()
                };
                ListItem::new(Line::from(vec![Span::raw(content)]))
            })
            .collect();

        let border_style = if is_active {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        };

        List::new(list_items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style),
        )
    }

    pub fn render(&self, f: &mut Frame) {
        let frame_rect = f.area();

        let main_area = Rect {
            x: frame_rect.x,
            y: frame_rect.y,
            width: frame_rect.width,
            height: frame_rect.height.saturating_sub(1), // Leave 1 row for help text
        };

        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(65), Constraint::Min(0)])
            .split(main_area);

        let mut state_guard = self.get_state();

        state_guard.state_rect = main_chunks[0];
        state_guard.log_rect = main_chunks[1];

        let state_list = Self::get_list(
            "Live State",
            &state_guard.state_lines,
            state_guard.state_v_scroll,
            state_guard.state_h_scroll,
            state_guard.active_pane == ActivePane::StatePane,
        );
        f.render_widget(state_list, state_guard.state_rect);

        let log_list = Self::get_list(
            "Log",
            &state_guard.log_entries,
            state_guard.log_v_scroll,
            state_guard.log_h_scroll,
            state_guard.active_pane == ActivePane::LogPane,
        );
        f.render_widget(log_list, state_guard.log_rect);

        let help_text = " Press 'q' to quit, Tab to switch panes, scroll with ↑/↓, ←/→, 'b' to bottom and 't' to top";
        let help_paragraph = Paragraph::new(help_text).style(Style::default().fg(Color::Gray));
        let help_area = Rect {
            x: frame_rect.x,
            y: frame_rect.y + frame_rect.height.saturating_sub(1), // Last row
            width: frame_rect.width,
            height: 1,
        };
        f.render_widget(help_paragraph, help_area);
    }
}

impl LiveTuiLogger for LiveTuiView {
    fn add_log_entry(&self, entry: String) -> Result<()> {
        let mut state_guard = self.get_state();

        let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();

        let lines: Vec<&str> = entry.lines().collect();

        if lines.is_empty() {
            return Ok(());
        }

        let mut log_entry = Vec::new();

        for (i, line) in lines.iter().enumerate() {
            let log_entry_line = if i == 0 {
                format!("[{}] {}", timestamp, line)
            } else {
                format!("           {}", line)
            };

            if let Some(log_file) = state_guard.log_file.as_mut() {
                writeln!(log_file, "{}", log_entry_line).map_err(|e| {
                    LiveTuiError::Generic(format!("couldn't write to log file {}", e.to_string()))
                })?;
                log_file.flush().map_err(|e| {
                    LiveTuiError::Generic(format!("couldn't flush log file {}", e.to_string()))
                })?;
            }

            log_entry.push(log_entry_line)
        }

        // Add entry at the beginning of the TUI log

        for entry_line in log_entry.into_iter().rev() {
            state_guard.log_max_line_width = state_guard.log_max_line_width.max(entry_line.len());
            state_guard.log_entries.insert(0, entry_line);
        }

        // Adjust scroll position to maintain the user's view
        if state_guard.log_v_scroll != 0 {
            state_guard.log_v_scroll = state_guard.log_v_scroll.saturating_add(lines.len());
        }

        if state_guard.log_entries.len() > self.max_tui_log_len {
            state_guard.log_entries.truncate(self.max_tui_log_len);

            let max_scroll =
                Self::max_scroll_down(&state_guard.log_rect, state_guard.log_entries.len());
            state_guard.log_v_scroll = state_guard.log_v_scroll.min(max_scroll);
        }

        Ok(())
    }
}

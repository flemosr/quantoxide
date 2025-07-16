use std::{
    fs::File,
    io::Write,
    sync::{Arc, Mutex, MutexGuard},
};

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::Paragraph,
};

use crate::tui::{Result, TuiError as LiveTuiError, TuiLogger, TuiView};

use super::LiveUiMessage;

#[derive(Debug, PartialEq)]
enum ActivePane {
    TradesPane,
    SummaryPane,
    LogPane,
}

pub struct LiveTuiViewState {
    log_file: Option<File>,
    active_pane: ActivePane,

    log_entries: Vec<String>,
    log_max_line_width: usize,
    log_rect: Rect,
    log_v_scroll: usize,
    log_h_scroll: usize,

    summary_lines: Vec<String>,
    summary_max_line_width: usize,
    summary_rect: Rect,
    summary_v_scroll: usize,
    summary_h_scroll: usize,

    trades_lines: Vec<String>,
    trades_max_line_width: usize,
    trades_rect: Rect,
    trades_v_scroll: usize,
    trades_h_scroll: usize,
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
                active_pane: ActivePane::LogPane,

                log_entries: Vec::new(),
                log_max_line_width: 0,
                log_rect: Rect::default(),
                log_v_scroll: 0,
                log_h_scroll: 0,

                summary_lines: vec!["Initializing...".to_string()],
                summary_max_line_width: 0,
                summary_rect: Rect::default(),
                summary_v_scroll: 0,
                summary_h_scroll: 0,

                trades_lines: vec!["Initializing...".to_string()],
                trades_max_line_width: 0,
                trades_rect: Rect::default(),
                trades_v_scroll: 0,
                trades_h_scroll: 0,
            }),
        })
    }

    pub fn update_summary(&self, state: String) {
        let mut state_guard = self.get_state();

        let mut new_lines: Vec<String> = state.lines().map(|line| line.to_string()).collect();
        new_lines.push("".to_string()); // Add empty line

        state_guard.summary_max_line_width = new_lines.iter().map(|line| line.len()).max().unwrap();

        // Only reset scroll if the content structure has significantly changed
        // or if current scroll position would be out of bounds
        if new_lines.len() != state_guard.summary_lines.len() {
            // If new content is shorter, adjust scroll to stay within bounds
            if state_guard.summary_v_scroll >= new_lines.len() && new_lines.len() > 0 {
                state_guard.summary_v_scroll = new_lines.len().saturating_sub(1);
            }
        }

        state_guard.summary_lines = new_lines;
    }

    pub fn update_trades(&self, trades_table: String) {
        let mut state_guard = self.get_state();

        let mut new_lines: Vec<String> =
            trades_table.lines().map(|line| line.to_string()).collect();
        new_lines.push("".to_string()); // Add empty line

        state_guard.trades_max_line_width = new_lines.iter().map(|line| line.len()).max().unwrap();

        // Only reset scroll if the content structure has significantly changed
        // or if current scroll position would be out of bounds
        if new_lines.len() != state_guard.trades_lines.len() {
            // If new content is shorter, adjust scroll to stay within bounds
            if state_guard.trades_v_scroll >= new_lines.len() && new_lines.len() > 0 {
                state_guard.trades_v_scroll = new_lines.len().saturating_sub(1);
            }
        }

        state_guard.trades_lines = new_lines;
    }
}

impl TuiLogger for LiveTuiView {
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

impl TuiView for LiveTuiView {
    type UiMessage = LiveUiMessage;

    type State = LiveTuiViewState;

    fn get_active_scroll_data(state: &Self::State) -> (usize, usize, &Rect, usize, usize) {
        match state.active_pane {
            ActivePane::TradesPane => (
                state.trades_v_scroll,
                state.trades_h_scroll,
                &state.trades_rect,
                state.trades_lines.len(),
                state.trades_max_line_width,
            ),
            ActivePane::SummaryPane => (
                state.summary_v_scroll,
                state.summary_h_scroll,
                &state.summary_rect,
                state.summary_lines.len(),
                state.summary_max_line_width,
            ),
            ActivePane::LogPane => (
                state.log_v_scroll,
                state.log_h_scroll,
                &state.log_rect,
                state.log_entries.len(),
                state.log_max_line_width,
            ),
        }
    }

    fn get_active_scroll_mut(state: &mut Self::State) -> (&mut usize, &mut usize) {
        match state.active_pane {
            ActivePane::TradesPane => (&mut state.trades_v_scroll, &mut state.trades_h_scroll),
            ActivePane::SummaryPane => (&mut state.summary_v_scroll, &mut state.summary_h_scroll),
            ActivePane::LogPane => (&mut state.log_v_scroll, &mut state.log_h_scroll),
        }
    }

    fn get_state(&self) -> MutexGuard<'_, Self::State> {
        self.state
            .lock()
            .expect("`LiveTuiView` mutex can't be poisoned")
    }

    fn handle_ui_message(&self, message: Self::UiMessage) -> Result<bool> {
        match message {
            LiveUiMessage::LogEntry(entry) => {
                self.add_log_entry(entry)?;
                Ok(false)
            }
            LiveUiMessage::SummaryUpdate(summary) => {
                self.update_summary(summary);
                Ok(false)
            }
            LiveUiMessage::TradesUpdate(trades_table) => {
                self.update_trades(trades_table);
                Ok(false)
            }
            LiveUiMessage::ShutdownCompleted => Ok(true),
        }
    }

    fn render(&self, f: &mut Frame) {
        let frame_rect = f.area();

        let main_area = Rect {
            x: frame_rect.x,
            y: frame_rect.y,
            width: frame_rect.width,
            height: frame_rect.height.saturating_sub(1), // Leave 1 row for help text
        };

        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(main_area);

        let bottom_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(52), Constraint::Min(0)].as_ref())
            .split(main_chunks[1]);

        let mut state_guard = self.get_state();

        state_guard.trades_rect = main_chunks[0];
        state_guard.summary_rect = bottom_chunks[0];
        state_guard.log_rect = bottom_chunks[1];

        let state_list = Self::get_list(
            "Trades",
            &state_guard.trades_lines,
            state_guard.trades_v_scroll,
            state_guard.trades_h_scroll,
            state_guard.active_pane == ActivePane::TradesPane,
        );
        f.render_widget(state_list, state_guard.trades_rect);

        let state_list = Self::get_list(
            "Trades Summary",
            &state_guard.summary_lines,
            state_guard.summary_v_scroll,
            state_guard.summary_h_scroll,
            state_guard.active_pane == ActivePane::SummaryPane,
        );
        f.render_widget(state_list, state_guard.summary_rect);

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

    fn scroll_up(&self) {
        let mut state_guard = self.get_state();

        let (v_scroll, _) = Self::get_active_scroll_mut(&mut state_guard);

        *v_scroll = v_scroll.saturating_sub(1);
    }

    fn scroll_down(&self) {
        let mut state_guard = self.get_state();
        let (curr_v_scroll, _, rect, lines_len, _) = Self::get_active_scroll_data(&state_guard);

        let max_v = Self::max_scroll_down(rect, lines_len);
        if curr_v_scroll < max_v {
            let (v_scroll, _) = Self::get_active_scroll_mut(&mut state_guard);

            *v_scroll += 1;
        }
    }

    fn scroll_left(&self) {
        let mut state_guard = self.get_state();

        let (_, h_scroll) = Self::get_active_scroll_mut(&mut state_guard);

        *h_scroll = h_scroll.saturating_sub(1);
    }

    fn scroll_right(&self) {
        let mut state_guard = self.get_state();

        let (_, current_h_scroll, rect, _, max_line_width) =
            Self::get_active_scroll_data(&state_guard);

        let max_h = Self::max_scroll_right(rect, max_line_width);
        if current_h_scroll < max_h {
            let (_, h_scroll) = Self::get_active_scroll_mut(&mut state_guard);

            *h_scroll += 1;
        }
    }

    fn reset_scroll(&self) {
        let mut state_guard = self.get_state();

        let (v_scroll, h_scroll) = Self::get_active_scroll_mut(&mut state_guard);

        *v_scroll = 0;
        *h_scroll = 0;
    }

    fn scroll_to_bottom(&self) {
        let mut state_guard = self.get_state();
        let (_, _, rect, lines_len, _) = Self::get_active_scroll_data(&state_guard);

        let max_v_scroll = Self::max_scroll_down(rect, lines_len);
        let (v_scroll, h_scroll) = Self::get_active_scroll_mut(&mut state_guard);

        *v_scroll = max_v_scroll;
        *h_scroll = 0;
    }

    fn switch_pane(&self) {
        let mut state_guard = self.get_state();

        state_guard.active_pane = match state_guard.active_pane {
            ActivePane::TradesPane => ActivePane::LogPane,
            ActivePane::LogPane => ActivePane::SummaryPane,
            ActivePane::SummaryPane => ActivePane::TradesPane,
        };
    }
}

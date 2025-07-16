use std::{
    fs::File,
    io::Write,
    sync::{Arc, Mutex, MutexGuard},
};

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};
use strum::EnumIter;

use crate::tui::{Result, TuiError as SyncTuiError, TuiLogger, TuiView};

use super::SyncUiMessage;

#[derive(Debug, PartialEq, EnumIter)]
pub enum SyncTuiPane {
    PriceHistoryStatePane,
    LogPane,
}

pub struct SyncTuiViewState {
    log_file: Option<File>,
    active_pane: SyncTuiPane,

    ph_state_lines: Vec<String>,
    ph_state_max_line_width: usize,
    ph_state_rect: Rect,
    ph_state_v_scroll: usize,
    ph_state_h_scroll: usize,

    log_entries: Vec<String>,
    log_max_line_width: usize,
    log_rect: Rect,
    log_v_scroll: usize,
    log_h_scroll: usize,
}

pub struct SyncTuiView {
    max_tui_log_len: usize,
    state: Mutex<SyncTuiViewState>,
}

impl SyncTuiView {
    pub fn new(max_tui_log_len: usize, log_file: Option<File>) -> Arc<Self> {
        Arc::new(Self {
            max_tui_log_len,
            state: Mutex::new(SyncTuiViewState {
                log_file,
                active_pane: SyncTuiPane::PriceHistoryStatePane,

                ph_state_lines: vec!["Initializing...".to_string()],
                ph_state_max_line_width: 0,
                ph_state_rect: Rect::default(),
                ph_state_v_scroll: 0,
                ph_state_h_scroll: 0,

                log_entries: Vec::new(),
                log_max_line_width: 0,
                log_rect: Rect::default(),
                log_v_scroll: 0,
                log_h_scroll: 0,
            }),
        })
    }

    pub fn update_sync_state(&self, state: String) {
        let mut state_guard = self
            .state
            .lock()
            .expect("`SyncTuiContent` mutex can't be poisoned");

        let mut new_lines = Vec::new();

        // Split the state into lines for display
        for line in state.lines() {
            state_guard.ph_state_max_line_width =
                state_guard.ph_state_max_line_width.max(line.len());
            new_lines.push(line.to_string());
        }

        new_lines.push("".to_string());

        if new_lines.len() != state_guard.ph_state_lines.len() {
            if state_guard.ph_state_v_scroll >= new_lines.len() && new_lines.len() > 0 {
                state_guard.ph_state_v_scroll = new_lines.len().saturating_sub(1);
            }
        }

        state_guard.ph_state_lines = new_lines;
    }
}

impl TuiLogger for SyncTuiView {
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
                    SyncTuiError::Generic(format!("couldn't write to log file {}", e.to_string()))
                })?;
                log_file.flush().map_err(|e| {
                    SyncTuiError::Generic(format!("couldn't flush log file {}", e.to_string()))
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

impl TuiView for SyncTuiView {
    type UiMessage = SyncUiMessage;

    type TuiPane = SyncTuiPane;

    type State = SyncTuiViewState;

    fn get_active_scroll_data(state: &Self::State) -> (usize, usize, &Rect, usize, usize) {
        match state.active_pane {
            SyncTuiPane::PriceHistoryStatePane => (
                state.ph_state_v_scroll,
                state.ph_state_h_scroll,
                &state.ph_state_rect,
                state.ph_state_lines.len(),
                state.ph_state_max_line_width,
            ),
            SyncTuiPane::LogPane => (
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
            SyncTuiPane::PriceHistoryStatePane => {
                (&mut state.ph_state_v_scroll, &mut state.ph_state_h_scroll)
            }
            SyncTuiPane::LogPane => (&mut state.log_v_scroll, &mut state.log_h_scroll),
        }
    }

    fn get_pane_render_info(
        state: &Self::State,
        pane: Self::TuiPane,
    ) -> (&'static str, &Vec<String>, usize, usize, Rect, bool) {
        match pane {
            SyncTuiPane::PriceHistoryStatePane => (
                "Price History State",
                &state.ph_state_lines,
                state.ph_state_v_scroll,
                state.ph_state_h_scroll,
                state.ph_state_rect,
                state.active_pane == SyncTuiPane::PriceHistoryStatePane,
            ),
            SyncTuiPane::LogPane => (
                "Log",
                &state.log_entries,
                state.log_v_scroll,
                state.log_h_scroll,
                state.log_rect,
                state.active_pane == SyncTuiPane::LogPane,
            ),
        }
    }

    fn get_state(&self) -> MutexGuard<'_, Self::State> {
        self.state
            .lock()
            .expect("`SyncTuiView` mutex can't be poisoned")
    }

    fn handle_ui_message(&self, message: Self::UiMessage) -> Result<bool> {
        match message {
            SyncUiMessage::LogEntry(entry) => {
                self.add_log_entry(entry)?;
                Ok(false)
            }
            SyncUiMessage::StateUpdate(state) => {
                self.update_sync_state(state);
                Ok(false)
            }
            SyncUiMessage::ShutdownCompleted => Ok(true),
        }
    }

    fn render(&self, f: &mut Frame) {
        let main_area = Self::get_main_area(f);

        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(65), Constraint::Min(0)])
            .split(main_area);

        let mut state_guard = self.get_state();

        state_guard.ph_state_rect = main_chunks[0];
        state_guard.log_rect = main_chunks[1];

        Self::render_widgets(f, &state_guard);
    }

    fn switch_pane(&self) {
        let mut state_guard = self.get_state();

        state_guard.active_pane = match state_guard.active_pane {
            SyncTuiPane::PriceHistoryStatePane => SyncTuiPane::LogPane,
            SyncTuiPane::LogPane => SyncTuiPane::PriceHistoryStatePane,
        };
    }
}

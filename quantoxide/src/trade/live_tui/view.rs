use std::{
    fs::File,
    sync::{Arc, Mutex, MutexGuard},
};

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};
use strum::EnumIter;

use crate::tui::{Result, TuiLogger, TuiView};

use super::LiveUiMessage;

#[derive(Debug, PartialEq, EnumIter)]
pub enum LiveTuiPane {
    TradesPane,
    SummaryPane,
    LogPane,
}

pub struct LiveTuiViewState {
    log_file: Option<File>,
    active_pane: LiveTuiPane,

    trades_lines: Vec<String>,
    trades_max_line_width: usize,
    trades_rect: Rect,
    trades_v_scroll: usize,
    trades_h_scroll: usize,

    summary_lines: Vec<String>,
    summary_max_line_width: usize,
    summary_rect: Rect,
    summary_v_scroll: usize,
    summary_h_scroll: usize,

    log_entries: Vec<String>,
    log_max_line_width: usize,
    log_rect: Rect,
    log_v_scroll: usize,
    log_h_scroll: usize,
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
                active_pane: LiveTuiPane::LogPane,

                trades_lines: vec!["Initializing...".to_string()],
                trades_max_line_width: 0,
                trades_rect: Rect::default(),
                trades_v_scroll: 0,
                trades_h_scroll: 0,

                summary_lines: vec!["Initializing...".to_string()],
                summary_max_line_width: 0,
                summary_rect: Rect::default(),
                summary_v_scroll: 0,
                summary_h_scroll: 0,

                log_entries: Vec::new(),
                log_max_line_width: 0,
                log_rect: Rect::default(),
                log_v_scroll: 0,
                log_h_scroll: 0,
            }),
        })
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
}

impl TuiLogger for LiveTuiView {
    type State = LiveTuiViewState;

    fn get_max_tui_log_len(&self) -> usize {
        self.max_tui_log_len
    }

    fn get_log_components_mut(
        state: &mut Self::State,
    ) -> (
        Option<&mut File>,
        &mut Vec<String>,
        &mut usize,
        Rect,
        &mut usize,
    ) {
        (
            state.log_file.as_mut(),
            &mut state.log_entries,
            &mut state.log_max_line_width,
            state.log_rect.clone(),
            &mut state.log_v_scroll,
        )
    }

    fn get_state(&self) -> MutexGuard<'_, Self::State> {
        self.state
            .lock()
            .expect("`LiveTuiView` mutex can't be poisoned")
    }
}

impl TuiView for LiveTuiView {
    type UiMessage = LiveUiMessage;

    type TuiPane = LiveTuiPane;

    fn get_active_scroll_data(state: &Self::State) -> (usize, usize, &Rect, usize, usize) {
        match state.active_pane {
            LiveTuiPane::TradesPane => (
                state.trades_v_scroll,
                state.trades_h_scroll,
                &state.trades_rect,
                state.trades_lines.len(),
                state.trades_max_line_width,
            ),
            LiveTuiPane::SummaryPane => (
                state.summary_v_scroll,
                state.summary_h_scroll,
                &state.summary_rect,
                state.summary_lines.len(),
                state.summary_max_line_width,
            ),
            LiveTuiPane::LogPane => (
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
            LiveTuiPane::TradesPane => (&mut state.trades_v_scroll, &mut state.trades_h_scroll),
            LiveTuiPane::SummaryPane => (&mut state.summary_v_scroll, &mut state.summary_h_scroll),
            LiveTuiPane::LogPane => (&mut state.log_v_scroll, &mut state.log_h_scroll),
        }
    }

    fn get_pane_render_info(
        state: &Self::State,
        pane: Self::TuiPane,
    ) -> (&'static str, &Vec<String>, usize, usize, Rect, bool) {
        match pane {
            LiveTuiPane::TradesPane => (
                "Trades",
                &state.trades_lines,
                state.trades_v_scroll,
                state.trades_h_scroll,
                state.trades_rect,
                state.active_pane == LiveTuiPane::TradesPane,
            ),
            LiveTuiPane::SummaryPane => (
                "Trades Summary",
                &state.summary_lines,
                state.summary_v_scroll,
                state.summary_h_scroll,
                state.summary_rect,
                state.active_pane == LiveTuiPane::SummaryPane,
            ),
            LiveTuiPane::LogPane => (
                "Log",
                &state.log_entries,
                state.log_v_scroll,
                state.log_h_scroll,
                state.log_rect,
                state.active_pane == LiveTuiPane::LogPane,
            ),
        }
    }

    fn handle_ui_message(&self, message: Self::UiMessage) -> Result<bool> {
        match message {
            LiveUiMessage::TradesUpdate(trades_table) => {
                self.update_trades(trades_table);
                Ok(false)
            }
            LiveUiMessage::SummaryUpdate(summary) => {
                self.update_summary(summary);
                Ok(false)
            }
            LiveUiMessage::LogEntry(entry) => {
                self.add_log_entry(entry)?;
                Ok(false)
            }
            LiveUiMessage::ShutdownCompleted => Ok(true),
        }
    }

    fn render(&self, f: &mut Frame) {
        let main_area = Self::get_main_area(f);

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

        Self::render_widgets(f, &state_guard);
    }

    fn switch_pane(&self) {
        let mut state_guard = self.get_state();

        state_guard.active_pane = match state_guard.active_pane {
            LiveTuiPane::TradesPane => LiveTuiPane::LogPane,
            LiveTuiPane::LogPane => LiveTuiPane::SummaryPane,
            LiveTuiPane::SummaryPane => LiveTuiPane::TradesPane,
        };
    }
}

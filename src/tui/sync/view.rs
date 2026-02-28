use std::{
    fs::File,
    sync::{Arc, Mutex, MutexGuard},
};

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};
use strum::EnumIter;

use super::{
    super::{
        error::Result,
        view::{TuiLogManager, TuiView},
    },
    SyncUiMessage,
};

#[derive(Debug, PartialEq, EnumIter)]
pub(in crate::tui) enum SyncTuiPane {
    SyncStatePane,
    LogPane,
}

pub(in crate::tui) struct SyncTuiViewState {
    log_file: Option<File>,
    active_pane: SyncTuiPane,

    ph_state_text: String,
    fs_state_text: String,
    state_lines: Vec<String>,
    state_max_line_width: usize,
    state_rect: Rect,
    state_v_scroll: usize,
    state_h_scroll: usize,

    log_entries: Vec<String>,
    log_max_line_width: usize,
    log_rect: Rect,
    log_v_scroll: usize,
    log_h_scroll: usize,
}

pub(in crate::tui) struct SyncTuiView {
    max_tui_log_len: usize,
    state: Mutex<SyncTuiViewState>,
}

impl SyncTuiView {
    pub fn new(max_tui_log_len: usize, log_file: Option<File>) -> Arc<Self> {
        Arc::new(Self {
            max_tui_log_len,
            state: Mutex::new(SyncTuiViewState {
                log_file,
                active_pane: SyncTuiPane::LogPane,

                ph_state_text: String::new(),
                fs_state_text: String::new(),
                state_lines: vec!["Initializing...".to_string()],
                state_max_line_width: 0,
                state_rect: Rect::default(),
                state_v_scroll: 0,
                state_h_scroll: 0,

                log_entries: Vec::new(),
                log_max_line_width: 0,
                log_rect: Rect::default(),
                log_v_scroll: 0,
                log_h_scroll: 0,
            }),
        })
    }

    fn compose_state_pane(state: &SyncTuiViewState) -> String {
        let mut result = String::new();

        if !state.ph_state_text.is_empty() {
            result.push_str("\n[Price History]\n");
            result.push_str(&state.ph_state_text);
        }

        if !state.fs_state_text.is_empty() && !state.ph_state_text.is_empty() {
            result.push_str("\n\n[Funding Settlements]\n");
            result.push_str(&state.fs_state_text);
        }

        result
    }
}

impl TuiLogManager for SyncTuiView {
    type State = SyncTuiViewState;

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
            state.log_rect,
            &mut state.log_v_scroll,
        )
    }

    fn get_state(&self) -> MutexGuard<'_, Self::State> {
        self.state
            .lock()
            .expect("`SyncTuiView` mutex can't be poisoned")
    }
}

impl TuiView for SyncTuiView {
    type UiMessage = SyncUiMessage;

    type TuiPane = SyncTuiPane;

    fn get_active_scroll_data(state: &Self::State) -> (usize, usize, &Rect, usize, usize) {
        match state.active_pane {
            SyncTuiPane::SyncStatePane => (
                state.state_v_scroll,
                state.state_h_scroll,
                &state.state_rect,
                state.state_lines.len(),
                state.state_max_line_width,
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
            SyncTuiPane::SyncStatePane => (&mut state.state_v_scroll, &mut state.state_h_scroll),
            SyncTuiPane::LogPane => (&mut state.log_v_scroll, &mut state.log_h_scroll),
        }
    }

    fn get_pane_render_info(
        state: &Self::State,
        pane: Self::TuiPane,
    ) -> (&'static str, &Vec<String>, usize, usize, Rect, bool) {
        match pane {
            SyncTuiPane::SyncStatePane => (
                "Sync State",
                &state.state_lines,
                state.state_v_scroll,
                state.state_h_scroll,
                state.state_rect,
                state.active_pane == SyncTuiPane::SyncStatePane,
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

    fn get_pane_data_mut(
        state: &mut Self::State,
        pane: Self::TuiPane,
    ) -> (&mut Vec<String>, &mut usize, &mut usize) {
        match pane {
            SyncTuiPane::SyncStatePane => (
                &mut state.state_lines,
                &mut state.state_max_line_width,
                &mut state.state_v_scroll,
            ),
            SyncTuiPane::LogPane => (
                &mut state.log_entries,
                &mut state.log_max_line_width,
                &mut state.log_v_scroll,
            ),
        }
    }

    fn handle_ui_message(&self, message: Self::UiMessage) -> Result<bool> {
        match message {
            SyncUiMessage::PriceHistoryStateUpdate(text) => {
                let mut state_guard = self.get_state();
                state_guard.ph_state_text = text;
                let combined = Self::compose_state_pane(&state_guard);
                drop(state_guard);
                self.update_pane_content(SyncTuiPane::SyncStatePane, combined);
                Ok(false)
            }
            SyncUiMessage::FundingSettlementsStateUpdate(text) => {
                let mut state_guard = self.get_state();
                state_guard.fs_state_text = text;
                let combined = Self::compose_state_pane(&state_guard);
                drop(state_guard);
                self.update_pane_content(SyncTuiPane::SyncStatePane, combined);
                Ok(false)
            }
            SyncUiMessage::LogEntry(entry) => {
                self.add_log_entry(entry)?;
                Ok(false)
            }
            SyncUiMessage::ShutdownCompleted => Ok(true),
        }
    }

    fn render(&self, f: &mut Frame) {
        let main_area = Self::get_main_area(f);

        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(52), Constraint::Min(0)])
            .split(main_area);

        let mut state_guard = self.get_state();

        state_guard.state_rect = main_chunks[0];
        state_guard.log_rect = main_chunks[1];

        Self::render_panes(f, &state_guard);
    }

    fn switch_pane(&self) {
        let mut state_guard = self.get_state();

        state_guard.active_pane = match state_guard.active_pane {
            SyncTuiPane::SyncStatePane => SyncTuiPane::LogPane,
            SyncTuiPane::LogPane => SyncTuiPane::SyncStatePane,
        };
    }
}

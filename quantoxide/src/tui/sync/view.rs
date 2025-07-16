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
        view::{TuiLogger, TuiView},
    },
    SyncUiMessage,
};

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
}

impl TuiLogger for SyncTuiView {
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
            state.log_rect.clone(),
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

    fn get_pane_data_mut(
        state: &mut Self::State,
        pane: Self::TuiPane,
    ) -> (&mut Vec<String>, &mut usize, &mut usize) {
        match pane {
            SyncTuiPane::PriceHistoryStatePane => (
                &mut state.ph_state_lines,
                &mut state.ph_state_max_line_width,
                &mut state.ph_state_v_scroll,
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
            SyncUiMessage::StateUpdate(state) => {
                self.update_pane_content(SyncTuiPane::PriceHistoryStatePane, state);
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

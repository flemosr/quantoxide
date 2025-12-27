use std::{
    fs::File,
    sync::{Arc, Mutex, MutexGuard},
};

use chrono::{DateTime, Utc};
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
    BacktestUiMessage,
};

mod net_value_chart;

use net_value_chart::NetValueChartData;

#[derive(Debug, PartialEq, EnumIter)]
pub(in crate::tui) enum BacktestTuiPane {
    TradingStatePane,
    LogPane,
}

pub(in crate::tui) struct BacktestTuiViewState {
    log_file: Option<File>,
    active_pane: BacktestTuiPane,

    chart_data: NetValueChartData,

    td_state_lines: Vec<String>,
    td_state_max_line_width: usize,
    td_state_rect: Rect,
    td_state_v_scroll: usize,
    td_state_h_scroll: usize,

    log_entries: Vec<String>,
    log_max_line_width: usize,
    log_rect: Rect,
    log_v_scroll: usize,
    log_h_scroll: usize,
}

pub(in crate::tui) struct BacktestTuiView {
    max_tui_log_len: usize,
    state: Mutex<BacktestTuiViewState>,
}

impl BacktestTuiView {
    pub fn new(max_tui_log_len: usize, log_file: Option<File>) -> Arc<Self> {
        Arc::new(Self {
            max_tui_log_len,
            state: Mutex::new(BacktestTuiViewState {
                log_file,
                active_pane: BacktestTuiPane::LogPane,

                chart_data: NetValueChartData::new(),

                td_state_lines: vec!["Initializing...".to_string()],
                td_state_max_line_width: 0,
                td_state_rect: Rect::default(),
                td_state_v_scroll: 0,
                td_state_h_scroll: 0,

                log_entries: Vec::new(),
                log_max_line_width: 0,
                log_rect: Rect::default(),
                log_v_scroll: 0,
                log_h_scroll: 0,
            }),
        })
    }

    pub fn initialize_chart(
        &self,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        start_balance: u64,
    ) {
        let mut state_guard = self.state.lock().expect("not poisoned");

        state_guard
            .chart_data
            .initialize(start_time, end_time, start_balance);
    }

    pub fn add_chart_point(&self, time: DateTime<Utc>, balance: u64) {
        let mut state_guard = self.state.lock().expect("not poisoned");

        state_guard.chart_data.add_point(time, balance);
    }
}

impl TuiLogManager for BacktestTuiView {
    type State = BacktestTuiViewState;

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
            .expect("`BacktestTuiView` mutex can't be poisoned")
    }
}

impl TuiView for BacktestTuiView {
    type UiMessage = BacktestUiMessage;

    type TuiPane = BacktestTuiPane;

    fn get_active_scroll_data(state: &Self::State) -> (usize, usize, &Rect, usize, usize) {
        match state.active_pane {
            BacktestTuiPane::TradingStatePane => (
                state.td_state_v_scroll,
                state.td_state_h_scroll,
                &state.td_state_rect,
                state.td_state_lines.len(),
                state.td_state_max_line_width,
            ),
            BacktestTuiPane::LogPane => (
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
            BacktestTuiPane::TradingStatePane => {
                (&mut state.td_state_v_scroll, &mut state.td_state_h_scroll)
            }
            BacktestTuiPane::LogPane => (&mut state.log_v_scroll, &mut state.log_h_scroll),
        }
    }

    fn get_pane_render_info(
        state: &Self::State,
        pane: Self::TuiPane,
    ) -> (&'static str, &Vec<String>, usize, usize, Rect, bool) {
        match pane {
            BacktestTuiPane::TradingStatePane => (
                "Trading State",
                &state.td_state_lines,
                state.td_state_v_scroll,
                state.td_state_h_scroll,
                state.td_state_rect,
                state.active_pane == BacktestTuiPane::TradingStatePane,
            ),
            BacktestTuiPane::LogPane => (
                "Log",
                &state.log_entries,
                state.log_v_scroll,
                state.log_h_scroll,
                state.log_rect,
                state.active_pane == BacktestTuiPane::LogPane,
            ),
        }
    }

    fn get_pane_data_mut(
        state: &mut Self::State,
        pane: Self::TuiPane,
    ) -> (&mut Vec<String>, &mut usize, &mut usize) {
        match pane {
            BacktestTuiPane::TradingStatePane => (
                &mut state.td_state_lines,
                &mut state.td_state_max_line_width,
                &mut state.td_state_v_scroll,
            ),
            BacktestTuiPane::LogPane => (
                &mut state.log_entries,
                &mut state.log_max_line_width,
                &mut state.log_v_scroll,
            ),
        }
    }

    fn handle_ui_message(&self, message: Self::UiMessage) -> Result<bool> {
        match message {
            BacktestUiMessage::StateUpdate(state) => {
                self.add_chart_point(state.last_tick_time(), state.total_net_value());

                self.update_pane_content(
                    BacktestTuiPane::TradingStatePane,
                    format!("\n{}", state.summary()),
                );
                Ok(false)
            }
            BacktestUiMessage::LogEntry(entry) => {
                self.add_log_entry(entry)?;
                Ok(false)
            }
            BacktestUiMessage::ShutdownCompleted => Ok(true),
        }
    }

    fn render(&self, f: &mut Frame) {
        let main_area = Self::get_main_area(f);

        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
            .split(main_area);

        let bottom_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(55), Constraint::Min(0)])
            .split(main_chunks[1]);

        let mut state_guard = self.get_state();

        let chart_area = main_chunks[0];

        let chart = state_guard.chart_data.to_widget();
        f.render_widget(chart, chart_area);

        state_guard.td_state_rect = bottom_chunks[0];
        state_guard.log_rect = bottom_chunks[1];

        Self::render_panes(f, &state_guard);
    }

    fn switch_pane(&self) {
        let mut state_guard = self.get_state();

        state_guard.active_pane = match state_guard.active_pane {
            BacktestTuiPane::TradingStatePane => BacktestTuiPane::LogPane,
            BacktestTuiPane::LogPane => BacktestTuiPane::TradingStatePane,
        };
    }
}

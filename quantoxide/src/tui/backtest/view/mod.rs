use std::{
    f64,
    fs::File,
    sync::{Arc, Mutex, MutexGuard},
};

use chrono::{DateTime, Utc};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    symbols::Marker,
    text::Span,
    widgets::{Axis, Block, Borders, Chart, Dataset, GraphType},
};
use strum::EnumIter;

use super::{
    super::{
        error::Result,
        view::{TuiLogger, TuiView},
    },
    BacktestUiMessage,
};

#[derive(Debug, PartialEq, EnumIter)]
pub enum BacktestTuiPane {
    TradingStatePane,
    LogPane,
}

struct BalanceChartData {
    title: String,
    data: Vec<(f64, f64)>,
    start_time: f64,
    end_time: f64,
    max_balance: f64,
}

impl BalanceChartData {
    fn new() -> Self {
        Self {
            title: "No Data Available".to_string(),
            data: vec![],
            start_time: 0.0,
            end_time: 0.0,
            max_balance: 0.0,
        }
    }

    fn initialize(
        &mut self,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        start_balance: u64,
    ) {
        let start_time = start_time.timestamp() as f64;
        let start_balance = start_balance as f64;

        self.title = "Balance over time".to_string();
        self.start_time = start_time;
        self.end_time = end_time.timestamp() as f64;
        self.max_balance = start_balance;

        self.data.push((start_time, start_balance))
    }

    fn add_point(&mut self, time: DateTime<Utc>, balance: u64) {
        let balance = balance as f64;

        if balance > self.max_balance {
            self.max_balance = balance;
        }

        self.data.push((time.timestamp() as f64, balance))
    }

    fn to_widget(&self) -> Chart<'_> {
        let y_min = 0.; // Keep y axis starting at 0
        let y_max = self.max_balance * 1.1; // Add padding to max_balance

        let datasets = vec![
            Dataset::default()
                .marker(Marker::Dot)
                .graph_type(GraphType::Scatter)
                .style(Style::default().fg(Color::Cyan))
                .data(&self.data),
        ];

        let x_labels = [
            self.start_time,
            (self.start_time + self.end_time) / 2.,
            self.end_time,
        ]
        .iter()
        .map(|&time| {
            Span::raw(
                DateTime::from_timestamp(time as i64, 0)
                    .unwrap()
                    .format("%y/%m/%d")
                    .to_string(),
            )
        })
        .collect::<Vec<_>>();

        let y_labels = [y_min, (y_min + y_max) / 2., y_max]
            .iter()
            .map(|s| Span::raw((*s as u64).to_string()))
            .collect::<Vec<_>>();

        Chart::new(datasets)
            .block(
                Block::default()
                    .title("Balance over time")
                    .borders(Borders::ALL),
            )
            .x_axis(
                Axis::default()
                    .title("Time [UTC]")
                    .style(Style::default().fg(Color::Gray))
                    .bounds([self.start_time, self.end_time])
                    .labels(x_labels),
            )
            .y_axis(
                Axis::default()
                    .title("Balance [sats]")
                    .style(Style::default().fg(Color::Gray))
                    .bounds([y_min, y_max])
                    .labels(y_labels),
            )
    }
}

pub struct BacktestTuiViewState {
    log_file: Option<File>,
    active_pane: BacktestTuiPane,

    chart_data: BalanceChartData,

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

pub struct BacktestTuiView {
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

                chart_data: BalanceChartData::new(),

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

impl TuiLogger for BacktestTuiView {
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
            state.log_rect.clone(),
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
                self.add_chart_point(state.current_time(), state.current_balance());

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
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(main_area);

        let bottom_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(65), Constraint::Min(0)])
            .split(main_chunks[1]);

        let mut state_guard = self.get_state();

        let chart_area = main_chunks[0];

        let chart = state_guard.chart_data.to_widget();
        f.render_widget(chart, chart_area);

        state_guard.td_state_rect = bottom_chunks[0];
        state_guard.log_rect = bottom_chunks[1];

        Self::render_widgets(f, &state_guard);
    }

    fn switch_pane(&self) {
        let mut state_guard = self.get_state();

        state_guard.active_pane = match state_guard.active_pane {
            BacktestTuiPane::TradingStatePane => BacktestTuiPane::LogPane,
            BacktestTuiPane::LogPane => BacktestTuiPane::TradingStatePane,
        };
    }
}

use std::f64;

use chrono::{DateTime, Utc};
use ratatui::{
    style::{Color, Style},
    symbols::Marker,
    text::Span,
    widgets::{Axis, Block, Borders, Chart, Dataset, GraphType},
};

pub struct BalanceChartData {
    title: String,
    data: Vec<(f64, f64)>,
    start_time: f64,
    end_time: f64,
    max_balance: f64,
}

impl BalanceChartData {
    pub fn new() -> Self {
        Self {
            title: "No Data Available".to_string(),
            data: vec![],
            start_time: 0.0,
            end_time: 0.0,
            max_balance: 0.0,
        }
    }

    pub fn initialize(
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

    pub fn add_point(&mut self, time: DateTime<Utc>, balance: u64) {
        let balance = balance as f64;

        if balance > self.max_balance {
            self.max_balance = balance;
        }

        self.data.push((time.timestamp() as f64, balance))
    }

    pub fn to_widget(&self) -> Chart<'_> {
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

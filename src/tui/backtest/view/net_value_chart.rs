use chrono::{DateTime, Utc};
use ratatui::{
    style::{Color, Style},
    symbols::Marker,
    text::Span,
    widgets::{Axis, Block, Borders, Chart, Dataset, GraphType, Padding},
};

use crate::models::SATS_PER_BTC;

#[derive(Default, Clone, Copy)]
pub(super) enum ChartMode {
    #[default]
    Sats,
    BtcPrice,
    Usd,
}

pub(super) struct NetValueChartData {
    title: String,
    data_nav_sats: Vec<(f64, f64)>,
    data_btc_price: Vec<(f64, f64)>,
    data_nav_usd: Vec<(f64, f64)>,
    start_time: f64,
    end_time: f64,
    max_nav_sats: f64,
    max_btc_price: f64,
    max_nav_usd: f64,
    active_chart: ChartMode,
}

impl NetValueChartData {
    pub fn new() -> Self {
        Self {
            title: "No Data Available".to_string(),
            data_nav_sats: vec![],
            data_btc_price: vec![],
            data_nav_usd: vec![],
            start_time: 0.0,
            end_time: 0.0,
            max_nav_sats: 0.0,
            max_btc_price: 0.0,
            max_nav_usd: 0.0,
            active_chart: ChartMode::default(),
        }
    }

    pub fn initialize(
        &mut self,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        start_net_value: u64,
    ) {
        let start_time = start_time.timestamp() as f64;
        let start_net_value = start_net_value as f64;

        self.title = "Total net value over time".to_string();
        self.start_time = start_time;
        self.end_time = end_time.timestamp() as f64;
        self.max_nav_sats = start_net_value;

        self.data_nav_sats.push((start_time, start_net_value))
    }

    pub fn add_point(&mut self, time: DateTime<Utc>, total_net_value: u64, market_price: f64) {
        let total_net_value_f64 = total_net_value as f64;
        let timestamp = time.timestamp() as f64;

        if total_net_value_f64 > self.max_nav_sats {
            self.max_nav_sats = total_net_value_f64;
        }
        self.data_nav_sats.push((timestamp, total_net_value_f64));

        if market_price > self.max_btc_price {
            self.max_btc_price = market_price;
        }
        self.data_btc_price.push((timestamp, market_price));

        let net_value_usd = total_net_value_f64 * market_price / SATS_PER_BTC;
        if net_value_usd > self.max_nav_usd {
            self.max_nav_usd = net_value_usd;
        }
        self.data_nav_usd.push((timestamp, net_value_usd));
    }

    pub fn set_chart_mode(&mut self, mode: ChartMode) {
        self.active_chart = mode;
    }

    pub fn to_widget(&self) -> Chart<'_> {
        let (data, max_value, block_title, y_title, format_usd, chart_color) =
            match self.active_chart {
                ChartMode::Sats => (
                    &self.data_nav_sats,
                    self.max_nav_sats,
                    "[x] NAV [sats] | [ ] BTC Price | [ ] NAV [USD]",
                    "NAV [sats]",
                    false,
                    Color::Rgb(255, 165, 0),
                ),
                ChartMode::BtcPrice => (
                    &self.data_btc_price,
                    self.max_btc_price,
                    "[ ] NAV [sats] | [x] BTC Price | [ ] NAV [USD]",
                    "Price [USD]",
                    true,
                    Color::Rgb(180, 100, 255),
                ),
                ChartMode::Usd => (
                    &self.data_nav_usd,
                    self.max_nav_usd,
                    "[ ] NAV [sats] | [ ] BTC Price | [x] NAV [USD]",
                    "NAV [USD]",
                    true,
                    Color::Rgb(0, 210, 210),
                ),
            };

        let y_min = 0.; // Keep y axis starting at 0
        let y_max = max_value * 1.1; // Add padding to max value

        let datasets = vec![
            Dataset::default()
                .marker(Marker::Dot)
                .graph_type(GraphType::Scatter)
                .style(Style::default().fg(chart_color))
                .data(data),
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
            .map(|s| {
                let txt = if format_usd {
                    format!("{:.2}", s)
                } else {
                    (*s as u64).to_string()
                };
                Span::raw(txt)
            })
            .collect::<Vec<_>>();

        Chart::new(datasets)
            .block(
                Block::default()
                    .title(block_title)
                    .borders(Borders::ALL)
                    .padding(Padding::top(1)),
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
                    .title(y_title)
                    .style(Style::default().fg(Color::Gray))
                    .bounds([y_min, y_max])
                    .labels(y_labels),
            )
    }
}

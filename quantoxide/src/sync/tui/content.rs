use std::{fs::File, io::Write};

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use super::super::{SyncError, error::Result};

const MAX_LOG_ENTRIES: usize = 10_000;

#[derive(Debug, PartialEq)]
enum ActivePane {
    StatePane,
    LogPane,
}

pub struct SyncTuiContent {
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

impl SyncTuiContent {
    pub fn new(log_file: Option<File>) -> Self {
        Self {
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
        }
    }

    fn max_scroll_down(rect: &Rect, entries_len: usize) -> usize {
        let visible_height = rect.height.saturating_sub(2) as usize; // Subtract borders
        entries_len.saturating_sub(visible_height)
    }

    pub fn update_state(&mut self, state: String) {
        let mut new_lines = Vec::new();

        // Split the state into lines for display
        for line in state.lines() {
            self.state_max_line_width = self.state_max_line_width.max(line.len());
            new_lines.push(line.to_string());
        }

        new_lines.push("".to_string());

        if new_lines.len() != self.state_lines.len() {
            if self.state_v_scroll >= new_lines.len() && new_lines.len() > 0 {
                self.state_v_scroll = new_lines.len().saturating_sub(1);
            }
        }

        self.state_lines = new_lines;
    }

    pub fn add_log_entry(&mut self, entry: String) -> Result<()> {
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

            if let Some(log_file) = self.log_file.as_mut() {
                writeln!(log_file, "{}", log_entry_line).map_err(|e| {
                    SyncError::Generic(format!("couldn't write to log file {}", e.to_string()))
                })?;
                log_file.flush().map_err(|e| {
                    SyncError::Generic(format!("couldn't flush log file {}", e.to_string()))
                })?;
            }

            log_entry.push(log_entry_line)
        }

        // Add entry at the beginning of the TUI log

        for entry_line in log_entry.into_iter().rev() {
            self.log_max_line_width = self.log_max_line_width.max(entry_line.len());
            self.log_entries.insert(0, entry_line);
        }

        // Adjust scroll position to maintain the user's view
        if self.log_v_scroll != 0 {
            self.log_v_scroll = self.log_v_scroll.saturating_add(lines.len());
        }

        if self.log_entries.len() > MAX_LOG_ENTRIES {
            self.log_entries.truncate(MAX_LOG_ENTRIES);

            let max_scroll = Self::max_scroll_down(&self.log_rect, self.log_entries.len());
            self.log_v_scroll = self.log_v_scroll.min(max_scroll);
        }

        Ok(())
    }

    pub fn scroll_up(&mut self) {
        match self.active_pane {
            ActivePane::StatePane => self.state_v_scroll = self.state_v_scroll.saturating_sub(1),
            ActivePane::LogPane => self.log_v_scroll = self.log_v_scroll.saturating_sub(1),
        }
    }

    pub fn scroll_down(&mut self) {
        match self.active_pane {
            ActivePane::StatePane => {
                let max = Self::max_scroll_down(&self.state_rect, self.state_lines.len());
                if self.state_v_scroll < max {
                    self.state_v_scroll += 1;
                }
            }
            ActivePane::LogPane => {
                let max = Self::max_scroll_down(&self.log_rect, self.log_entries.len());
                if self.log_v_scroll < max {
                    self.log_v_scroll += 1;
                }
            }
        }
    }

    pub fn scroll_left(&mut self) {
        match self.active_pane {
            ActivePane::StatePane => {
                self.state_h_scroll = self.state_h_scroll.saturating_sub(1);
            }
            ActivePane::LogPane => {
                self.log_h_scroll = self.log_h_scroll.saturating_sub(1);
            }
        }
    }

    fn max_scroll_right(rect: &Rect, max_line_width: usize) -> usize {
        let visible_width = rect.width.saturating_sub(4) as usize; // Subtract borders and padding
        max_line_width.saturating_sub(visible_width)
    }

    pub fn scroll_right(&mut self) {
        match self.active_pane {
            ActivePane::StatePane => {
                let max = Self::max_scroll_right(&self.state_rect, self.state_max_line_width);
                if self.state_h_scroll < max {
                    self.state_h_scroll += 1;
                }
            }
            ActivePane::LogPane => {
                let max = Self::max_scroll_right(&self.log_rect, self.log_max_line_width);
                if self.log_h_scroll < max {
                    self.log_h_scroll += 1;
                }
            }
        }
    }

    pub fn switch_pane(&mut self) {
        self.active_pane = match self.active_pane {
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

    pub fn render(&mut self, f: &mut Frame) {
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

        self.state_rect = main_chunks[0];
        self.log_rect = main_chunks[1];

        let state_list = Self::get_list(
            "Sync State",
            &self.state_lines,
            self.state_v_scroll,
            self.state_h_scroll,
            self.active_pane == ActivePane::StatePane,
        );
        f.render_widget(state_list, self.state_rect);

        let log_list = Self::get_list(
            "Log",
            &self.log_entries,
            self.log_v_scroll,
            self.log_h_scroll,
            self.active_pane == ActivePane::LogPane,
        );
        f.render_widget(log_list, self.log_rect);

        let help_text = "Press 'q' to quit, Tab to switch panes, ↑/↓ ←/→ to scroll active pane";
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

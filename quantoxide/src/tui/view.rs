use std::{fs::File, io::Write, sync::MutexGuard};

use chrono::Local;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};
use strum::IntoEnumIterator;

use super::error::{Result, TuiError};

pub trait TuiLogManager: Sync + Send + 'static {
    type State;

    fn get_max_tui_log_len(&self) -> usize;

    /// Returns mutable references to the log data components needed for TUI logging.
    ///
    /// # Returns
    ///
    /// A tuple containing:
    /// - `Option<&mut File>`: Optional log file handle for writing log entries to disk
    /// - `&mut Vec<String>`: Mutable reference to the log entries buffer
    /// - `&mut usize`: Mutable reference to the maximum line width tracker
    /// - `Rect`: The rectangular area where logs should be displayed in the TUI
    /// - `&mut usize`: Mutable reference to the vertical scroll position
    ///
    /// This method provides access to all the necessary components for managing
    /// log display and persistence in the terminal user interface.
    fn get_log_components_mut(
        state: &mut Self::State,
    ) -> (
        Option<&mut File>,
        &mut Vec<String>,
        &mut usize,
        Rect,
        &mut usize,
    );

    fn get_state(&self) -> MutexGuard<'_, Self::State>;

    fn max_scroll_down(rect: &Rect, entries_len: usize) -> usize {
        let visible_height = rect.height.saturating_sub(2) as usize; // Subtract borders
        entries_len.saturating_sub(visible_height)
    }

    fn add_log_entry(&self, entry: String) -> Result<()> {
        let mut state_guard = self.get_state();

        let max_tui_log_len = self.get_max_tui_log_len();
        let (mut log_file, log_entries, log_max_line_width, log_rect, log_v_scroll) =
            Self::get_log_components_mut(&mut state_guard);

        let timestamp = Local::now().format("%Y-%m-%dT%H:%M:%S%.3f%:z").to_string();

        let lines: Vec<&str> = entry.lines().collect();

        if lines.is_empty() {
            return Ok(());
        }

        let mut log_entry = Vec::new();

        for (i, line) in lines.iter().enumerate() {
            let log_entry_line = if i == 0 {
                format!("[{:<29}] {}", timestamp, line)
            } else {
                format!("{}{}", " ".repeat(32), line)
            };

            if let Some(log_file) = log_file.as_mut() {
                writeln!(log_file, "{}", log_entry_line).map_err(|e| {
                    TuiError::Generic(format!("couldn't write to log file {}", e.to_string()))
                })?;
                log_file.flush().map_err(|e| {
                    TuiError::Generic(format!("couldn't flush log file {}", e.to_string()))
                })?;
            }

            log_entry.push(log_entry_line)
        }

        // Add entry at the beginning of the TUI log

        for entry_line in log_entry.into_iter().rev() {
            *log_max_line_width = (*log_max_line_width).max(entry_line.len());
            log_entries.insert(0, entry_line);
        }

        // Adjust scroll position to maintain the user's view
        if *log_v_scroll != 0 {
            *log_v_scroll = log_v_scroll.saturating_add(lines.len());
        }

        if log_entries.len() > max_tui_log_len {
            log_entries.truncate(max_tui_log_len);

            let max_scroll = Self::max_scroll_down(&log_rect, log_entries.len());
            *log_v_scroll = (*log_v_scroll).min(max_scroll);
        }

        Ok(())
    }
}

pub trait TuiView: TuiLogManager {
    type UiMessage;

    type TuiPane: IntoEnumIterator;

    // type State;

    fn render(&self, f: &mut Frame);

    /// Handle a UI message and return whether shutdown was completed
    fn handle_ui_message(&self, message: Self::UiMessage) -> Result<bool>;

    fn max_scroll_right(rect: &Rect, max_line_width: usize) -> usize {
        let visible_width = rect.width.saturating_sub(4) as usize; // Subtract borders and padding
        max_line_width.saturating_sub(visible_width)
    }

    fn get_main_area(f: &mut Frame) -> Rect {
        let frame_rect = f.area();

        Rect {
            x: frame_rect.x,
            y: frame_rect.y,
            width: frame_rect.width,
            height: frame_rect.height.saturating_sub(1), // Leave 1 row for help text
        }
    }

    fn get_help_area(f: &mut Frame) -> Rect {
        let frame_rect = f.area();

        Rect {
            x: frame_rect.x,
            y: frame_rect.y + frame_rect.height.saturating_sub(1), // Last row
            width: frame_rect.width,
            height: 1,
        }
    }

    /// Returns the scroll data for the currently active pane.
    ///
    /// Returns a tuple containing:
    /// - `title`
    /// - `lines`
    /// - `vertical_scroll`: Current vertical scroll position
    /// - `horizontal_scroll`: Current horizontal scroll position
    /// - `rect`: Reference to the pane's display rectangle
    /// - `is_active`
    fn get_pane_render_info(
        state: &Self::State,
        pane: Self::TuiPane,
    ) -> (&'static str, &Vec<String>, usize, usize, Rect, bool);

    fn render_pane(f: &mut Frame, state: &Self::State, pane: Self::TuiPane) {
        let (title, lines, v_scroll, h_scroll, rect, is_active) =
            Self::get_pane_render_info(state, pane);

        let list_items: Vec<ListItem> = lines
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

        let list = List::new(list_items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style),
        );

        f.render_widget(list, rect);
    }

    fn render_panes(f: &mut Frame, state: &Self::State) {
        for pane in Self::TuiPane::iter() {
            Self::render_pane(f, state, pane);
        }

        let help_area = Self::get_help_area(f);

        let help_text = " Press 'q' to quit, Tab to switch panes, scroll with ↑/↓, ←/→, 'b' to bottom and 't' to top";
        let help_paragraph = Paragraph::new(help_text).style(Style::default().fg(Color::Gray));
        f.render_widget(help_paragraph, help_area);
    }

    /// Returns mutable references to the pane data components needed for updating content.
    ///
    /// # Returns
    ///
    /// A tuple containing:
    /// - `&mut Vec<String>`: Mutable reference to the pane's lines buffer
    /// - `&mut usize`: Mutable reference to the pane's maximum line width tracker
    /// - `&mut usize`: Mutable reference to the pane's vertical scroll position
    ///
    /// This method provides access to all the necessary components for updating
    /// pane content while maintaining scroll state appropriately.
    fn get_pane_data_mut(
        state: &mut Self::State,
        pane: Self::TuiPane,
    ) -> (&mut Vec<String>, &mut usize, &mut usize);

    fn update_pane_content(&self, pane: Self::TuiPane, content: String) {
        let mut state_guard = self.get_state();

        let mut new_lines: Vec<String> = content.lines().map(|line| line.to_string()).collect();
        new_lines.push("".to_string()); // Add empty line

        let max_line_width = new_lines.iter().map(|line| line.len()).max().unwrap_or(0);

        let (lines, max_width_ref, v_scroll) = Self::get_pane_data_mut(&mut state_guard, pane);

        *max_width_ref = max_line_width;

        // Only reset scroll if the content structure has significantly changed
        // or if current scroll position would be out of bounds
        if new_lines.len() != lines.len() {
            // If new content is shorter, adjust scroll to stay within bounds
            if *v_scroll >= new_lines.len() && new_lines.len() > 0 {
                *v_scroll = new_lines.len().saturating_sub(1);
            }
        }

        *lines = new_lines;
    }

    /// Returns the scroll data for the currently active pane.
    ///
    /// Returns a tuple containing:
    /// - `vertical_scroll`: Current vertical scroll position
    /// - `horizontal_scroll`: Current horizontal scroll position
    /// - `rect`: Reference to the pane's display rectangle
    /// - `total_lines`: Total number of lines in the pane
    /// - `max_line_width`: Maximum line width in the pane for horizontal scrolling
    fn get_active_scroll_data(state: &Self::State) -> (usize, usize, &Rect, usize, usize);

    /// Returns mutable references to the scroll positions for the currently active pane.
    ///
    /// Returns a tuple containing:
    /// - `vertical_scroll`: Mutable reference to the vertical scroll position
    /// - `horizontal_scroll`: Mutable reference to the horizontal scroll position
    ///
    /// This allows the scroll positions to be modified based on user input or other events.
    fn get_active_scroll_mut(state: &mut Self::State) -> (&mut usize, &mut usize);

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

    fn switch_pane(&self);
}

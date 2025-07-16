use std::sync::MutexGuard;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
};
use strum::IntoEnumIterator;

use super::Result;

pub trait TuiLogger: Sync + Send + 'static {
    fn add_log_entry(&self, entry: String) -> Result<()>;
}

pub trait TuiView: TuiLogger {
    type UiMessage;

    type TuiPane: IntoEnumIterator;

    type State;

    fn render(&self, f: &mut Frame);

    /// Handle a UI message and return whether shutdown was completed
    fn handle_ui_message(&self, message: Self::UiMessage) -> Result<bool>;

    fn max_scroll_down(rect: &Rect, entries_len: usize) -> usize {
        let visible_height = rect.height.saturating_sub(2) as usize; // Subtract borders
        entries_len.saturating_sub(visible_height)
    }

    fn max_scroll_right(rect: &Rect, max_line_width: usize) -> usize {
        let visible_width = rect.width.saturating_sub(4) as usize; // Subtract borders and padding
        max_line_width.saturating_sub(visible_width)
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

    fn render_pane(state: &Self::State, pane: Self::TuiPane, f: &mut Frame) {
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

    fn render_all_panes(state: &Self::State, f: &mut Frame) {
        for pane in Self::TuiPane::iter() {
            Self::render_pane(state, pane, f);
        }
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

    fn get_state(&self) -> MutexGuard<'_, Self::State>;

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

use std::sync::MutexGuard;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
};

use super::Result;

pub trait TuiLogger: Sync + Send + 'static {
    fn add_log_entry(&self, entry: String) -> Result<()>;
}

pub trait TuiView: TuiLogger {
    type UiMessage;
    type State;

    fn render(&self, f: &mut Frame);

    /// Handle a UI message and return whether shutdown was completed
    fn handle_ui_message(&self, message: Self::UiMessage) -> Result<bool>;

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

    fn max_scroll_down(rect: &Rect, entries_len: usize) -> usize {
        let visible_height = rect.height.saturating_sub(2) as usize; // Subtract borders
        entries_len.saturating_sub(visible_height)
    }

    fn max_scroll_right(rect: &Rect, max_line_width: usize) -> usize {
        let visible_width = rect.width.saturating_sub(4) as usize; // Subtract borders and padding
        max_line_width.saturating_sub(visible_width)
    }

    fn get_active_scroll_data(state: &Self::State) -> (usize, usize, &Rect, usize, usize);

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

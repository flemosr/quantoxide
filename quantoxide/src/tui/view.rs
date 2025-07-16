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

    fn scroll_up(&self);

    fn scroll_down(&self);

    fn scroll_left(&self);

    fn scroll_right(&self);

    fn reset_scroll(&self);

    fn scroll_to_bottom(&self);

    fn switch_pane(&self);
}

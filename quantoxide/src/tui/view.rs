use ratatui::Frame;

use super::Result;

pub trait TuiLogger: Sync + Send + 'static {
    fn add_log_entry(&self, entry: String) -> Result<()>;
}

pub trait TuiView: TuiLogger {
    type UiMessage;

    fn render(&self, f: &mut Frame);

    /// Handle a UI message and return whether shutdown was completed
    fn handle_ui_message(&self, message: Self::UiMessage) -> Result<bool>;

    fn scroll_up(&self);

    fn scroll_down(&self);

    fn scroll_left(&self);

    fn scroll_right(&self);

    fn reset_scroll(&self);

    fn scroll_to_bottom(&self);

    fn switch_pane(&self);
}

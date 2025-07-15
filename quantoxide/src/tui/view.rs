use ratatui::Frame;

use super::Result;

pub trait TuiLogger: Sync + Send + 'static {
    fn add_log_entry(&self, entry: String) -> Result<()>;
}

pub trait TuiViewRenderer: Sync + Send + 'static {
    fn render(&self, f: &mut Frame);
}

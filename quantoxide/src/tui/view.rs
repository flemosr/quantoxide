use ratatui::Frame;

pub trait TuiViewRenderer: Sync + Send + 'static {
    fn render(&self, f: &mut Frame);
}

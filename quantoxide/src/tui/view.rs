use std::{
    fs::File,
    io::Write,
    sync::{Arc, Mutex, MutexGuard},
};

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use crate::tui::{Result, TuiError as LiveTuiError};

pub trait TuiViewRenderer: Sync + Send + 'static {
    fn render(&self, f: &mut Frame);
}

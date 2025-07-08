use std::{
    io::{self, Stdout},
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};
use tokio::{sync::mpsc, time};

use crate::{
    sync::{SyncEngine, SyncError, error::Result},
    util::AbortOnDropHandle,
};

const MAX_LOG_ENTRIES: usize = 10_000;

#[derive(Debug)]
enum UiMessage {
    LogEntry(String),
    StateUpdate(String),
    ShutdownConfirmation,
}

#[derive(Debug, PartialEq)]
enum ActivePane {
    StatePane,
    LogPane,
}

#[derive(Clone)]
struct SyncTuiTerminal(Arc<Mutex<TerminalState>>);

struct TerminalState {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    restored: bool,
}

impl SyncTuiTerminal {
    pub fn new() -> Result<Self> {
        enable_raw_mode().map_err(|e| SyncError::Generic(e.to_string()))?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
            .map_err(|e| SyncError::Generic(e.to_string()))?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend).map_err(|e| SyncError::Generic(e.to_string()))?;

        Ok(Self(Arc::new(Mutex::new(TerminalState {
            terminal,
            restored: false,
        }))))
    }

    fn get_state(&self) -> MutexGuard<'_, TerminalState> {
        self.0.lock().expect("not poisoned")
    }

    pub fn draw(&self, tui_content: &mut SyncTuiContent) -> Result<()> {
        let mut state = self.get_state();
        if state.restored {
            return Err(SyncError::Generic("Terminal already restored".to_string()));
        }

        state
            .terminal
            .draw(|f| tui_content.render(f))
            .map_err(|e| SyncError::Generic(e.to_string()))?;

        Ok(())
    }

    pub fn restore(&self) -> Result<()> {
        let mut state = self.get_state();
        if state.restored {
            return Ok(());
        }

        disable_raw_mode().map_err(|e| SyncError::Generic(e.to_string()))?;
        execute!(
            state.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )
        .map_err(|e| SyncError::Generic(e.to_string()))?;

        state
            .terminal
            .show_cursor()
            .map_err(|e| SyncError::Generic(e.to_string()))?;

        state.restored = true;

        Ok(())
    }
}

impl Drop for SyncTuiTerminal {
    fn drop(&mut self) {
        if let Err(e) = self.restore() {
            eprintln!("Failed to restore terminal on Drop: {:?}", e);
        }
    }
}

struct SyncTuiContent {
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
    fn new() -> Self {
        Self {
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

    fn update_state(&mut self, state: String) {
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

    fn add_log_entry(&mut self, entry: String) {
        let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();

        let lines: Vec<&str> = entry.lines().collect();

        if lines.is_empty() {
            return;
        }

        let mut log_entry = Vec::new();

        for (i, line) in lines.iter().enumerate() {
            let log_entry_line = if i == 0 {
                format!("[{}] {}", timestamp, line)
            } else {
                format!("           {}", line)
            };

            // TODO: write log entry to file

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
    }

    fn scroll_up(&mut self) {
        match self.active_pane {
            ActivePane::StatePane => self.state_v_scroll = self.state_v_scroll.saturating_sub(1),
            ActivePane::LogPane => self.log_v_scroll = self.log_v_scroll.saturating_sub(1),
        }
    }

    fn scroll_down(&mut self) {
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

    fn scroll_left(&mut self) {
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

    fn scroll_right(&mut self) {
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

    fn switch_pane(&mut self) {
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

    fn render(&mut self, f: &mut Frame) {
        let frame_rect = f.area();

        let main_area = Rect {
            x: frame_rect.x,
            y: frame_rect.y,
            width: frame_rect.width,
            height: frame_rect.height.saturating_sub(1), // Leave 1 row for help text
        };

        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
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

#[derive(Debug, PartialEq)]
pub enum SyncTuiStatusNotRunning {
    Crashed(SyncError),
    ShutdownInitiated,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SyncTuiStatus {
    NotRunning(Arc<SyncTuiStatusNotRunning>),
    Running,
}

impl From<SyncTuiStatusNotRunning> for SyncTuiStatus {
    fn from(value: SyncTuiStatusNotRunning) -> Self {
        Self::NotRunning(Arc::new(value))
    }
}

struct SyncTuiStatusManager(Mutex<SyncTuiStatus>);

impl SyncTuiStatusManager {
    fn new_running() -> Arc<Self> {
        Arc::new(Self(Mutex::new(SyncTuiStatus::Running)))
    }

    fn status(&self) -> SyncTuiStatus {
        self.0.lock().expect("not poisoned").clone()
    }

    fn set(&self, new_status: SyncTuiStatus) {
        let mut curr = self.0.lock().expect("not poisoned");
        *curr = new_status
    }

    fn set_crashed(&self, error: SyncError) {
        self.set(SyncTuiStatusNotRunning::Crashed(error).into());
    }

    fn set_shutdown_initiated(&self) {
        self.set(SyncTuiStatusNotRunning::ShutdownInitiated.into());
    }

    fn set_shutdown(&self) {
        self.set(SyncTuiStatusNotRunning::Shutdown.into());
    }
}

pub struct SyncTui {
    status_manager: Arc<SyncTuiStatusManager>,
    // Retain ownership to ensure `SyncTuiTerminal` destructor is executed when
    // `SyncTui` is dropped.
    _sync_tui_terminal: SyncTuiTerminal,
    ui_tx: mpsc::Sender<UiMessage>,
    // Explicitly aborted on drop, to ensure the terminal is restored before
    // `SyncTui`'s drop is completed.
    ui_task_handle: Arc<Mutex<Option<AbortOnDropHandle<Result<()>>>>>,
    _shutdown_listener_handle: AbortOnDropHandle<()>,
}

impl SyncTui {
    async fn run_ui(
        terminal: SyncTuiTerminal,
        mut ui_rx: mpsc::Receiver<UiMessage>,
        shutdown_tx: mpsc::Sender<()>,
    ) -> Result<()> {
        let mut tui_content = SyncTuiContent::new();

        loop {
            tokio::task::yield_now().await;
            terminal.draw(&mut tui_content)?;

            if let Ok(message) = ui_rx.try_recv() {
                match message {
                    UiMessage::LogEntry(entry) => {
                        tui_content.add_log_entry(entry);
                    }
                    UiMessage::StateUpdate(state) => {
                        tui_content.update_state(state);
                    }
                    UiMessage::ShutdownConfirmation => {
                        tui_content.add_log_entry("Shutdown completed".to_string());
                        terminal.draw(&mut tui_content)?;
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        return Ok(());
                    }
                }
            }

            if event::poll(Duration::from_millis(50))
                .map_err(|e| SyncError::Generic(e.to_string()))?
            {
                if let Event::Key(key) =
                    event::read().map_err(|e| SyncError::Generic(e.to_string()))?
                {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') => {
                            tui_content.add_log_entry("'q' pressed".to_string());
                            if let Err(e) = shutdown_tx.send(()).await {
                                tui_content.add_log_entry(format!(
                                    "Error: Failed to send shutdown signal, {:?}",
                                    e
                                ));
                            }
                            break;
                        }
                        KeyCode::Up => tui_content.scroll_up(),
                        KeyCode::Down => tui_content.scroll_down(),
                        KeyCode::Left => tui_content.scroll_left(),
                        KeyCode::Right => tui_content.scroll_right(),
                        KeyCode::Tab => tui_content.switch_pane(),
                        _ => {}
                    }
                }
            }
        }

        terminal.draw(&mut tui_content)?;

        while let Some(message) = ui_rx.recv().await {
            match message {
                UiMessage::LogEntry(entry) => {
                    tui_content.add_log_entry(entry);
                    terminal.draw(&mut tui_content)?;
                }
                UiMessage::StateUpdate(state) => {
                    tui_content.update_state(state);
                    terminal.draw(&mut tui_content)?;
                }
                UiMessage::ShutdownConfirmation => {
                    tui_content.add_log_entry("Shutdown completed.".to_string());
                    terminal.draw(&mut tui_content)?;
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    return Ok(());
                }
            }
        }

        Ok(())
    }

    fn spawn_ui_task(
        sync_tui_terminal: SyncTuiTerminal,
        ui_rx: mpsc::Receiver<UiMessage>,
        shutdown_tx: mpsc::Sender<()>,
    ) -> Arc<Mutex<Option<AbortOnDropHandle<Result<()>>>>> {
        Arc::new(Mutex::new(Some(
            tokio::spawn(Self::run_ui(sync_tui_terminal, ui_rx, shutdown_tx)).into(),
        )))
    }

    // TODO: review error handling
    async fn shutdown_inner(
        ui_task_handle: Arc<Mutex<Option<AbortOnDropHandle<Result<()>>>>>,
        ui_tx: mpsc::Sender<UiMessage>,
    ) -> Result<()> {
        let Some(mut handle) = ui_task_handle
            .lock()
            .expect("`ui_task_handle` mutex can't be poisoned")
            .take()
        else {
            return Err(SyncError::Generic(
                "Sync TUI was already shutdown".to_string(),
            ));
        };

        let log_res_a = ui_tx
            .send(UiMessage::LogEntry("Shutdown initiated...".to_string()))
            .await
            .map_err(|e| SyncError::Generic(e.to_string()));

        // TODO: Additional shutdown logic

        time::sleep(Duration::from_secs(5)).await;

        let log_res_b = ui_tx
            .send(UiMessage::ShutdownConfirmation)
            .await
            .map_err(|e| {
                handle.abort();
                SyncError::Generic(format!("Failed to send shutdown confirmation, {e}"))
            });

        log_res_a.and(log_res_b)?;

        tokio::select! {
            join_res = &mut handle => {
                join_res.map_err(SyncError::TaskJoin)?
            }
            _ = time::sleep(Duration::from_secs(5)) => {
                handle.abort();
                Err(SyncError::Generic("Shutdown timeout".to_string()))
            }
        }
    }

    fn spawn_shutdown_signal_listener(
        mut shutdown_rx: mpsc::Receiver<()>,
        ui_task_handle: Arc<Mutex<Option<AbortOnDropHandle<Result<()>>>>>,
        ui_tx: mpsc::Sender<UiMessage>,
    ) -> AbortOnDropHandle<()> {
        tokio::spawn(async move {
            if let Some(_) = shutdown_rx.recv().await {
                if let Err(e) = Self::shutdown_inner(ui_task_handle, ui_tx.clone()).await {
                    eprintln!("shutdown_inner failed {:?}", e);
                }
            }
        })
        .into()
    }

    pub async fn launch() -> Result<Self> {
        let (ui_tx, ui_rx) = mpsc::channel::<UiMessage>(100);
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(10);

        let sync_tui_terminal = SyncTuiTerminal::new()?;

        let ui_task_handle = Self::spawn_ui_task(sync_tui_terminal.clone(), ui_rx, shutdown_tx);

        let _shutdown_listener_handle = Self::spawn_shutdown_signal_listener(
            shutdown_rx,
            ui_task_handle.clone(),
            ui_tx.clone(),
        );

        Ok(Self {
            _sync_tui_terminal: sync_tui_terminal,
            ui_tx,
            ui_task_handle,
            _shutdown_listener_handle,
        })
    }

    pub async fn log(&self, log_entry: impl Into<String>) -> Result<()> {
        self.ui_tx
            .send(UiMessage::LogEntry(log_entry.into()))
            .await
            .map_err(|_| SyncError::Generic("TUI was already shutdown".to_string()))
    }

    pub fn couple(engine: SyncEngine) {
        todo!()
    }

    pub async fn shutdown(&self) -> Result<()> {
        Self::shutdown_inner(self.ui_task_handle.clone(), self.ui_tx.clone()).await
    }
}

impl Drop for SyncTui {
    fn drop(&mut self) {
        if let Some(ui_handle) = self
            .ui_task_handle
            .lock()
            .expect("`ui_task_handle` mutex can't be poisoned")
            .take()
        {
            ui_handle.abort();
        };
    }
}

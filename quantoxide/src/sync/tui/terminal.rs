use std::{
    io::{self, Stdout},
    sync::{Arc, Mutex, MutexGuard},
};

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use super::{
    super::{SyncError, error::Result},
    content::SyncTuiContent,
};

struct TerminalState {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    restored: bool,
}

pub struct SyncTuiTerminal(Mutex<TerminalState>);

impl SyncTuiTerminal {
    pub fn new() -> Result<Arc<Self>> {
        enable_raw_mode().map_err(|e| SyncError::Generic(e.to_string()))?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
            .map_err(|e| SyncError::Generic(e.to_string()))?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend).map_err(|e| SyncError::Generic(e.to_string()))?;

        Ok(Arc::new(Self(Mutex::new(TerminalState {
            terminal,
            restored: false,
        }))))
    }

    fn get_state(&self) -> MutexGuard<'_, TerminalState> {
        self.0.lock().expect("not poisoned")
    }

    pub fn draw(&self, tui_content: &SyncTuiContent) -> Result<()> {
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

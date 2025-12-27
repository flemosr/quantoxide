use std::{
    io::{self, Stdout},
    sync::{Arc, Mutex, MutexGuard},
};

use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    crossterm::{
        event::{DisableMouseCapture, EnableMouseCapture},
        execute,
        terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
    },
};

use super::{
    error::{Result, TuiError},
    view::TuiView,
};

struct TerminalState {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    restored: bool,
}

pub(super) struct TuiTerminal(Mutex<TerminalState>);

impl TuiTerminal {
    pub fn new() -> Result<Arc<Self>> {
        enable_raw_mode().map_err(TuiError::TerminalSetup)?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
            .map_err(TuiError::TerminalSetup)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend).map_err(TuiError::TerminalSetup)?;

        Ok(Arc::new(Self(Mutex::new(TerminalState {
            terminal,
            restored: false,
        }))))
    }

    fn get_state(&self) -> MutexGuard<'_, TerminalState> {
        self.0.lock().expect("not poisoned")
    }

    pub fn draw<T: TuiView>(&self, tui_view: &T) -> Result<()> {
        let mut state = self.get_state();
        if state.restored {
            return Err(TuiError::DrawTerminalAlreadyRestored);
        }

        state
            .terminal
            .draw(|f| tui_view.render(f))
            .map_err(TuiError::DrawFailed)?;

        Ok(())
    }

    pub fn restore(&self) -> Result<()> {
        let mut state = self.get_state();
        if state.restored {
            return Ok(());
        }

        disable_raw_mode().map_err(TuiError::TerminalRestore)?;
        execute!(
            state.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )
        .map_err(TuiError::TerminalRestore)?;

        state
            .terminal
            .show_cursor()
            .map_err(TuiError::TerminalRestore)?;

        state.restored = true;

        Ok(())
    }
}

impl Drop for TuiTerminal {
    fn drop(&mut self) {
        if let Err(e) = self.restore() {
            eprintln!("Failed to restore `TuiTerminal` on Drop: {:?}", e);
        }
    }
}

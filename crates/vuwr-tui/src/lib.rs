//! ratatui terminal frontend for vuwr. Native only — ratatui does not run in
//! a browser.

mod app;
pub mod keymap;
pub mod palette;
pub mod ui;

pub use app::App;
pub use vuwr_core::ViewMode;

use std::io;
use std::path::PathBuf;

use crossterm::ExecutableCommand;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::prelude::*;
use vuwr_core::Document;

/// Run the interactive table UI until the user quits. The terminal is always
/// restored, even on error.
/// Run the terminal UI.
///
/// Returns anything the session asked to hand back to the shell — the
/// marked rows from `Ctrl-E`, say. It is printed by the caller after the
/// alternate screen is gone, so it lands in the scrollback and can be
/// piped, which is the whole point of it.
pub fn run(path: PathBuf, doc: Document) -> io::Result<Option<String>> {
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let terminal = Terminal::new(CrosstermBackend::new(io::stdout()));
    let mut terminal = match terminal {
        Ok(t) => t,
        Err(e) => {
            disable_raw_mode().ok();
            io::stdout().execute(LeaveAlternateScreen).ok();
            return Err(e);
        }
    };
    let mut app = App::new(path, doc);
    let result = event_loop(&mut terminal, &mut app);
    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;
    result.map(|()| app.take_output())
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> io::Result<()> {
    while !app.quit {
        terminal.draw(|f| ui::render(f, app))?;
        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.handle_key(key);
        }
    }
    Ok(())
}

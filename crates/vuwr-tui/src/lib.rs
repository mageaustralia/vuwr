//! ratatui terminal frontend for vuwr. Native only — ratatui does not run in
//! a browser.

mod app;
pub mod ui;

pub use app::{App, ViewMode};

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
pub fn run(path: PathBuf, doc: Document) -> io::Result<()> {
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let result = event_loop(&mut terminal, App::new(path, doc));
    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;
    result
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    mut app: App,
) -> io::Result<()> {
    while !app.quit {
        terminal.draw(|f| ui::render(f, &mut app))?;
        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.handle_key(key);
        }
    }
    Ok(())
}

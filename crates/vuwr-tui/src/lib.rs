//! ratatui terminal frontend for vuwr. Native only — ratatui does not run in
//! a browser.

mod app;
pub mod detect;
pub mod keymap;
pub mod palette;
pub mod ui;

pub use app::App;
pub use vuwr_core::ViewMode;

use std::io::{self, IsTerminal};
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
    // The document may have arrived on stdin — `cat feed.xml | vuwr` —
    // which leaves file descriptor 0 an exhausted pipe. There is then
    // nowhere to read a keystroke from, and nowhere for the terminal's
    // answer about its own colour to arrive, so the answer was left in
    // the terminal's buffer and the shell read it as a command after we
    // exited.
    //
    // So point stdin at the terminal, which is what a pager does. After
    // this everything downstream — crossterm's reader included — sees the
    // ordinary case of stdin being a terminal.
    #[cfg(unix)]
    attach_stdin_to_terminal();
    enable_raw_mode()?;
    // Before the alternate screen, while the terminal is still showing
    // its own background — and after raw mode, or the reply is line
    // buffered and never arrives.
    palette::detect_ground();
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

/// Point stdin at the terminal, if it is not already.
///
/// Duplicated from stdout or stderr rather than opened from `/dev/tty`,
/// which matters more than it looks. On macOS a descriptor obtained by
/// opening `/dev/tty` cannot be registered with kqueue, so the event
/// reader that crossterm builds on it fails outright — which is exactly
/// what crossterm does for itself when stdin is not a terminal, and
/// exactly the error a piped document produced. A duplicate of a
/// descriptor the terminal already handed us has no such problem.
///
/// Best effort: with nothing to duplicate this leaves matters as they
/// were, and the failure is the one that existed before rather than a
/// panic.
#[cfg(unix)]
fn attach_stdin_to_terminal() {
    use std::os::fd::AsRawFd;

    if io::stdin().is_terminal() {
        return;
    }
    let from = if io::stdout().is_terminal() {
        io::stdout().as_raw_fd()
    } else if io::stderr().is_terminal() {
        io::stderr().as_raw_fd()
    } else {
        return;
    };
    // SAFETY: `dup2` between two descriptors this process owns, both open
    // for the duration of the call. The workspace warns on `unsafe` so
    // each block has to say why it is here: std cannot replace one of the
    // standard descriptors.
    #[allow(unsafe_code)]
    unsafe {
        libc::dup2(from, libc::STDIN_FILENO);
    }
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

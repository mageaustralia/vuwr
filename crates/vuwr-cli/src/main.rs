use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use vuwr_core::{Document, FormatHint};

/// vuwr — a fast, editable viewer for CSV / JSON / XML
#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// File to open. Omit, or pass `-`, to read from standard input.
    file: Option<PathBuf>,
    /// Force the terminal UI
    #[arg(long)]
    tui: bool,
    /// Force the graphical UI
    #[arg(long)]
    gui: bool,
}

fn main() -> ExitCode {
    let args = Args::parse();

    // `-` and a bare `vuwr` with something piped in both mean stdin.
    let from_stdin = match &args.file {
        Some(p) => p.as_os_str() == "-",
        None => !std::io::stdin().is_terminal(),
    };

    if args.file.is_none() && !from_stdin {
        eprintln!("vuwr: no file given (and nothing piped in)");
        return ExitCode::from(2);
    }

    let (bytes, label) = if from_stdin {
        let mut buf = Vec::new();
        if let Err(e) = std::io::stdin().read_to_end(&mut buf) {
            eprintln!("vuwr: reading standard input: {e}");
            return ExitCode::FAILURE;
        }
        (buf, PathBuf::from("-"))
    } else {
        let path = args.file.clone().expect("checked above");
        match std::fs::read(&path) {
            Ok(bytes) => (bytes, path),
            Err(e) => {
                eprintln!("vuwr: {}: {e}", path.display());
                return ExitCode::FAILURE;
            }
        }
    };

    // An explicit extension is trusted over content sniffing, so a .csv
    // whose first cell starts with `{` is still parsed as CSV. Piped input
    // has no name, so it is always sniffed.
    let hint = if from_stdin {
        FormatHint::Auto
    } else {
        match label.extension().and_then(|e| e.to_str()) {
            Some("csv") => FormatHint::Csv,
            Some("tsv") => FormatHint::Tsv,
            Some("json") => FormatHint::Json,
            Some("xml") => FormatHint::Xml,
            _ => FormatHint::Auto,
        }
    };

    let doc = match Document::parse(&bytes, hint) {
        Ok(doc) => doc,
        Err(e) => {
            eprintln!("vuwr: {}: {e}", label.display());
            return ExitCode::FAILURE;
        }
    };

    // Nothing interactive can happen without a terminal to draw on, so
    // behave like a pager does when piped: write the document through and
    // exit. This used to try to launch the GUI, which made
    // `vuwr f.csv | head` fail for no good reason.
    if !args.gui && !std::io::stdout().is_terminal() {
        let mut out = std::io::stdout().lock();
        if let Err(e) = out.write_all(&doc.serialize()) {
            // A closed pipe (`| head`) is the normal way this ends.
            if e.kind() != std::io::ErrorKind::BrokenPipe {
                eprintln!("vuwr: {e}");
                return ExitCode::FAILURE;
            }
        }
        return ExitCode::SUCCESS;
    }

    if args.gui {
        eprintln!("vuwr: the GUI is not implemented yet (phase 6) — use --tui");
        return ExitCode::from(2);
    }

    // Editing piped input has nowhere to write back to; the TUI shows the
    // path as `-` and `:w` will report the failure.
    match vuwr_tui::run(label, doc) {
        Ok(output) => {
            // Printed after the alternate screen is gone, so it lands in
            // the scrollback and can be piped.
            if let Some(text) = output {
                print!("{text}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("vuwr: {e}");
            ExitCode::FAILURE
        }
    }
}

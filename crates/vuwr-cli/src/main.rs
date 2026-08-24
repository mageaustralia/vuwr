use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use vuwr_core::{Document, FormatHint};

/// vuwr — a fast, editable viewer for CSV / JSON / XML
#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// File to open
    file: PathBuf,
    /// Force the terminal UI
    #[arg(long)]
    tui: bool,
    /// Force the graphical UI
    #[arg(long)]
    gui: bool,
}

fn main() -> ExitCode {
    let args = Args::parse();

    let bytes = match std::fs::read(&args.file) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("vuwr: {}: {e}", args.file.display());
            return ExitCode::FAILURE;
        }
    };
    // An explicit extension is trusted over content sniffing, so a .csv
    // whose first cell starts with `{` is still parsed as CSV.
    let hint = match args.file.extension().and_then(|e| e.to_str()) {
        Some("csv") => FormatHint::Csv,
        Some("tsv") => FormatHint::Tsv,
        Some("json") => FormatHint::Json,
        Some("xml") => FormatHint::Xml,
        _ => FormatHint::Auto,
    };
    let doc = match Document::parse(&bytes, hint) {
        Ok(doc) => doc,
        Err(e) => {
            eprintln!("vuwr: {}: {e}", args.file.display());
            return ExitCode::FAILURE;
        }
    };

    // TTY → TUI, otherwise GUI. `--tui`/`--gui` override.
    let want_gui = args.gui || (!args.tui && !std::io::stdout().is_terminal());
    if want_gui {
        eprintln!("vuwr: the GUI is not implemented yet (phase 6) — use --tui");
        return ExitCode::from(2);
    }
    match vuwr_tui::run(args.file, doc) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("vuwr: {e}");
            ExitCode::FAILURE
        }
    }
}

use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use vuwr_core::{Document, FormatHint};

/// vuwr — a fast, editable viewer for CSV / JSON / XML
#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// Files to open. Omit, or pass `-`, to read from standard input.
    /// More than one is only meaningful with --check.
    files: Vec<PathBuf>,
    /// Validate and exit: nothing is displayed, and the exit status says
    /// whether the input parsed. Replaces `jq empty` and
    /// `xmllint --noout`, and covers CSV too.
    #[arg(long)]
    check: bool,
    /// With --check, print nothing and rely on the exit status alone.
    #[arg(short, long)]
    quiet: bool,
    /// Print the licence notices bundled with this binary and exit.
    #[arg(long)]
    licenses: bool,
    /// Force the terminal UI
    #[arg(long)]
    tui: bool,
    /// Force the graphical UI
    #[arg(long)]
    gui: bool,
}

/// Format hint from a file extension. Piped input has no name, so it is
/// always sniffed.
fn hint_for(path: &std::path::Path) -> FormatHint {
    match path.extension().and_then(|e| e.to_str()) {
        Some("csv") => FormatHint::Csv,
        Some("tsv") => FormatHint::Tsv,
        Some("json") => FormatHint::Json,
        Some("xml") => FormatHint::Xml,
        _ => FormatHint::Auto,
    }
}

/// `--check`: parse each input and report where it fails.
///
/// Exit status is 0 when everything parsed, 1 when something did not, and
/// 2 when a file could not be read — so a missing file is distinguishable
/// from an invalid one in a script.
fn check(args: &Args) -> ExitCode {
    let mut worst = 0u8;

    let inputs: Vec<PathBuf> = if args.files.is_empty() {
        vec![PathBuf::from("-")]
    } else {
        args.files.clone()
    };

    for path in inputs {
        let stdin = path.as_os_str() == "-";
        let bytes = if stdin {
            let mut buf = Vec::new();
            match std::io::stdin().read_to_end(&mut buf) {
                Ok(_) => buf,
                Err(e) => {
                    if !args.quiet {
                        eprintln!("vuwr: reading standard input: {e}");
                    }
                    worst = worst.max(2);
                    continue;
                }
            }
        } else {
            match std::fs::read(&path) {
                Ok(b) => b,
                Err(e) => {
                    if !args.quiet {
                        eprintln!("vuwr: {}: {e}", path.display());
                    }
                    worst = worst.max(2);
                    continue;
                }
            }
        };

        let hint = if stdin {
            FormatHint::Auto
        } else {
            hint_for(&path)
        };
        match Document::parse(&bytes, hint) {
            Ok(_) => {}
            Err(e) => {
                if !args.quiet {
                    eprintln!("{}:{}", path.display(), e.located(&bytes));
                }
                worst = worst.max(1);
            }
        }
    }

    ExitCode::from(worst)
}

fn main() -> ExitCode {
    let args = Args::parse();

    // The bundled fonts are under licences requiring their notices to be
    // distributed with the software; the GUI shows them under Help, and
    // this is the same thing for anyone who never opens a window.
    if args.licenses {
        println!("vuwr is MIT OR Apache-2.0.\n");
        println!(
            "Built on egui and eframe (MIT OR Apache-2.0), ratatui (MIT), regex,\n\
             serde and clap (MIT OR Apache-2.0), and others — all permissive.\n"
        );
        println!(
            "The fonts below are bundled by egui and carry licences that require\n\
             these notices to be distributed with the software.\n"
        );
        for (title, text) in vuwr_gui::LICENSE_NOTICES {
            println!("{}\n{}\n{text}\n", title, "-".repeat(title.len()));
        }
        return ExitCode::SUCCESS;
    }

    if args.check {
        return check(&args);
    }

    if args.files.len() > 1 {
        eprintln!("vuwr: only one file can be opened at a time (use --check for many)");
        return ExitCode::from(2);
    }
    let file = args.files.first().cloned();

    // `-` and a bare `vuwr` with something piped in both mean stdin.
    let from_stdin = match &file {
        Some(p) => p.as_os_str() == "-",
        None => !std::io::stdin().is_terminal(),
    };

    if file.is_none() && !from_stdin {
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
        let path = file.clone().expect("checked above");
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
        hint_for(&label)
    };

    let doc = match Document::parse(&bytes, hint) {
        Ok(doc) => doc,
        Err(e) => {
            eprintln!("vuwr: {}:{}", label.display(), e.located(&bytes));
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
        // Piped input has nowhere to write back to, so the GUI is told
        // there is no path rather than being handed `-`.
        let gui_path = if from_stdin { None } else { Some(label) };
        return match vuwr_gui::run(gui_path, doc) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("vuwr: {e}");
                ExitCode::FAILURE
            }
        };
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

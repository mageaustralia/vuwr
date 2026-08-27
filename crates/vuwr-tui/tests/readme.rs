//! The README's key table is a promise. This checks it.
//!
//! Every key it names must resolve to a command, and every `:` name must
//! be one the palette accepts. A table that drifts from the keymap is
//! worse than no table: it is instructions that do not work, in the first
//! place anybody looks.
//!
//! Read from the file rather than copied here, so the two cannot part
//! company without this noticing.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use vuwr_core::Command;
use vuwr_tui::keymap::{Resolved, resolve};

/// Every `code`-quoted token in the README's key table.
fn advertised() -> Vec<String> {
    let readme = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../README.md"))
        .expect("the README is beside the crates");
    let mut keys = Vec::new();
    let mut in_table = false;
    for line in readme.lines() {
        // The key table is the one whose header is empty: `| | |`.
        if line.trim() == "| | |" {
            in_table = true;
            continue;
        }
        if in_table {
            if !line.starts_with('|') {
                break;
            }
            let Some(first) = line.split('|').nth(1) else {
                continue;
            };
            let mut rest = first;
            while let Some(open) = rest.find('`') {
                let after = &rest[open + 1..];
                let Some(close) = after.find('`') else { break };
                let token = &after[..close];
                // `h j k l` is one cell holding four keys.
                keys.extend(token.split_whitespace().map(str::to_string));
                rest = &after[close + 1..];
            }
        }
    }
    keys
}

/// The key event a README token stands for, if it is a key at all.
fn as_key(token: &str) -> Option<KeyEvent> {
    let chars: Vec<char> = token.chars().collect();
    match token {
        "Space" => Some(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)),
        "Enter" => Some(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        "Esc" => Some(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        "Tab" => Some(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
        _ if token.starts_with("Ctrl-") => {
            let c = token.strip_prefix("Ctrl-")?.chars().next()?;
            Some(KeyEvent::new(
                KeyCode::Char(c.to_ascii_lowercase()),
                KeyModifiers::CONTROL,
            ))
        }
        // A single character: `i`, `&`, `<`. Shift is implied by the case.
        _ if chars.len() == 1 => Some(KeyEvent::new(KeyCode::Char(chars[0]), KeyModifiers::NONE)),
        _ => None,
    }
}

#[test]
fn every_key_the_readme_names_does_something() {
    let tokens = advertised();
    if std::env::var("SHOW_KEYS").is_ok() {
        eprintln!("read {} keys: {tokens:?}", tokens.len());
    }
    assert!(
        tokens.len() > 20,
        "only found {} keys — the table was not read",
        tokens.len()
    );

    let mut broken = Vec::new();
    for token in &tokens {
        if let Some(name) = token.strip_prefix(':') {
            // A `:` command has to be one the palette resolves.
            if Command::from_name(name).is_none() {
                broken.push(format!("`:{name}` is not a command"));
            }
            continue;
        }
        let Some(key) = as_key(token) else {
            broken.push(format!("`{token}` is not a key this test understands"));
            continue;
        };
        // `g` is the first half of `gg`, so a pending prefix counts.
        let resolved = resolve(key, false);
        if matches!(resolved, Resolved::None) && matches!(resolve(key, true), Resolved::None) {
            broken.push(format!("`{token}` does nothing"));
        }
    }
    assert!(broken.is_empty(), "\n{}", broken.join("\n"));
}

/// And the keys it names do what it says they do.
///
/// Spot-checked against the description in the same row, since the table
/// is prose and cannot be matched mechanically.
#[test]
fn the_keys_do_what_the_readme_says() {
    let cases: [(&str, Command); 14] = [
        ("1", Command::ViewTable),
        ("2", Command::ViewTree),
        ("3", Command::ViewText),
        ("i", Command::EditCell),
        ("c", Command::ReplaceCell),
        ("u", Command::Undo),
        ("/", Command::Find),
        ("n", Command::FindNext),
        ("N", Command::FindPrev),
        ("&", Command::Filter),
        ("r", Command::ClearFilter),
        ("%", Command::Substitute),
        ("m", Command::ToggleMark),
        ("f", Command::FreezeColumns),
    ];
    let mut wrong = Vec::new();
    for (token, expected) in cases {
        let key = as_key(token).expect("a key");
        match resolve(key, false) {
            Resolved::Run(got) if got == expected => {}
            Resolved::Run(got) => wrong.push(format!("`{token}` runs {got:?}, not {expected:?}")),
            _ => wrong.push(format!("`{token}` does not run a command at all")),
        }
    }
    assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));
}

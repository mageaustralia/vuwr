//! CLI behaviour. Runs the real binary, since the point of these is how it
//! behaves at the process boundary: pipes, exit codes, stdin.

use std::io::Write;
use std::process::{Command, Stdio};

fn vuwr() -> Command {
    Command::new(env!("CARGO_BIN_EXE_vuwr"))
}

/// Piped output has no terminal to draw on, so vuwr writes the document
/// through and exits — the way a pager does. This used to try to launch
/// the GUI, so `vuwr f.csv | head` failed for no good reason.
#[test]
fn piped_output_writes_the_document_through() {
    let dir = std::env::temp_dir().join("vuwr-cli-pipe");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("t.csv");
    std::fs::write(&path, "a,b\n1,2\n").unwrap();

    let out = vuwr().arg(&path).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout), "a,b\n1,2\n");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn reads_from_stdin() {
    for (label, input) in [
        ("csv", "a,b\n1,2\n"),
        ("json", "{\"a\":1}"),
        ("xml", "<r><a/></r>"),
    ] {
        let mut child = vuwr()
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{label}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            input,
            "{label} round-trips"
        );
    }
}

/// A bare `vuwr` with something piped in reads it, no `-` needed.
#[test]
fn bare_invocation_reads_piped_input() {
    let mut child = vuwr()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"x,y\n3,4\n")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), "x,y\n3,4\n");
}

#[test]
fn missing_file_reports_and_fails() {
    let out = vuwr().arg("/nonexistent-vuwr/nope.csv").output().unwrap();
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("nope.csv"),
        "names the file"
    );
}

/// Invalid input must fail loudly rather than opening an empty view.
#[test]
fn unparseable_input_reports_the_error() {
    let mut child = vuwr()
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"{\"a\":,,,}")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("unexpected token"), "useful error: {err}");
}

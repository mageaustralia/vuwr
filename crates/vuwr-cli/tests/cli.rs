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

// --- --check (phase 5) ---

fn write(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, body).unwrap();
    p
}

#[test]
fn check_succeeds_silently_on_valid_input() {
    let dir = std::env::temp_dir().join("vuwr-check-ok");
    std::fs::create_dir_all(&dir).unwrap();
    let files = [
        write(&dir, "a.json", "{\"a\": [1, 2, 3]}"),
        write(&dir, "b.xml", "<?xml version=\"1.0\"?><r><a/></r>"),
        write(&dir, "c.csv", "x,y\n1,2\n"),
    ];
    for f in &files {
        let out = Command::new(env!("CARGO_BIN_EXE_vuwr"))
            .arg("--check")
            .arg(f)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}: {}",
            f.display(),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            out.stdout.is_empty() && out.stderr.is_empty(),
            "silent on success"
        );
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// The whole point of --check: a non-zero status and a position you can
/// jump to, the way `jq empty` and `xmllint --noout` are used in scripts.
#[test]
fn check_fails_with_a_line_and_column() {
    let dir = std::env::temp_dir().join("vuwr-check-bad");
    std::fs::create_dir_all(&dir).unwrap();
    let f = write(&dir, "bad.json", "{\n  \"a\": 1,\n  \"b\": ,\n}");

    let out = Command::new(env!("CARGO_BIN_EXE_vuwr"))
        .arg("--check")
        .arg(&f)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("bad.json:3:"),
        "names file, line and column: {err}"
    );
    assert!(err.contains("unexpected token"), "{err}");
    std::fs::remove_dir_all(&dir).ok();
}

/// A missing file is a different problem from an invalid one, so scripts
/// can tell them apart.
#[test]
fn check_distinguishes_unreadable_from_invalid() {
    let out = Command::new(env!("CARGO_BIN_EXE_vuwr"))
        .args(["--check", "/nonexistent-vuwr/x.json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "unreadable is 2, not 1");
}

#[test]
fn check_accepts_many_files_and_reports_each() {
    let dir = std::env::temp_dir().join("vuwr-check-many");
    std::fs::create_dir_all(&dir).unwrap();
    let good = write(&dir, "good.json", "{\"a\":1}");
    let bad1 = write(&dir, "bad1.json", "{\"a\":}");
    let bad2 = write(&dir, "bad2.json", "[1,2,");

    let out = Command::new(env!("CARGO_BIN_EXE_vuwr"))
        .arg("--check")
        .args([&good, &bad1, &bad2])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("bad1.json"), "{err}");
    assert!(
        err.contains("bad2.json"),
        "reports every failure, not just the first: {err}"
    );
    assert!(
        !err.contains("good.json"),
        "silent about what passed: {err}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn check_reads_stdin_and_can_be_quiet() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_vuwr"))
        .args(["--check", "--quiet"])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"{\"a\":,}")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stderr.is_empty(), "--quiet says nothing");
}

/// CSV gets checked too, which neither jq nor xmllint can do.
#[test]
fn check_covers_csv() {
    let dir = std::env::temp_dir().join("vuwr-check-csv");
    std::fs::create_dir_all(&dir).unwrap();
    let f = write(&dir, "bad.csv", "a,b\n\"unclosed,2\n");
    let out = Command::new(env!("CARGO_BIN_EXE_vuwr"))
        .args(["--check"])
        .arg(&f)
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(1),
        "an unclosed quote is invalid CSV"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("bad.csv:2:"), "with a position: {err}");
    std::fs::remove_dir_all(&dir).ok();
}

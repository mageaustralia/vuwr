//! `cat feed.xml | vuwr` — a document on stdin and a terminal on stdout.
//!
//! The README offers this and it did not work: reading the document left
//! file descriptor 0 an exhausted pipe, so there was nowhere to read a
//! keystroke from. Crossterm's answer to that is to open `/dev/tty`, and
//! on macOS a descriptor obtained that way cannot be registered with
//! kqueue — so the reader failed to initialise and vuwr quit before
//! drawing anything. The terminal's reply to our question about its own
//! background colour was then left unread, and the shell ran it as a
//! command.
//!
//! Testing it needs a real terminal on the other end, so the test builds
//! one. Unix only: `openpty` has no Windows equivalent, and crossterm
//! reads the console directly there rather than a descriptor.

#![cfg(unix)]
// Building a terminal means system calls, and this whole file is that.
// The workspace warns on `unsafe` so each block has to argue for itself;
// here the argument is the file's own purpose, made once at the top.
#![allow(unsafe_code)]

use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// A pseudo-terminal, as the master and slave ends.
///
/// Through `posix_openpt` rather than `openpty`, which on Linux lives in
/// a separate library the `libc` crate does not link by default.
fn pty() -> (OwnedFd, OwnedFd) {
    unsafe {
        let master = libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY);
        assert!(master >= 0, "posix_openpt: {}", last_error());
        assert_eq!(libc::grantpt(master), 0, "grantpt: {}", last_error());
        assert_eq!(libc::unlockpt(master), 0, "unlockpt: {}", last_error());

        let name = libc::ptsname(master);
        assert!(!name.is_null(), "ptsname: {}", last_error());
        let slave = libc::open(name, libc::O_RDWR | libc::O_NOCTTY);
        assert!(slave >= 0, "open slave: {}", last_error());

        // A pty starts with no size at all, and a terminal of no rows and
        // no columns has nowhere to draw a table: the program starts,
        // finds nothing to fill, and leaves again.
        let size = libc::winsize {
            ws_row: 40,
            ws_col: 120,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        assert_eq!(
            libc::ioctl(slave, libc::TIOCSWINSZ as _, &size),
            0,
            "TIOCSWINSZ: {}",
            last_error()
        );

        (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave))
    }
}

fn last_error() -> std::io::Error {
    std::io::Error::last_os_error()
}

/// Read until `needle` appears or the deadline passes.
///
/// Waiting for what the child drew rather than for a fixed pause: a debug
/// build on a loaded machine takes its time to start, and a test that
/// sends its keystrokes before the program is listening tests nothing.
fn read_until(master: &OwnedFd, needle: &str, until: Duration) -> String {
    let deadline = Instant::now() + until;
    let mut out = String::new();
    while Instant::now() < deadline {
        out.push_str(&read_available(master, Duration::from_millis(100)));
        if out.contains(needle) {
            break;
        }
    }
    out
}

/// Read whatever the child has written, until it goes quiet or the
/// deadline passes. Never blocks for longer than the deadline, so a
/// failure is a failed assertion rather than a hung suite.
fn read_available(master: &OwnedFd, until: Duration) -> String {
    let deadline = Instant::now() + until;
    let mut out = Vec::new();
    let mut file = unsafe { std::fs::File::from_raw_fd(libc::dup(master.as_raw_fd())) };
    // Non-blocking, so the loop owns the timing.
    unsafe {
        let flags = libc::fcntl(file.as_raw_fd(), libc::F_GETFL);
        libc::fcntl(file.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK);
    }
    let mut buf = [0u8; 4096];
    while Instant::now() < deadline {
        match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => out.extend_from_slice(&buf[..n]),
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Run vuwr with `input` on stdin and a terminal on stdout, send `keys`,
/// and return what it drew.
fn run_piped(input: &str, keys: &str) -> String {
    run_program(env!("CARGO_BIN_EXE_vuwr"), &[], input, keys)
}

fn run_program(program: &str, args: &[&str], input: &str, keys: &str) -> String {
    let (master, slave) = pty();
    let slave_fd = slave.as_raw_fd();

    // The document in a file rather than a pipe we feed after spawning:
    // either way stdin is not a terminal, which is the condition under
    // test, and this way the child sees the whole document and its end
    // without depending on when the parent gets around to writing.
    let mut document = std::env::temp_dir();
    document.push(format!("vuwr-piped-{}.csv", std::process::id()));
    std::fs::write(&document, input).expect("write document");
    let file = std::fs::File::open(&document).expect("open document");

    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::from(file))
        .stdout(Stdio::from(slave.try_clone().expect("clone slave")))
        .stderr(Stdio::from(slave.try_clone().expect("clone slave")))
        .env_remove("COLORFGBG")
        .env("TERM", "xterm-256color");

    // A session of its own with the pty as its controlling terminal,
    // which is what makes `/dev/tty` mean this pty, and what a shell does
    // when it starts a program.
    //
    // SAFETY: `pre_exec` runs between fork and exec, where only
    // async-signal-safe calls are allowed. `setsid`, `ioctl` and
    // `tcsetpgrp` all are.
    unsafe {
        command.pre_exec(move || {
            libc::setsid();
            // The request's type differs between the BSDs and Linux.
            if libc::ioctl(slave_fd, libc::TIOCSCTTY as _, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            // And the foreground process group of that terminal. Without
            // this the child is a background job, and turning raw mode on
            // calls `tcsetattr`, which stops a background process with
            // SIGTTOU: alive, silent, and drawing nothing.
            libc::tcsetpgrp(slave_fd, libc::getpid());
            Ok(())
        });
    }
    let mut child = command.spawn().expect("spawn vuwr");

    // The alternate screen means the reader is up and raw mode is on, so
    // a keystroke will reach the program rather than be echoed back.
    let drawn = read_until(&master, "\x1b[?1049h", Duration::from_secs(10));
    let drawn = format!(
        "{drawn}{}",
        read_available(&master, Duration::from_millis(400))
    );

    let mut keyboard = unsafe { std::fs::File::from_raw_fd(libc::dup(master.as_raw_fd())) };
    let _ = keyboard.write_all(keys.as_bytes());
    let _ = keyboard.flush();

    let rest = read_available(&master, Duration::from_millis(800));
    let _ = child.kill();
    let _ = child.wait();
    // Held until now: on macOS, when the last descriptor for the slave
    // closes, anything still in the buffer is discarded rather than
    // delivered — so a short-lived child's output would vanish.
    drop(slave);
    let _ = std::fs::remove_file(&document);
    format!("{drawn}{rest}")
}

/// The document arrives, the terminal still works, and the reader starts.
#[test]
fn a_piped_document_opens_in_the_terminal() {
    let out = run_piped("sku,qty\nSKU-1001,7\nSKU-1002,3\n", "q");

    assert!(
        !out.contains("Failed to initialize input reader"),
        "vuwr could not read the keyboard:\n{out}"
    );
    // The alternate screen is where the table is drawn. Reaching it means
    // the reader was built and the first frame went out.
    assert!(
        out.contains("\x1b[?1049h"),
        "vuwr never drew anything:\n{out:?}"
    );
    assert!(
        out.contains("SKU-1001"),
        "the piped document was not displayed:\n{out:?}"
    );
}

/// And the terminal's answer is consumed rather than echoed.
///
/// The weaker of the two: it holds even against the old code, which quit
/// before the reply could be echoed. The one above is what pins the
/// failure. This guards the other half — that the reply we asked for is
/// swallowed by the program rather than left on screen or, as it was, in
/// the buffer for the shell to run as a command.
///
/// The reply to `OSC 11` goes to the terminal's input, which is stdin. It
/// was a pipe, so the answer sat unread until the shell got it and tried
/// to run `11;rgb:ffff/ffff/ffff` as a command.
#[test]
fn the_terminals_reply_is_not_left_for_the_shell() {
    let out = run_piped("a,b\n1,2\n", "\x1b]11;rgb:1e1e/1e1e/2e2e\x07q");

    assert!(
        !out.contains("rgb:"),
        "the terminal's answer was echoed back rather than consumed:\n{out:?}"
    );
}

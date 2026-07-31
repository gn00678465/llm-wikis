//! Disposable fixture for `tests/process_supervisor.rs` (plan Task 8 Step 1). Every
//! mode is a thin, deliberately dumb std-only primitive; no argument parsing beyond
//! positional strings, no config, no dependencies. Not part of the workspace and
//! never shipped.

use std::env;
use std::io::{self, Read, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("echo-stdin") => echo_stdin(),
        Some("echo-argv") => echo_argv(&args[1..]),
        Some("sleep-ms") => sleep_ms(&args),
        Some("exit-code") => exit_code(&args),
        Some("flood-stdout") => flood(Stream::Stdout, &args),
        Some("flood-stderr") => flood(Stream::Stderr, &args),
        Some("dual-pipe-pressure") => dual_pipe_pressure(),
        Some("grandchild") => grandchild(&args),
        other => {
            eprintln!("process-helper: unknown mode {other:?}");
            std::process::exit(2);
        }
    }
}

fn arg_u64(args: &[String], index: usize) -> u64 {
    args.get(index)
        .unwrap_or_else(|| panic!("missing positional argument {index}"))
        .parse()
        .unwrap_or_else(|e| panic!("argument {index} is not a u64: {e}"))
}

fn drain_stdin() -> Vec<u8> {
    let mut buf = Vec::new();
    let _ = io::stdin().read_to_end(&mut buf);
    buf
}

/// Reads all of stdin, writes it back verbatim, then writes a marker only
/// reachable once `read_to_end` observed EOF (spec §10.1 stdin-then-close).
fn echo_stdin() {
    let received = drain_stdin();
    let mut out = io::stdout();
    out.write_all(&received).unwrap();
    out.write_all(b"\n<<STDIN-EOF-OBSERVED>>\n").unwrap();
    out.flush().unwrap();
}

/// Prints every argv element it actually received, one per line, plus the byte
/// length of whatever arrived on stdin — proving argv and stdin never mix
/// (plan Task 8 Steps 2-3: batch-shim/metacharacter boundary).
fn echo_argv(rest: &[String]) {
    let stdin_len = drain_stdin().len();
    for (i, a) in rest.iter().enumerate() {
        println!("ARG{i}=[{a}]");
    }
    println!("STDIN_LEN={stdin_len}");
}

fn sleep_ms(args: &[String]) {
    let ms = arg_u64(args, 1);
    std::thread::sleep(Duration::from_millis(ms));
}

fn exit_code(args: &[String]) {
    let code = arg_u64(args, 1);
    std::process::exit(code as i32);
}

enum Stream {
    Stdout,
    Stderr,
}

/// Writes `total_bytes` of a fixed pattern to one stream in fixed-size chunks,
/// flushing after each chunk so a capped reader on the other end observes
/// gradual growth rather than one giant write (plan Task 8 Step 7: streaming
/// cap enforcement, not post-buffering).
fn flood(stream: Stream, args: &[String]) {
    let total = arg_u64(args, 1) as usize;
    const CHUNK: usize = 4096;
    let chunk = vec![b'x'; CHUNK];
    let mut written = 0usize;
    while written < total {
        let n = CHUNK.min(total - written);
        match stream {
            Stream::Stdout => {
                let mut out = io::stdout();
                out.write_all(&chunk[..n]).unwrap();
                out.flush().unwrap();
            }
            Stream::Stderr => {
                let mut err = io::stderr();
                err.write_all(&chunk[..n]).unwrap();
                err.flush().unwrap();
            }
        }
        written += n;
    }
}

/// Reproduces the adversarial one-pipe-full-while-other-blocked pattern:
/// a short stdout write, then a large stderr write, then a final stdout
/// write — proving concurrent (not sequential) draining (plan Task 8 Step 7).
fn dual_pipe_pressure() {
    let mut out = io::stdout();
    out.write_all(b"ready").unwrap();
    out.flush().unwrap();

    let big = vec![b'e'; 300_000];
    let mut err = io::stderr();
    err.write_all(&big).unwrap();
    err.flush().unwrap();

    out.write_all(b"done").unwrap();
    out.flush().unwrap();
}

/// Spawns exactly one grandchild of itself (a plain `sleep-ms`, no wrapping of
/// its own), prints both PIDs, then blocks for the same duration — giving the
/// test window to observe both alive before the supervisor kills the tree
/// (plan Task 8 Step 8/9: process-tree termination reaches grandchildren).
fn grandchild(args: &[String]) {
    let ms = arg_u64(args, 1);
    let exe = env::current_exe().expect("current_exe");
    let child = Command::new(exe)
        .arg("sleep-ms")
        .arg(ms.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn grandchild");
    println!("PARENT_PID={}", std::process::id());
    println!("CHILD_PID={}", child.id());
    io::stdout().flush().unwrap();
    std::thread::sleep(Duration::from_millis(ms));
}

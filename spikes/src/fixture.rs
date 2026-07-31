//! Hidden child-process modes reused by several spikes. The spike binary
//! re-invokes itself (`std::env::current_exe()`) with `__fixture <mode>` as
//! argv so the "child under test" is always this same compiled program.
//! This is spike scaffolding only; it is never invoked directly by a user.

use std::io::{Read, Write};

pub fn run(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        Some("echo-stdin") => echo_stdin(),
        Some("argv-echo") => argv_echo(&args[1..]),
        Some("alternate") => alternate(&args[1..]),
        Some("fill-block") => fill_block(&args[1..]),
        other => {
            eprintln!("unknown fixture mode: {other:?}");
            2
        }
    }
}

/// Reads all of stdin verbatim and writes it back to stdout unchanged.
/// Reports its own argv (as this process's OS-level view of it, not an
/// assumption) on stderr as one JSON line, so the parent can independently
/// verify argv/stdin separation.
fn echo_stdin() -> i32 {
    let argv: Vec<String> = std::env::args().collect();
    eprintln!("{}", serde_json::json!({ "argv": argv }));
    let mut buf = Vec::new();
    if std::io::stdin().read_to_end(&mut buf).is_err() {
        return 2;
    }
    let mut stdout = std::io::stdout();
    if stdout.write_all(&buf).is_err() || stdout.flush().is_err() {
        return 2;
    }
    0
}

/// Prints this process's argv (elements after "argv-echo") as a JSON array
/// on stdout. Used to prove exact argv arrival through a Windows .cmd shim.
fn argv_echo(rest: &[String]) -> i32 {
    println!("{}", serde_json::json!({ "argv": rest }));
    0
}

/// Alternately writes `count` chunks of `chunk_size` bytes to stdout then
/// stderr, to exceed a bounded-pipe cap on both streams.
fn alternate(args: &[String]) -> i32 {
    let count: usize = args.first().and_then(|s| s.parse().ok()).unwrap_or(50);
    let chunk_size: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(2000);
    let chunk = "o".repeat(chunk_size);
    let echunk = "e".repeat(chunk_size);
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    for _ in 0..count {
        let _ = writeln!(stdout, "{chunk}");
        let _ = stdout.flush();
        let _ = writeln!(stderr, "{echunk}");
        let _ = stderr.flush();
    }
    0
}

/// Writes a small "ready" marker to stdout, then a single large write to
/// stderr (large enough to fill an unread OS pipe buffer and block), then a
/// "done" marker to stdout. A supervisor that drains stdout/stderr
/// sequentially instead of concurrently will deadlock on this fixture.
fn fill_block(args: &[String]) -> i32 {
    let fill_bytes: usize = args.first().and_then(|s| s.parse().ok()).unwrap_or(300_000);
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    let _ = writeln!(stdout, "ready");
    let _ = stdout.flush();
    let block = "e".repeat(fill_bytes);
    let _ = stderr.write_all(block.as_bytes());
    let _ = stderr.flush();
    let _ = writeln!(stdout, "done");
    let _ = stdout.flush();
    0
}

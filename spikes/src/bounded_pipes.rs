//! Step 7: prove bounded concurrent output (spec §10.1, plan Task 2 Step 7).
//! Drains stdout/stderr on separate threads so neither pipe can deadlock the
//! child, enforces independent byte caps, and kills the whole process tree
//! (via process-wrap's Windows Job Object wrapper) on overflow.

use crate::report::Report;
use process_wrap::std::CommandWrap;
#[cfg(windows)]
use process_wrap::std::JobObject;
use std::io::Read;
use std::process::Stdio;
use std::sync::mpsc;
use std::time::Duration;

const STDOUT_CAP: usize = 8_000;
const STDERR_CAP: usize = 8_000;
const WATCHDOG: Duration = Duration::from_secs(15);

struct DrainResult {
    label: &'static str,
    bytes: usize,
    hit_cap: bool,
}

fn spawn_capped_reader(
    mut reader: impl Read + Send + 'static,
    cap: usize,
    label: &'static str,
    tx: mpsc::Sender<DrainResult>,
) {
    std::thread::spawn(move || {
        let mut total = 0usize;
        let mut buf = [0u8; 4096];
        let mut hit_cap = false;
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    total += n;
                    if total >= cap {
                        hit_cap = true;
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = tx.send(DrainResult { label, bytes: total, hit_cap });
    });
}

pub fn run() -> i32 {
    let mut r = Report::new("bounded-pipes");
    scenario_alternate_exceeds_caps(&mut r);
    scenario_fill_one_block_other(&mut r);
    r.finish()
}

fn make_wrap(fixture_args: &[&str]) -> CommandWrap {
    let exe = std::env::current_exe().expect("current_exe");
    let mut wrap = CommandWrap::with_new(&exe, |cmd| {
        cmd.args(fixture_args);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
    });
    #[cfg(windows)]
    wrap.wrap(JobObject);
    wrap
}

fn scenario_alternate_exceeds_caps(r: &mut Report) {
    let mut wrap = make_wrap(&["__fixture", "alternate", "80", "3000"]);
    let mut child = match wrap.spawn() {
        Ok(c) => c,
        Err(e) => {
            r.check("alternate_spawn", false, format!("{e}"));
            return;
        }
    };
    let pid = child.id();
    let stdout = child.stdout().take().expect("piped stdout");
    let stderr = child.stderr().take().expect("piped stderr");

    let (tx, rx) = mpsc::channel();
    spawn_capped_reader(stdout, STDOUT_CAP, "stdout", tx.clone());
    spawn_capped_reader(stderr, STDERR_CAP, "stderr", tx);

    let first = rx.recv_timeout(WATCHDOG);
    let no_deadlock_first = first.is_ok();
    r.check(
        "alternate_no_deadlock_first_cap_signal",
        no_deadlock_first,
        format!("first drain signal within {WATCHDOG:?}: {:?}", first.as_ref().map(|d| (d.label, d.bytes, d.hit_cap))),
    );

    // Kill the whole tree once either stream exceeds its cap.
    let _ = child.start_kill();
    let wait_res = child.wait();

    let second = rx.recv_timeout(Duration::from_secs(5));
    let no_deadlock_second = second.is_ok();
    r.check(
        "alternate_no_deadlock_second_cap_signal",
        no_deadlock_second,
        format!("second drain signal after kill: {:?}", second.as_ref().map(|d| (d.label, d.bytes, d.hit_cap))),
    );

    let results: Vec<(&str, usize, bool)> = [first, second]
        .into_iter()
        .filter_map(|x| x.ok())
        .map(|d| (d.label, d.bytes, d.hit_cap))
        .collect();
    let stdout_capped = results.iter().any(|(l, b, c)| *l == "stdout" && *c && *b >= STDOUT_CAP);
    let stderr_capped = results.iter().any(|(l, b, c)| *l == "stderr" && *c && *b >= STDERR_CAP);
    r.check(
        "alternate_correct_stream_byte_counts",
        stdout_capped && stderr_capped,
        format!("results={results:?}"),
    );

    r.check(
        "alternate_process_tree_terminated",
        wait_res.is_ok() && !crate::report::pid_alive(pid),
        format!("wait={:?} pid_alive={}", wait_res.map(|s| s.code()), crate::report::pid_alive(pid)),
    );
}

fn scenario_fill_one_block_other(r: &mut Report) {
    // fill_bytes chosen well above a typical anonymous pipe buffer so the
    // child would block on the stderr write if nothing drains it.
    let mut wrap = make_wrap(&["__fixture", "fill-block", "300000"]);
    let mut child = match wrap.spawn() {
        Ok(c) => c,
        Err(e) => {
            r.check("fill_block_spawn", false, format!("{e}"));
            return;
        }
    };
    let pid = child.id();
    let stdout = child.stdout().take().expect("piped stdout");
    let stderr = child.stderr().take().expect("piped stderr");

    let (tx, rx) = mpsc::channel();
    spawn_capped_reader(stdout, usize::MAX, "stdout", tx.clone());
    spawn_capped_reader(stderr, usize::MAX, "stderr", tx);

    let a = rx.recv_timeout(WATCHDOG);
    let b = rx.recv_timeout(WATCHDOG);
    let no_deadlock = a.is_ok() && b.is_ok();
    r.check(
        "fill_block_no_deadlock_both_streams_drained",
        no_deadlock,
        format!("a={:?} b={:?}", a.as_ref().map(|d| (d.label, d.bytes)), b.as_ref().map(|d| (d.label, d.bytes))),
    );

    let stdout_bytes = [&a, &b]
        .into_iter()
        .filter_map(|x| x.as_ref().ok())
        .find(|d| d.label == "stdout")
        .map(|d| d.bytes)
        .unwrap_or(0);
    // "ready\n" + "done\n" = 11 bytes minimum.
    r.check(
        "fill_block_stdout_reached_done_marker",
        stdout_bytes >= 11,
        format!("stdout_bytes={stdout_bytes}"),
    );

    let wait_res = child.wait();
    r.check(
        "fill_block_process_tree_terminated_after_natural_exit",
        wait_res.is_ok() && !crate::report::pid_alive(pid),
        format!("wait={:?} pid_alive={}", wait_res.map(|s| s.code()), crate::report::pid_alive(pid)),
    );
}

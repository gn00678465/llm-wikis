//! Step 8: prove timeout and process-tree termination (spec §10.1, plan
//! Task 2 Step 8). Windows only in this task; Linux/macOS rows are PENDING,
//! owned by Task 14.

use crate::report::{pid_alive, Report};
use process_wrap::std::{CommandWrap, JobObject};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

fn child_child_exe() -> PathBuf {
    let mut p = std::env::current_exe().expect("current_exe");
    p.pop();
    p.push("process-tree-child.exe");
    p
}

fn read_pidfile(path: &std::path::Path, deadline: Instant) -> Option<(u32, u32)> {
    while Instant::now() < deadline {
        if let Ok(text) = std::fs::read_to_string(path) {
            if let Some((a, b)) = text.split_once(',') {
                if let (Ok(a), Ok(b)) = (a.trim().parse(), b.trim().parse()) {
                    return Some((a, b));
                }
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

pub fn run() -> i32 {
    let mut r = Report::new("process-tree");
    let helper = child_child_exe();
    if !helper.is_file() {
        r.check(
            "process_tree_child_helper_built",
            false,
            format!("expected sibling binary not found: {}", helper.display()),
        );
        return r.finish();
    }

    scenario_success(&mut r, &helper);
    scenario_timeout(&mut r, &helper);
    scenario_forced_overflow(&mut r, &helper);

    r.finish()
}

fn spawn_tree(helper: &std::path::Path, pidfile: &std::path::Path, mode: &str, stdout_piped: bool) -> CommandWrap {
    let mut wrap = CommandWrap::with_new(helper, |cmd| {
        cmd.args([pidfile.to_str().unwrap(), mode]);
        cmd.stdin(Stdio::null());
        cmd.stdout(if stdout_piped { Stdio::piped() } else { Stdio::null() });
        cmd.stderr(Stdio::null());
    });
    wrap.wrap(JobObject);
    wrap
}

fn scenario_success(r: &mut Report, helper: &std::path::Path) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pidfile = dir.path().join("pids-success.txt");
    let mut wrap = spawn_tree(helper, &pidfile, "quick", false);
    let mut child = match wrap.spawn() {
        Ok(c) => c,
        Err(e) => {
            r.check("success_spawn", false, format!("{e}"));
            return;
        }
    };

    let pids = read_pidfile(&pidfile, Instant::now() + Duration::from_secs(5));
    let Some((child_pid, gc_pid)) = pids else {
        r.check("success_pidfile_written", false, "pidfile never appeared");
        let _ = child.kill();
        return;
    };
    r.check("success_pidfile_written", true, format!("child_pid={child_pid} grandchild_pid={gc_pid}"));

    // No forced termination: the tree exits on its own within a generous bound.
    let wait_res = child.wait();
    let natural = wait_res.as_ref().map(|s| s.success()).unwrap_or(false);
    r.check(
        "success_no_forced_termination_needed",
        natural,
        format!("wait={:?}", wait_res.as_ref().map(|s| s.code())),
    );
    // give the OS a brief moment to finish tearing down both processes
    std::thread::sleep(Duration::from_millis(200));
    let both_dead = !pid_alive(child_pid) && !pid_alive(gc_pid);
    r.check(
        "success_neither_pid_alive_after_completion",
        both_dead,
        format!("child_alive={} grandchild_alive={}", pid_alive(child_pid), pid_alive(gc_pid)),
    );
}

fn scenario_timeout(r: &mut Report, helper: &std::path::Path) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pidfile = dir.path().join("pids-timeout.txt");
    let mut wrap = spawn_tree(helper, &pidfile, "loop", false);
    let mut child = match wrap.spawn() {
        Ok(c) => c,
        Err(e) => {
            r.check("timeout_spawn", false, format!("{e}"));
            return;
        }
    };

    let pids = read_pidfile(&pidfile, Instant::now() + Duration::from_secs(5));
    let Some((child_pid, gc_pid)) = pids else {
        r.check("timeout_pidfile_written", false, "pidfile never appeared");
        let _ = child.kill();
        return;
    };
    r.check("timeout_pidfile_written", true, format!("child_pid={child_pid} grandchild_pid={gc_pid}"));

    let alive_before = pid_alive(child_pid) && pid_alive(gc_pid);
    r.check(
        "timeout_both_alive_before_deadline",
        alive_before,
        format!("child_alive={} grandchild_alive={}", pid_alive(child_pid), pid_alive(gc_pid)),
    );

    // Simulate the enforced deadline: this process would loop forever
    // without a forced kill, so the supervisor terminates the whole tree.
    std::thread::sleep(Duration::from_millis(500));
    let kill_res = child.start_kill();
    let wait_res = child.wait();
    r.check(
        "timeout_forced_kill_issued",
        kill_res.is_ok(),
        format!("kill={:?} wait={:?}", kill_res, wait_res.map(|s| s.code())),
    );

    std::thread::sleep(Duration::from_millis(300));
    let both_dead = !pid_alive(child_pid) && !pid_alive(gc_pid);
    r.check(
        "timeout_neither_pid_alive_after_forced_kill",
        both_dead,
        format!("child_alive={} grandchild_alive={}", pid_alive(child_pid), pid_alive(gc_pid)),
    );
}

fn scenario_forced_overflow(r: &mut Report, helper: &std::path::Path) {
    let dir = tempfile::tempdir().expect("tempdir");
    let pidfile = dir.path().join("pids-overflow.txt");
    let mut wrap = spawn_tree(helper, &pidfile, "spew", true);
    let mut child = match wrap.spawn() {
        Ok(c) => c,
        Err(e) => {
            r.check("overflow_spawn", false, format!("{e}"));
            return;
        }
    };

    let pids = read_pidfile(&pidfile, Instant::now() + Duration::from_secs(5));
    let Some((child_pid, gc_pid)) = pids else {
        r.check("overflow_pidfile_written", false, "pidfile never appeared");
        let _ = child.kill();
        return;
    };
    r.check("overflow_pidfile_written", true, format!("child_pid={child_pid} grandchild_pid={gc_pid}"));

    // Drain stdout on a thread with a byte cap; once exceeded, this models
    // the supervisor's output-overflow trigger for a forced tree kill.
    let cap = 16_000usize;
    let mut stdout = child.stdout().take().expect("piped stdout");
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use std::io::Read;
        let mut total = 0usize;
        let mut buf = [0u8; 4096];
        loop {
            match stdout.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    total += n;
                    if total >= cap {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = tx.send(total);
    });

    let overflow_signal = rx.recv_timeout(Duration::from_secs(15));
    r.check(
        "overflow_cap_exceeded_signal_received",
        overflow_signal.is_ok(),
        format!("{overflow_signal:?}"),
    );

    let kill_res = child.start_kill();
    let wait_res = child.wait();
    r.check(
        "overflow_forced_kill_issued",
        kill_res.is_ok(),
        format!("kill={:?} wait={:?}", kill_res, wait_res.map(|s| s.code())),
    );

    std::thread::sleep(Duration::from_millis(300));
    let both_dead = !pid_alive(child_pid) && !pid_alive(gc_pid);
    r.check(
        "overflow_neither_pid_alive_after_forced_kill",
        both_dead,
        format!("child_alive={} grandchild_alive={}", pid_alive(child_pid), pid_alive(gc_pid)),
    );
}

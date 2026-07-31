//! Spike evidence only — never copied into production. See spikes/README.md.
//!
//! Helper for the `process-tree` spike (plan Task 2, Step 8). Argv:
//! `process-tree-child.exe <pidfile> <mode> [role]`
//!
//! `mode` is one of:
//!   quick - writes the pidfile then exits immediately (models a clean,
//!           unsupervised-termination completion).
//!   loop  - writes the pidfile then sleeps forever (models a hang that
//!           requires the supervisor to enforce a timeout).
//!   spew  - writes the pidfile then floods stdout forever (models a
//!           runaway process that requires the supervisor to enforce an
//!           output-byte cap).
//!
//! `role` is "top" (default) or "leaf". A "top" process spawns exactly one
//! grandchild of itself in the same mode with role "leaf", then writes both
//! PIDs (self, grandchild) to the pidfile as "self_pid,grandchild_pid". A
//! "leaf" process does not spawn further children. Both apply `mode`
//! identically, so a Job Object / process-group kill of the whole tree can
//! be proven to reach the grandchild as well as the direct child.

use std::time::Duration;

fn spew_forever() -> ! {
    loop {
        println!("{}", "x".repeat(4096));
        use std::io::Write;
        let _ = std::io::stdout().flush();
    }
}

fn loop_forever() -> ! {
    loop {
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let pidfile = args.get(1).expect("argv[1] = pidfile path");
    let mode = args.get(2).expect("argv[2] = mode").as_str();
    let role = args.get(3).map(String::as_str).unwrap_or("top");

    if role == "top" {
        let exe = std::env::current_exe().expect("current_exe");
        let child = std::process::Command::new(&exe)
            .args([pidfile.as_str(), mode, "leaf"])
            .spawn()
            .expect("spawn grandchild");
        let my_pid = std::process::id();
        let gc_pid = child.id();
        std::fs::write(pidfile, format!("{my_pid},{gc_pid}")).expect("write pidfile");

        match mode {
            "quick" => {
                // Let the grandchild finish too, then exit clean.
                let mut child = child;
                let _ = child.wait();
                std::process::exit(0);
            }
            "loop" => loop_forever(),
            "spew" => spew_forever(),
            other => {
                eprintln!("unknown mode: {other}");
                std::process::exit(2);
            }
        }
    } else {
        match mode {
            "quick" => std::process::exit(0),
            "loop" => loop_forever(),
            "spew" => spew_forever(),
            other => {
                eprintln!("unknown mode: {other}");
                std::process::exit(2);
            }
        }
    }
}

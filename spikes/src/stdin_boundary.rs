//! Step 4: prove stdin and argv separation (spec §10.1, plan Task 2 Step 4).

use crate::report::Report;
use std::io::Write;
use std::process::Stdio;

const PAYLOAD: &str = "繁體中文\n--leading-dash\n\"quotes\" `backticks` $() & | < > ^ % !\n";

pub fn run() -> i32 {
    let mut r = Report::new("stdin-boundary");
    let exe = std::env::current_exe().expect("current_exe");

    // Fixed argv, deliberately unrelated to the payload text below.
    let fixed_argv = ["__fixture", "echo-stdin", "spike-fixed-arg-1", "spike-fixed-arg-2"];

    let mut child = match std::process::Command::new(&exe)
        .args(fixed_argv)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            r.check("spawn_fixture_child", false, format!("{e}"));
            return r.finish();
        }
    };

    {
        let mut stdin = child.stdin.take().expect("piped stdin");
        if let Err(e) = stdin.write_all(PAYLOAD.as_bytes()) {
            r.check("write_stdin_payload", false, format!("{e}"));
            return r.finish();
        }
        // stdin dropped here, closing it.
    }

    let output = match child.wait_with_output() {
        Ok(o) => o,
        Err(e) => {
            r.check("wait_with_output", false, format!("{e}"));
            return r.finish();
        }
    };

    let roundtrip_ok = output.stdout == PAYLOAD.as_bytes();
    r.check(
        "stdin_roundtrip_byte_identical",
        roundtrip_ok,
        format!(
            "sent {} bytes, echoed back {} bytes, equal={}",
            PAYLOAD.len(),
            output.stdout.len(),
            roundtrip_ok
        ),
    );

    let argv_report: serde_json::Value = match serde_json::from_slice(&output.stderr) {
        Ok(v) => v,
        Err(e) => {
            r.check(
                "child_reported_argv_parses",
                false,
                format!("stderr not valid JSON: {e}"),
            );
            return r.finish();
        }
    };
    let argv: Vec<String> = argv_report["argv"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let argv_joined = argv.join("\u{1}"); // join with a separator that cannot appear in the payload

    let question_substrings = ["繁體中文", "--leading-dash", "quotes", "backticks", "$()"];
    let leaked: Vec<&str> = question_substrings
        .iter()
        .filter(|s| argv_joined.contains(*s))
        .copied()
        .collect();
    r.check(
        "fixture_argv_contains_no_question_substring",
        leaked.is_empty(),
        format!("child argv={argv:?}, leaked_substrings={leaked:?}"),
    );

    r.finish()
}

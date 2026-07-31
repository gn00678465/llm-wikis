//! Step 5: prove Windows executable resolution and the batch-shim boundary
//! (spec §10.1, plan Task 2 Step 5).

use crate::report::Report;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Minimal PATH/PATHEXT resolver, mirroring what Windows `CreateProcess`
/// search + shell association would find, without shelling out.
fn resolve_on_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
    let exts: Vec<String> = pathext.split(';').map(|s| s.to_lowercase()).collect();
    for dir in std::env::split_paths(&path_var) {
        for ext in &exts {
            let candidate = dir.join(format!("{name}{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn classify(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .as_deref()
    {
        Some("exe") => "exe",
        Some("cmd") | Some("bat") => "cmd-or-bat",
        _ => "unknown",
    }
}

const METACHAR_ARGS: [&str; 7] = ["&", "|", "^", "%VAR%", "!DELAYED!", ">", "<"];

pub fn run() -> i32 {
    let mut r = Report::new("windows-resolution");

    // --- Part A: resolve and classify the real provider executables. ---
    for name in ["claude", "codex"] {
        match resolve_on_path(name) {
            Some(p) => {
                let class = classify(&p);
                r.check(
                    &format!("resolve_{name}"),
                    class == "exe",
                    format!("resolved {} -> {} (classified {})", name, p.display(), class),
                );
                // Non-billable: --version only, direct exe spawn, no shell.
                let out = Command::new(&p)
                    .arg("--version")
                    .stdin(Stdio::null())
                    .output();
                match out {
                    Ok(o) => r.check(
                        &format!("{name}_version_direct_exe_spawn"),
                        o.status.success(),
                        format!("exit={:?} stdout={:?}", o.status.code(), String::from_utf8_lossy(&o.stdout).trim()),
                    ),
                    Err(e) => r.check(&format!("{name}_version_direct_exe_spawn"), false, format!("{e}")),
                }
            }
            None => r.check(&format!("resolve_{name}"), false, "not found on PATH (PENDING-worthy, not FAIL: provider absence)"),
        }
    }

    // --- Part B: build a fixture .cmd shim under a path containing a space,
    //     and determine the exact quoting rule that keeps metacharacter
    //     argv elements literal through it. ---
    let tmp = match tempfile::Builder::new().prefix("llm-wikis-spike-").tempdir() {
        Ok(t) => t,
        Err(e) => {
            r.check("create_tempdir", false, format!("{e}"));
            return r.finish();
        }
    };
    let space_dir = tmp.path().join("quote test dir");
    if let Err(e) = std::fs::create_dir_all(&space_dir) {
        r.check("create_space_dir", false, format!("{e}"));
        return r.finish();
    }
    r.check(
        "space_dir_contains_space",
        space_dir.display().to_string().contains(' '),
        format!("{}", space_dir.display()),
    );

    let exe = std::env::current_exe().expect("current_exe");
    let cmd_path = space_dir.join("echo-args.cmd");
    // The .cmd's only job is to hand argv straight to our own fixture, which
    // reports exactly what it received as JSON.
    let script = format!(
        "@echo off\r\n\"{}\" __fixture argv-echo %*\r\n",
        exe.display()
    );
    if let Err(e) = std::fs::write(&cmd_path, script) {
        r.check("write_fixture_cmd", false, format!("{e}"));
        return r.finish();
    }

    // Candidate 1: spawn the .cmd directly via std::process::Command, no
    // manual cmd.exe wrapping. Since RUSTSEC-2024-0243, std on Windows
    // auto-detects a .bat/.cmd program and internally routes it through
    // cmd.exe while escaping shell metacharacters in each argument. Test
    // whether that holds on this toolchain (rustc 1.97.1) rather than
    // assume it.
    let direct = spawn_and_capture_argv(Command::new(&cmd_path).args(METACHAR_ARGS));
    let direct_ok = match &direct {
        Ok(got) => got == &METACHAR_ARGS,
        Err(_) => false,
    };

    let (rule, rule_argv) = if direct_ok {
        (
            "direct: Command::new(<path-to.cmd>).args([...]) — std::process::Command on this Rust version (1.97.1) auto-detects the .bat/.cmd extension and internally invokes it via cmd.exe with per-argument escaping that keeps shell metacharacters (&,|,^,%VAR%,!DELAYED!,<,>) literal in the child's argv. No manual cmd.exe wrapping needed.",
            direct,
        )
    } else {
        // Candidate 2 (manual fallback): explicit cmd.exe /D /S /C wrapping,
        // with the .cmd path and every argument passed as separate
        // Command::arg() elements (never pre-joined into one string), which
        // is what actually defeats cmd.exe's own metacharacter parser: cmd
        // only special-cases &,|,^,<,> when they appear unquoted in the
        // assembled command line, and std's per-argument quoting wraps any
        // argument containing such characters in double quotes.
        let fallback = spawn_and_capture_argv(
            Command::new("cmd")
                .arg("/D")
                .arg("/S")
                .arg("/C")
                .arg(&cmd_path)
                .args(METACHAR_ARGS),
        );
        (
            "fallback: Command::new(\"cmd\").args([\"/D\",\"/S\",\"/C\"]).arg(<path-to.cmd>).args([...]) — direct spawn of the .cmd failed, so the .cmd must be invoked through an explicit cmd.exe /D /S /C wrapper with every argument passed as a separate Command::arg() (never string-joined) so std's own per-argument escaping quotes any metacharacter-bearing token.",
            fallback,
        )
    };

    let rule_ok = match &rule_argv {
        Ok(got) => got == &METACHAR_ARGS,
        Err(_) => false,
    };
    r.check(
        "cmd_shim_metacharacter_argv_exact_no_expansion",
        rule_ok,
        format!(
            "direct_attempt_ok={direct_ok} chosen_rule=[{rule}] result={:?}",
            rule_argv
        ),
    );

    r.finish()
}

/// Spawns `cmd`, waits for it, and parses the fixture's argv-echo JSON from
/// stdout. Returns Err on any spawn/parse failure or non-success exit.
fn spawn_and_capture_argv(cmd: &mut Command) -> Result<Vec<String>, String> {
    let out = cmd
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("spawn error: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "non-success exit={:?} stderr={:?}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("stdout not JSON: {e}"))?;
    let argv = v["argv"]
        .as_array()
        .ok_or_else(|| "no argv array in fixture output".to_string())?
        .iter()
        .filter_map(|x| x.as_str().map(str::to_owned))
        .collect();
    Ok(argv)
}

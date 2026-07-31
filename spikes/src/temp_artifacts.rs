//! Step 10: prove temp-artifact safety (spec §10.1, plan Task 2 Step 10).
//! Configured roots are synthetic tempdirs, never D:\Wikis.

use crate::report::Report;
use std::path::{Path, PathBuf};

/// Refuses a candidate temp root that is, or is nested inside, any
/// configured project/content root. Must be checked before any artifact is
/// created or any child spawned.
fn safe_temp_root(candidate: &Path, configured_roots: &[PathBuf]) -> Result<PathBuf, String> {
    let canonical = candidate
        .canonicalize()
        .map_err(|e| format!("cannot canonicalize candidate: {e}"))?;
    for root in configured_roots {
        let croot = root
            .canonicalize()
            .map_err(|e| format!("cannot canonicalize configured root: {e}"))?;
        if canonical == croot || canonical.starts_with(&croot) {
            return Err(format!(
                "candidate {} is inside configured root {}",
                canonical.display(),
                croot.display()
            ));
        }
    }
    Ok(canonical)
}

pub fn run() -> i32 {
    let mut r = Report::new("temp-artifacts");

    // Two synthetic "configured roots" — never the real D:\Wikis.
    let root_a = tempfile::tempdir().expect("tempdir a");
    let root_b = tempfile::tempdir().expect("tempdir b");
    let configured_roots = vec![root_a.path().to_path_buf(), root_b.path().to_path_buf()];

    // --- Injected unsafe temp root must fail before spawn. ---
    let unsafe_candidate = root_a.path().join("nested-unsafe-temp");
    std::fs::create_dir_all(&unsafe_candidate).expect("mkdir unsafe candidate");
    let unsafe_result = safe_temp_root(&unsafe_candidate, &configured_roots);
    r.check(
        "unsafe_injected_root_rejected_before_spawn",
        unsafe_result.is_err(),
        format!("{unsafe_result:?}"),
    );

    // --- Safe candidate: real OS temp dir, unrelated to the synthetic roots. ---
    let safe_base = tempfile::tempdir().expect("safe base tempdir");
    let safe_root = match safe_temp_root(safe_base.path(), &configured_roots) {
        Ok(p) => p,
        Err(e) => {
            r.check("safe_root_accepted", false, e);
            return r.finish();
        }
    };
    r.check("safe_root_accepted_outside_configured_roots", true, format!("{}", safe_root.display()));

    let claude_cfg = safe_root.join("claude-mcp-config.json");
    let codex_schema = safe_root.join("codex-output-schema.json");
    let batch_helper = safe_root.join("batch-helper.cmd");

    let create_ok = create_exclusive(&claude_cfg, "{\"mcpServers\":{}}")
        && create_exclusive(&codex_schema, "{\"type\":\"object\"}")
        && create_exclusive(&batch_helper, "@echo off\r\n");
    r.check("all_three_artifacts_created_exclusively", create_ok, format!("{} {} {}", claude_cfg.display(), codex_schema.display(), batch_helper.display()));

    // Re-attempting exclusive creation at the same path must fail.
    let reuse_rejected = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&claude_cfg)
        .is_err();
    r.check("exclusive_creation_rejects_reuse_of_same_path", reuse_rejected, "second create_new attempt");

    // User-only permissions where supported (Windows: icacls, best-effort).
    let mut perms_ok = true;
    for p in [&claude_cfg, &codex_schema, &batch_helper] {
        let username = std::env::var("USERNAME").unwrap_or_default();
        let out = std::process::Command::new("icacls")
            .arg(p)
            .arg("/inheritance:r")
            .arg("/grant:r")
            .arg(format!("{username}:F"))
            .output();
        perms_ok &= out.map(|o| o.status.success()).unwrap_or(false);
    }
    r.check("user_only_permissions_applied_where_supported", perms_ok, "icacls /inheritance:r /grant:r <user>:F on each artifact");

    // Survival until child reap: spawn a trivial child while artifacts exist.
    let exe = std::env::current_exe().expect("current_exe");
    let child = std::process::Command::new(&exe)
        .args(["__fixture", "argv-echo", "alive-check"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .spawn();
    let survived = match child {
        Ok(mut c) => {
            let alive_during = claude_cfg.exists() && codex_schema.exists() && batch_helper.exists();
            let _ = c.wait(); // reap
            alive_during
        }
        Err(_) => false,
    };
    r.check("artifacts_survive_until_child_reap", survived, "checked existence while child ran, then reaped");

    // Removal after.
    let _ = std::fs::remove_file(&claude_cfg);
    let _ = std::fs::remove_file(&codex_schema);
    let _ = std::fs::remove_file(&batch_helper);
    let removed = !claude_cfg.exists() && !codex_schema.exists() && !batch_helper.exists();
    r.check("artifacts_removed_after_reap", removed, "all three unlinked");

    r.finish()
}

fn create_exclusive(path: &Path, content: &str) -> bool {
    use std::io::Write;
    match std::fs::OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut f) => f.write_all(content.as_bytes()).is_ok(),
        Err(_) => false,
    }
}

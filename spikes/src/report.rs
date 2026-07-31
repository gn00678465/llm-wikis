//! Tiny structured evidence collector shared by every spike subcommand.
//! Each spike prints one JSON object to stdout and returns an exit code:
//! 0 = every check passed, 1 = at least one check failed.

pub struct Report {
    spike: &'static str,
    checks: Vec<serde_json::Value>,
    ok: bool,
}

impl Report {
    pub fn new(spike: &'static str) -> Self {
        Self {
            spike,
            checks: Vec::new(),
            ok: true,
        }
    }

    pub fn check(&mut self, name: &str, pass: bool, detail: impl Into<String>) {
        if !pass {
            self.ok = false;
        }
        self.checks.push(serde_json::json!({
            "name": name,
            "pass": pass,
            "detail": detail.into(),
        }));
    }

    pub fn finish(self) -> i32 {
        let v = serde_json::json!({
            "spike": self.spike,
            "ok": self.ok,
            "checks": self.checks,
        });
        println!("{}", serde_json::to_string_pretty(&v).unwrap());
        if self.ok { 0 } else { 1 }
    }
}

/// Best-effort liveness check for a Windows PID via `tasklist`. Used by the
/// process-tree spike to prove a killed tree has no surviving members.
pub fn pid_alive(pid: u32) -> bool {
    let out = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
        .output();
    let Ok(out) = out else { return false };
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines().any(|line| {
        let cols: Vec<&str> = line.split(',').collect();
        cols.get(1)
            .map(|c| c.trim_matches('"') == pid.to_string())
            .unwrap_or(false)
    })
}

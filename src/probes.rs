//! Probe identity, the read-only gate, and the on-disk atomic probe store
//! (spec §15.1; plan Core Interfaces, plan Task 11).
//!
//! Task 9 defined only the logical key, the stored record shape, the
//! read-only [`ProbeReader`] trait `QueryService` depends on, [`QueryMode`],
//! and an in-memory test double — no filesystem access and no `ProbeWriter`.
//! [`ProbeReader`] is the seam that lets Task 10's `QueryService` depend on
//! this module without depending on Task 11's on-disk store (plan Core
//! Interfaces, "Dependency direction": Task 9 -> Task 10 -> Task 11, no
//! back-edge). This task (11) adds the on-disk [`FileProbeStore`] plus the
//! [`ProbeWriter`] trait doctor publishes through — `QueryService` itself
//! still depends on `ProbeReader` only and never sees `ProbeWriter`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::config::LoadMode;
use crate::error::{AppError, ErrorCode};
use crate::output::Agent;

/// The logical probe record key (spec §15.1): canonical content/project
/// roots, selected provider, load mode, and exact entrypoint. Exactly zero or
/// one record may exist per key. Deliberately no `Hash` derive (only
/// `PartialEq`/`Eq`) so [`FakeProbeReader`] can store records in a plain
/// `Vec` without requiring `Agent`/`LoadMode` to gain a `Hash` impl this task
/// is not permitted to add to `src/output.rs`/`src/config.rs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeKey {
    pub wiki_id: String,
    pub canonical_project_root: String,
    pub canonical_content_root: String,
    pub agent: Agent,
    pub load: LoadMode,
    pub entrypoint: String,
}

/// One stored probe record (spec §15.1): the current-verification tuple
/// beyond the logical key itself — canonical provider executable path,
/// provider version, skill fingerprint, normalized compatibility
/// fingerprint, and when it was verified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeRecord {
    pub agent_executable: String,
    pub agent_version: String,
    pub skill_fingerprint: String,
    pub compatibility_fingerprint: String,
    pub verified_at: String,
}

/// Read-only probe access (plan Core Interfaces). `QueryService` (Task 10)
/// depends only on this trait, never on a concrete store, which is why the
/// query flow does not depend on the doctor task (Task 11).
pub trait ProbeReader: Send + Sync {
    fn current_record(&self, key: &ProbeKey) -> Result<Option<ProbeRecord>, AppError>;
}

/// Normal query vs. live-doctor verification (plan Core Interfaces).
/// `Enforced` requires exactly one matching current probe record before
/// invoking a provider; `Verification` (live doctor only) skips that
/// requirement and never writes a record itself — the caller publishes one
/// only after validating native output, citations, and zero mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryMode {
    Enforced,
    Verification,
}

/// An in-memory [`ProbeReader`] test double (plan Task 9 Step 1: "No
/// filesystem access and no `ProbeWriter` in this task"). Tests seed it
/// directly with [`seed`](FakeProbeReader::seed); there is no production
/// on-disk store in this module.
#[derive(Debug, Default)]
pub struct FakeProbeReader {
    records: Mutex<Vec<(ProbeKey, ProbeRecord)>>,
}

impl FakeProbeReader {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces any existing record for `key` with `record`, mirroring the
    /// real store's "exactly zero or one record per logical key" invariant
    /// (spec §15.1) without needing a `Hash` impl on `ProbeKey`.
    pub fn seed(&self, key: ProbeKey, record: ProbeRecord) {
        let mut records = self.records.lock().unwrap();
        records.retain(|(existing, _)| existing != &key);
        records.push((key, record));
    }
}

impl ProbeReader for FakeProbeReader {
    fn current_record(&self, key: &ProbeKey) -> Result<Option<ProbeRecord>, AppError> {
        let records = self.records.lock().unwrap();
        Ok(records
            .iter()
            .find(|(existing, _)| existing == key)
            .map(|(_, record)| record.clone()))
    }
}

// ---------------------------------------------------------------------------
// `verified_at` timestamp (spec §15.1) — no chrono dependency available
// ---------------------------------------------------------------------------

/// Days-since-epoch to (year, month, day), Howard Hinnant's `civil_from_days`
/// (http://howardhinnant.github.io/date_algorithms.html) — public-domain
/// integer math, no calendar crate needed for one RFC 3339 UTC stamp.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (y + i64::from(m <= 2), m, d)
}

fn format_timestamp(t: SystemTime) -> String {
    let dur = t.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = dur.as_secs() as i64;
    let days = secs.div_euclid(86400);
    let secs_of_day = secs.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    let hh = secs_of_day / 3600;
    let mm = (secs_of_day % 3600) / 60;
    let ss = secs_of_day % 60;
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

/// The current UTC instant as an RFC 3339 `Z`-suffixed string (spec §15.1
/// `verified_at`), e.g. `"2026-07-30T12:00:00Z"`. Second precision only —
/// nothing in the spec asks for sub-second resolution.
pub fn current_timestamp() -> String {
    format_timestamp(SystemTime::now())
}

// ---------------------------------------------------------------------------
// On-disk document shape (spec §15.1)
// ---------------------------------------------------------------------------

/// One stored record exactly as spec §15.1 shows it: the logical key fields
/// and the current-verification tuple flattened into one JSON object. `load`
/// is a plain string (`"project_skill"` / `"local_plugin"`, via the local
/// [`load_mode_str`] below — deliberately not imported from
/// `crate::query::load_mode_str`, to avoid adding a `probes -> query` edge on
/// top of the existing `query -> probes` one) rather than `LoadMode` itself,
/// because `LoadMode` (spec §6.2, `src/config.rs`) derives only
/// `Deserialize` — this task's file boundary does not include
/// `src/config.rs`, so a `Serialize` impl is added here instead of there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StoredRecord {
    wiki_id: String,
    canonical_project_root: String,
    canonical_content_root: String,
    agent: Agent,
    agent_executable: String,
    agent_version: String,
    load: String,
    entrypoint: String,
    skill_fingerprint: String,
    compatibility_fingerprint: String,
    verified_at: String,
}

fn load_mode_from_str(s: &str) -> Option<LoadMode> {
    match s {
        "project_skill" => Some(LoadMode::ProjectSkill),
        "local_plugin" => Some(LoadMode::LocalPlugin),
        _ => None,
    }
}

/// The wire string for a [`LoadMode`] (spec §15.1's `load` field). Kept as a
/// tiny local duplicate of `crate::query::load_mode_str` — see the
/// [`StoredRecord`] doc comment for why.
fn load_mode_str(load: LoadMode) -> &'static str {
    match load {
        LoadMode::ProjectSkill => "project_skill",
        LoadMode::LocalPlugin => "local_plugin",
    }
}

impl StoredRecord {
    fn matches_key(&self, key: &ProbeKey) -> bool {
        self.wiki_id == key.wiki_id
            && self.canonical_project_root == key.canonical_project_root
            && self.canonical_content_root == key.canonical_content_root
            && self.agent == key.agent
            && load_mode_from_str(&self.load) == Some(key.load)
            && self.entrypoint == key.entrypoint
    }

    fn to_probe_record(&self) -> ProbeRecord {
        ProbeRecord {
            agent_executable: self.agent_executable.clone(),
            agent_version: self.agent_version.clone(),
            skill_fingerprint: self.skill_fingerprint.clone(),
            compatibility_fingerprint: self.compatibility_fingerprint.clone(),
            verified_at: self.verified_at.clone(),
        }
    }

    fn from_key_and_record(key: &ProbeKey, record: &ProbeRecord) -> Self {
        Self {
            wiki_id: key.wiki_id.clone(),
            canonical_project_root: key.canonical_project_root.clone(),
            canonical_content_root: key.canonical_content_root.clone(),
            agent: key.agent,
            agent_executable: record.agent_executable.clone(),
            agent_version: record.agent_version.clone(),
            load: load_mode_str(key.load).to_string(),
            entrypoint: key.entrypoint.clone(),
            skill_fingerprint: record.skill_fingerprint.clone(),
            compatibility_fingerprint: record.compatibility_fingerprint.clone(),
            verified_at: record.verified_at.clone(),
        }
    }
}

/// The complete on-disk document (spec §15.1): `{schema_version, records}`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ProbeDocument {
    schema_version: u32,
    records: Vec<StoredRecord>,
}

// ---------------------------------------------------------------------------
// `ProbeWriter` (plan Task 11) and the atomic on-disk store
// ---------------------------------------------------------------------------

/// Publishes a current probe record for a logical key (spec §15.1). Doctor's
/// live-check flow is the only production caller; `QueryService` (Task 10)
/// depends on [`ProbeReader`] only and never sees this trait.
pub trait ProbeWriter: Send + Sync {
    fn publish(&self, key: &ProbeKey, record: &ProbeRecord) -> Result<(), AppError>;
}

fn internal_error(message: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::InternalError, message)
}

/// Best-effort user-only permission narrowing (spec §15.1: "requests
/// user-only permissions where the platform supports them") — mirrors
/// `process.rs`'s identically-named private helper; duplicated rather than
/// shared because that one is private to `src/process.rs`, which this task's
/// file boundary does not include.
#[cfg(windows)]
fn restrict_to_current_user(path: &Path) {
    let user = std::env::var("USERNAME").unwrap_or_default();
    if user.is_empty() {
        return;
    }
    let _ = std::process::Command::new("icacls")
        .arg(path)
        .arg("/inheritance:r")
        .arg("/grant:r")
        .arg(format!("{user}:F"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

#[cfg(unix)]
fn restrict_to_current_user(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(md) = fs::metadata(path) {
        let mut perms = md.permissions();
        perms.set_mode(0o600);
        let _ = fs::set_permissions(path, perms);
    }
}

/// The real on-disk probe cache (spec §15.1): one JSON document at a
/// platform-native cache path (`crate::config::default_cache_path`),
/// replaced atomically through a same-directory temporary file on every
/// [`publish`](FileProbeStore::publish).
///
/// ponytail: `publish` is a plain read-modify-write-then-atomic-rename, not a
/// compare-and-swap retry loop. Two concurrent publishes for the *same*
/// logical key are still safe (each writer's snapshot already has exactly
/// one record for that key before its rename; whichever rename lands last
/// wins, and either outcome satisfies "exactly one record"). Two concurrent
/// publishes for *different* keys racing at the same instant can still lose
/// one update (last atomic rename wins wholesale) — acceptable for a single
/// operator's local doctor cache; add a lock file or CAS retry if multi-key
/// concurrent publish ever matters in practice.
pub struct FileProbeStore {
    path: PathBuf,
}

impl FileProbeStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Missing file, unreadable file, malformed JSON, or an unrecognized
    /// `schema_version` all read as "no document yet" (spec §15.1: "A
    /// missing, malformed, duplicate, or mismatched record is unverified").
    fn read_document(&self) -> Option<ProbeDocument> {
        let text = fs::read_to_string(&self.path).ok()?;
        let doc: ProbeDocument = serde_json::from_str(&text).ok()?;
        if doc.schema_version != 1 {
            return None;
        }
        Some(doc)
    }
}

impl ProbeReader for FileProbeStore {
    fn current_record(&self, key: &ProbeKey) -> Result<Option<ProbeRecord>, AppError> {
        let Some(doc) = self.read_document() else {
            return Ok(None);
        };
        let matches: Vec<&StoredRecord> =
            doc.records.iter().filter(|r| r.matches_key(key)).collect();
        // Exactly one match is verified; zero or a corrupt duplicate pair is
        // treated as unverified (spec §15.1), never an arbitrary pick.
        Ok(match matches.as_slice() {
            [one] => Some(one.to_probe_record()),
            _ => None,
        })
    }
}

impl ProbeWriter for FileProbeStore {
    fn publish(&self, key: &ProbeKey, record: &ProbeRecord) -> Result<(), AppError> {
        let parent = self
            .path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        fs::create_dir_all(&parent)
            .map_err(|e| internal_error(format!("cannot create probe cache directory: {e}")))?;

        // "removes every existing record for the logical key and atomically
        // writes exactly one" (spec §15.1): read-modify-write, never append.
        let mut doc = self.read_document().unwrap_or_default();
        doc.schema_version = 1;
        doc.records.retain(|r| !r.matches_key(key));
        doc.records
            .push(StoredRecord::from_key_and_record(key, record));
        let json = serde_json::to_string_pretty(&doc)
            .map_err(|e| internal_error(format!("cannot serialize probe document: {e}")))?;

        let mut tmp = tempfile::Builder::new()
            .prefix(".probes-")
            .suffix(".tmp")
            .tempfile_in(&parent)
            .map_err(|e| internal_error(format!("cannot create temp probe file: {e}")))?;
        tmp.write_all(json.as_bytes())
            .map_err(|e| internal_error(format!("cannot write temp probe file: {e}")))?;
        tmp.flush()
            .map_err(|e| internal_error(format!("cannot flush temp probe file: {e}")))?;
        restrict_to_current_user(tmp.path());

        // Windows' rename-over-existing (ReplaceFile/MoveFileEx) transiently
        // returns ERROR_ACCESS_DENIED or ERROR_FILE_NOT_FOUND when two
        // publishers race to replace the same destination; POSIX rename() has
        // no such window. Spec §15.1 requires concurrent publishes to all
        // succeed, so retry a bounded number of times on those transient
        // kinds before giving up.
        let mut attempts_left = 20;
        loop {
            match tmp.persist(&self.path) {
                Ok(_) => return Ok(()),
                Err(e) => {
                    attempts_left -= 1;
                    let transient = matches!(
                        e.error.kind(),
                        std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::NotFound
                    );
                    if !transient || attempts_left == 0 {
                        return Err(internal_error(format!(
                            "cannot atomically replace probe file: {}",
                            e.error
                        )));
                    }
                    tmp = e.file;
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Local-plugin lifecycle-component rejection (spec §15.1)
// ---------------------------------------------------------------------------

/// Conventional Claude local-plugin lifecycle artifacts this wrapper never
/// allows in a fingerprinted plugin directory (spec §15.1: "rejects hooks,
/// MCP, settings, or other executable lifecycle components").
///
/// ponytail: a fixed denylist of the conventional file/directory names and
/// manifest keys, not a full plugin-schema parser — this task has no live
/// spike confirming every possible lifecycle surface Claude's plugin format
/// may grow. Extend the list if a real plugin fixture demonstrates another
/// lifecycle component slipping through.
const REJECTED_PLUGIN_PATHS: &[&str] = &["hooks", "mcp.json", ".mcp.json", "settings.json"];
const REJECTED_MANIFEST_KEYS: &[&str] = &["hooks", "mcp", "mcpServers", "settings"];

/// Fails `ENTRYPOINT_INVALID` when `plugin_dir` (or its
/// `.claude-plugin/plugin.json` manifest) declares a rejected lifecycle
/// component. Called by doctor's `entrypoint` check and before fingerprinting
/// a `local_plugin` artifact.
pub fn check_no_plugin_lifecycle_components(plugin_dir: &Path) -> Result<(), AppError> {
    for name in REJECTED_PLUGIN_PATHS {
        if plugin_dir.join(name).exists() {
            return Err(AppError::new(
                ErrorCode::EntrypointInvalid,
                format!("local plugin declares a rejected lifecycle component: {name}"),
            ));
        }
    }
    let manifest_path = plugin_dir.join(".claude-plugin").join("plugin.json");
    if let Ok(text) = fs::read_to_string(&manifest_path)
        && let Ok(serde_json::Value::Object(obj)) = serde_json::from_str(&text)
    {
        for key in REJECTED_MANIFEST_KEYS {
            if obj.contains_key(*key) {
                return Err(AppError::new(
                    ErrorCode::EntrypointInvalid,
                    format!("local plugin manifest declares a rejected lifecycle key: {key}"),
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(entrypoint: &str) -> ProbeKey {
        ProbeKey {
            wiki_id: "agents".into(),
            canonical_project_root: "D:\\Wikis\\agents".into(),
            canonical_content_root: "D:\\Wikis\\agents".into(),
            agent: Agent::Claude,
            load: LoadMode::ProjectSkill,
            entrypoint: entrypoint.into(),
        }
    }

    fn record() -> ProbeRecord {
        ProbeRecord {
            agent_executable: "C:\\claude.exe".into(),
            agent_version: "2.1.220".into(),
            skill_fingerprint: "sha256:aaaa".into(),
            compatibility_fingerprint: "sha256:bbbb".into(),
            verified_at: "2026-07-30T12:00:00Z".into(),
        }
    }

    #[test]
    fn absent_key_returns_none() {
        let reader = FakeProbeReader::new();
        assert_eq!(reader.current_record(&key("/wiki-query")).unwrap(), None);
    }

    #[test]
    fn seeded_key_returns_the_record() {
        let reader = FakeProbeReader::new();
        reader.seed(key("/wiki-query"), record());
        assert_eq!(
            reader.current_record(&key("/wiki-query")).unwrap(),
            Some(record())
        );
    }

    #[test]
    fn reseeding_the_same_key_replaces_not_duplicates() {
        let reader = FakeProbeReader::new();
        reader.seed(key("/wiki-query"), record());
        let mut second = record();
        second.agent_version = "2.1.221".into();
        reader.seed(key("/wiki-query"), second.clone());
        assert_eq!(
            reader.current_record(&key("/wiki-query")).unwrap(),
            Some(second)
        );
    }

    #[test]
    fn different_entrypoint_is_a_different_key() {
        let reader = FakeProbeReader::new();
        reader.seed(key("/wiki-query"), record());
        assert_eq!(reader.current_record(&key("/other")).unwrap(), None);
    }
}

//! Strict configuration contract (spec §5.2, §6, §6.1-§6.4, §15.1) and `config init`
//! (spec §5.1/§5.2 CLI surface, plan Task 5).
//!
//! Loading is split into two phases on purpose, matching the query flow order in
//! spec §8.1: [`load`] performs pure schema/semantic validation of the TOML document
//! (no filesystem access beyond reading the file itself) — step 2/3 of the query
//! flow. Path containment/symlink scanning ([`resolve_wiki_roots`]) and static
//! artifact addressability ([`resolve_and_check_artifact`]) are separate functions
//! invoked later, per selected wiki/agent, matching flow steps 5/6. This keeps
//! `load` cheap (safe to call once) and keeps the filesystem-touching checks
//! testable in isolation.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, ErrorCode};
use crate::output::{Agent, SCHEMA_VERSION};

// ---------------------------------------------------------------------------
// Platform paths (spec §5.2 config path, §15.1 cache path)
// ---------------------------------------------------------------------------

/// The three platform families the spec defines path formulas for. Kept distinct
/// from the actual host OS so tests can exercise all three formulas from a single
/// host without touching real environment variables (plan Task 5 Step 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Linux,
    MacOs,
}

impl Platform {
    /// The platform family this binary is actually compiled for.
    pub fn host() -> Self {
        if cfg!(target_os = "windows") {
            Platform::Windows
        } else if cfg!(target_os = "macos") {
            Platform::MacOs
        } else {
            Platform::Linux
        }
    }
}

/// Injected environment lookup so platform-path resolution never mutates
/// process-global environment in (parallel) tests.
pub trait EnvLookup {
    fn get(&self, key: &str) -> Option<String>;
}

/// The real process environment, for production use.
pub struct ProcessEnv;

impl EnvLookup for ProcessEnv {
    fn get(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

/// A fixed map used to inject fake environment values in tests.
pub struct MapEnv(pub std::collections::HashMap<String, String>);

impl EnvLookup for MapEnv {
    fn get(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
}

/// Separators are built explicitly per simulated platform (never via
/// `Path::join`, which would silently apply the *host's* separator regardless
/// of which platform's formula is being computed).
pub fn default_config_path(platform: Platform, env: &dyn EnvLookup) -> Option<PathBuf> {
    match platform {
        Platform::Windows => {
            let appdata = non_empty(env.get("APPDATA"))?;
            Some(PathBuf::from(format!("{appdata}\\llm-wikis\\config.toml")))
        }
        Platform::Linux => {
            let base = xdg_or_home_fallback(env, "XDG_CONFIG_HOME", ".config")?;
            Some(PathBuf::from(format!("{base}/llm-wikis/config.toml")))
        }
        Platform::MacOs => {
            let home = non_empty(env.get("HOME"))?;
            Some(PathBuf::from(format!(
                "{home}/Library/Application Support/llm-wikis/config.toml"
            )))
        }
    }
}

/// The machine-local probe-store cache path (spec §15.1).
pub fn default_cache_path(platform: Platform, env: &dyn EnvLookup) -> Option<PathBuf> {
    match platform {
        Platform::Windows => {
            let local = non_empty(env.get("LOCALAPPDATA"))?;
            Some(PathBuf::from(format!("{local}\\llm-wikis\\probes-v1.json")))
        }
        Platform::Linux => {
            let base = xdg_or_home_fallback(env, "XDG_CACHE_HOME", ".cache")?;
            Some(PathBuf::from(format!("{base}/llm-wikis/probes-v1.json")))
        }
        Platform::MacOs => {
            let home = non_empty(env.get("HOME"))?;
            Some(PathBuf::from(format!(
                "{home}/Library/Caches/llm-wikis/probes-v1.json"
            )))
        }
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.is_empty())
}

fn xdg_or_home_fallback(env: &dyn EnvLookup, xdg_var: &str, home_suffix: &str) -> Option<String> {
    non_empty(env.get(xdg_var))
        .or_else(|| non_empty(env.get("HOME")).map(|h| format!("{h}/{home_suffix}")))
}

/// `--config` is operator/testing-only and absolute-only (spec §5.2).
pub fn validate_config_override(path: &Path) -> Result<(), AppError> {
    if path.is_absolute() {
        Ok(())
    } else {
        Err(AppError::new(
            ErrorCode::ArgumentInvalid,
            "--config override must be an absolute path",
        ))
    }
}

// ---------------------------------------------------------------------------
// Strict schema (spec §6, §6.2, §6.4)
// ---------------------------------------------------------------------------

fn default_timeout_seconds() -> u64 {
    180
}
fn default_max_question_bytes() -> u64 {
    65536
}
fn default_max_stdout_bytes() -> u64 {
    1_048_576
}
fn default_max_stderr_bytes() -> u64 {
    65536
}

/// `[runtime]` (spec §6). The whole table is optional (`Config::runtime` carries a
/// `#[serde(default)]`); each field independently defaults via its own
/// `default = "..."` function, so a partially-specified table still fills in the
/// missing fields individually rather than replacing the whole table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeConfig {
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
    #[serde(default = "default_max_question_bytes")]
    pub max_question_bytes: u64,
    #[serde(default = "default_max_stdout_bytes")]
    pub max_stdout_bytes: u64,
    #[serde(default = "default_max_stderr_bytes")]
    pub max_stderr_bytes: u64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            timeout_seconds: default_timeout_seconds(),
            max_question_bytes: default_max_question_bytes(),
            max_stdout_bytes: default_max_stdout_bytes(),
            max_stderr_bytes: default_max_stderr_bytes(),
        }
    }
}

/// `[providers.<agent>]` (spec §6). Table is optional per provider.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvidersConfig {
    #[serde(default)]
    pub claude: Option<ProviderConfig>,
    #[serde(default)]
    pub codex: Option<ProviderConfig>,
}

impl ProvidersConfig {
    fn table_for(&self, agent: Agent) -> Option<&ProviderConfig> {
        match agent {
            Agent::Claude => self.claude.as_ref(),
            Agent::Codex => self.codex.as_ref(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderConfig {
    #[serde(default)]
    pub executable: Option<String>,
}

/// `load` (spec §6.2). Exactly two supported modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadMode {
    ProjectSkill,
    LocalPlugin,
}

/// `[wikis.<id>.<agent>]` (spec §6.2). Field requiredness beyond `load`/`entrypoint`
/// depends on `load` and is checked semantically, not structurally.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderWikiConfig {
    pub load: LoadMode,
    pub entrypoint: String,
    #[serde(default)]
    pub skill_path: Option<String>,
    #[serde(default)]
    pub plugin_dir: Option<String>,
}

impl ProviderWikiConfig {
    fn validate_shape(&self) -> Result<(), AppError> {
        match self.load {
            LoadMode::ProjectSkill => {
                if self.skill_path.is_none() {
                    return Err(config_invalid("project_skill requires skill_path"));
                }
                if self.plugin_dir.is_some() {
                    return Err(config_invalid("plugin_dir is only valid for local_plugin"));
                }
            }
            LoadMode::LocalPlugin => {
                if self.plugin_dir.is_none() {
                    return Err(config_invalid("local_plugin requires plugin_dir"));
                }
                if self.skill_path.is_none() {
                    return Err(config_invalid("local_plugin requires skill_path"));
                }
            }
        }
        Ok(())
    }
}

/// `[wikis.<id>]` (spec §6). No `query_profiles` table exists in 0.2 — this struct
/// is the complete closed shape.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WikiConfig {
    pub title: String,
    pub project_root: String,
    pub content_root: String,
    pub agents: Vec<Agent>,
    pub query_prompt: String,
    #[serde(default)]
    pub claude: Option<ProviderWikiConfig>,
    #[serde(default)]
    pub codex: Option<ProviderWikiConfig>,
}

impl WikiConfig {
    fn provider_table(&self, agent: Agent) -> Option<&ProviderWikiConfig> {
        match agent {
            Agent::Claude => self.claude.as_ref(),
            Agent::Codex => self.codex.as_ref(),
        }
    }

    fn validate(&self, providers: &ProvidersConfig) -> Result<(), AppError> {
        if self.title.trim().is_empty() {
            return Err(config_invalid("title must not be empty"));
        }
        if self.project_root.trim().is_empty() {
            return Err(config_invalid("project_root must not be empty"));
        }
        if self.content_root.trim().is_empty() {
            return Err(config_invalid("content_root must not be empty"));
        }
        if self.agents.is_empty() {
            return Err(config_invalid("agents must be a non-empty array"));
        }
        if !agents_are_unique(&self.agents) {
            return Err(config_invalid("agents must not contain duplicates"));
        }
        validate_query_prompt(&self.query_prompt)?;

        for agent in &self.agents {
            if providers.table_for(*agent).is_none() {
                return Err(AppError::new(
                    ErrorCode::ProviderConfigMissing,
                    format!(
                        "agent {} is enabled without a global [providers.{}] table",
                        agent_key(*agent),
                        agent_key(*agent)
                    ),
                ));
            }
            let per_wiki = self.provider_table(*agent).ok_or_else(|| {
                config_invalid(format!(
                    "agent {} is enabled without a [wikis.<id>.{}] table",
                    agent_key(*agent),
                    agent_key(*agent)
                ))
            })?;
            per_wiki.validate_shape()?;
            validate_entrypoint(*agent, &per_wiki.entrypoint)?;
        }
        Ok(())
    }
}

fn agents_are_unique(agents: &[Agent]) -> bool {
    for i in 0..agents.len() {
        for j in (i + 1)..agents.len() {
            if agents[i] == agents[j] {
                return false;
            }
        }
    }
    true
}

fn agent_key(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "claude",
        Agent::Codex => "codex",
    }
}

/// The complete strict configuration document (spec §6). No `[query_profiles]`
/// table exists in 0.2 — an unknown top-level key of that (or any other) name is
/// rejected by `deny_unknown_fields`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub config_version: u32,
    #[serde(default)]
    pub default_agent: Option<Agent>,
    #[serde(default)]
    pub providers: ProvidersConfig,
    #[serde(default)]
    pub runtime: RuntimeConfig,
    #[serde(default)]
    pub wikis: BTreeMap<String, WikiConfig>,
}

impl Config {
    /// Semantic validation performed only after successful syntactic
    /// deserialization (plan Task 5 Step 8).
    pub fn validate(&self) -> Result<(), AppError> {
        if self.config_version != 1 {
            return Err(config_invalid("config_version must equal 1"));
        }
        if let Some(ProviderConfig {
            executable: Some(exe),
        }) = &self.providers.claude
        {
            validate_executable(exe)?;
        }
        if let Some(ProviderConfig {
            executable: Some(exe),
        }) = &self.providers.codex
        {
            validate_executable(exe)?;
        }
        for (id, wiki) in &self.wikis {
            validate_wiki_id(id)?;
            wiki.validate(&self.providers)?;
        }
        Ok(())
    }

    /// Reads, parses, and semantically validates a configuration file in one step.
    pub fn load(path: &Path) -> Result<Config, AppError> {
        let text = fs::read_to_string(path)
            .map_err(|e| config_invalid(format!("cannot read configuration file: {e}")))?;
        Self::load_str(&text)
    }

    /// Parses and validates configuration text directly (no filesystem access).
    pub fn load_str(text: &str) -> Result<Config, AppError> {
        let config: Config = toml::from_str(text)
            .map_err(|e| config_invalid(format!("configuration is not valid TOML: {e}")))?;
        config.validate()?;
        Ok(config)
    }
}

fn config_invalid(message: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::ConfigInvalid, message)
}

fn validate_wiki_id(id: &str) -> Result<(), AppError> {
    let is_valid = !id.is_empty()
        && id.split('-').all(|seg| {
            !seg.is_empty()
                && seg
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        });
    if is_valid {
        Ok(())
    } else {
        Err(config_invalid(format!(
            "wiki id {id:?} must match [a-z0-9]+(-[a-z0-9]+)*"
        )))
    }
}

/// `query_prompt` constraints (spec §6.4): required, single line, no control
/// characters, at most 500 UTF-8 bytes.
pub fn validate_query_prompt(prompt: &str) -> Result<(), AppError> {
    if prompt.is_empty() {
        return Err(config_invalid("query_prompt must not be empty"));
    }
    if prompt.chars().any(|c| c.is_control()) {
        return Err(config_invalid(
            "query_prompt must be a single line with no control characters",
        ));
    }
    if prompt.len() > 500 {
        return Err(config_invalid(
            "query_prompt must be at most 500 UTF-8 bytes",
        ));
    }
    Ok(())
}

const EXECUTABLE_SHELL_METACHARACTERS: &[char] = &[
    ';', '|', '&', '$', '`', '"', '\'', '<', '>', '(', ')', '{', '}', '[', ']', '*', '?', '~', '!',
    '#',
];

/// A provider `executable` is one bare command name or one absolute path; never
/// arguments, shell syntax, or a relative path component (spec §6.1).
pub fn validate_executable(value: &str) -> Result<(), AppError> {
    if value.is_empty() {
        return Err(config_invalid("provider executable must not be empty"));
    }
    if value.chars().any(|c| c.is_control()) {
        return Err(config_invalid(
            "provider executable must not contain control characters",
        ));
    }
    let has_separator = value.contains('/') || value.contains('\\');
    if has_separator {
        if !Path::new(value).is_absolute() {
            return Err(config_invalid(
                "provider executable path components are rejected; use one absolute path",
            ));
        }
        if value
            .chars()
            .any(|c| EXECUTABLE_SHELL_METACHARACTERS.contains(&c))
        {
            return Err(config_invalid(
                "provider executable must not contain shell metacharacters",
            ));
        }
    } else {
        if value.chars().any(|c| c.is_whitespace()) {
            return Err(config_invalid(
                "provider executable must be one command name, not arguments",
            ));
        }
        if !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
        {
            return Err(config_invalid(
                "provider executable command name contains invalid characters",
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Entrypoint validation (spec §6.3)
// ---------------------------------------------------------------------------

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-'
}

fn is_valid_name(s: &str) -> bool {
    !s.is_empty() && s.chars().all(is_name_char)
}

fn entrypoint_invalid(message: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::EntrypointInvalid, message)
}

/// Validates entrypoint syntax only (spec §6.3). Never appends a namespace,
/// renames, or infers anything from `skill_path` — this function's only inputs
/// are the agent and the entrypoint string as configured.
pub fn validate_entrypoint(agent: Agent, entrypoint: &str) -> Result<(), AppError> {
    if entrypoint.is_empty() {
        return Err(entrypoint_invalid("entrypoint must not be empty"));
    }
    if entrypoint
        .chars()
        .any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(entrypoint_invalid(
            "entrypoint must be a single token with no whitespace or control characters",
        ));
    }
    match agent {
        Agent::Claude => {
            let rest = entrypoint
                .strip_prefix('/')
                .ok_or_else(|| entrypoint_invalid("claude entrypoint must start with /"))?;
            match rest.split_once(':') {
                Some((plugin, skill)) => {
                    if rest.matches(':').count() == 1
                        && is_valid_name(plugin)
                        && is_valid_name(skill)
                    {
                        Ok(())
                    } else {
                        Err(entrypoint_invalid(
                            "claude plugin entrypoint must be /plugin-name:skill-name with exactly one colon",
                        ))
                    }
                }
                None => {
                    if is_valid_name(rest) {
                        Ok(())
                    } else {
                        Err(entrypoint_invalid("claude entrypoint must be /name"))
                    }
                }
            }
        }
        Agent::Codex => {
            let rest = entrypoint
                .strip_prefix('$')
                .ok_or_else(|| entrypoint_invalid("codex entrypoint must start with $"))?;
            if rest.contains(':') {
                return Err(entrypoint_invalid(
                    "codex $plugin-name:skill-name entrypoints are reserved and rejected in 0.1.0",
                ));
            }
            if is_valid_name(rest) {
                Ok(())
            } else {
                Err(entrypoint_invalid("codex entrypoint must be $name"))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Path resolution, containment, and special-entry scanning (spec §6.1)
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn is_special_entry(md: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    md.file_type().is_symlink() || (md.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT) != 0
}

#[cfg(not(windows))]
fn is_special_entry(md: &fs::Metadata) -> bool {
    md.file_type().is_symlink()
}

fn check_not_special(path: &Path) -> Result<(), AppError> {
    match fs::symlink_metadata(path) {
        Ok(md) => {
            if is_special_entry(&md) {
                Err(AppError::new(
                    ErrorCode::UnsafeFilesystemEntry,
                    "configured path contains a symlink, junction, reparse point, or mount point",
                ))
            } else {
                Ok(())
            }
        }
        // Not present yet at this component; canonicalize() below reports the
        // real existence error with the caller's chosen error code.
        Err(_) => Ok(()),
    }
}

/// Joins `configured` onto `anchor` (or uses it directly if already absolute),
/// checking every component of `configured` itself (not `anchor`'s own ancestry,
/// which was already checked when `anchor` itself was resolved) for special
/// status. Does not require existence; canonicalization/existence is the
/// caller's concern since the correct error code differs by context.
fn joined_checked(anchor: &Path, configured: &str) -> Result<PathBuf, AppError> {
    if configured.trim().is_empty() {
        return Err(config_invalid("path must not be empty"));
    }
    let configured_path = Path::new(configured);
    let mut prefixes: Vec<&Path> = configured_path.ancestors().collect();
    prefixes.reverse();
    if configured_path.is_absolute() {
        for prefix in &prefixes {
            check_not_special(prefix)?;
        }
        Ok(configured_path.to_path_buf())
    } else {
        for prefix in &prefixes {
            check_not_special(&anchor.join(prefix))?;
        }
        Ok(anchor.join(configured_path))
    }
}

/// Recursively scans `root` for special filesystem entries, excluding any
/// directory named in `exclude_top_level` that is an immediate child of `root`
/// (spec §6.1: the `.claude`/`.agents` exclusion applies only at the content-root
/// scan and only to its own immediate children).
fn scan_tree(root: &Path, exclude_top_level: &[&str]) -> Result<(), AppError> {
    scan_dir(root, root, exclude_top_level)
}

fn scan_dir(dir: &Path, root: &Path, exclude_top_level: &[&str]) -> Result<(), AppError> {
    let entries = fs::read_dir(dir)
        .map_err(|e| config_invalid(format!("cannot scan configured directory: {e}")))?;
    for entry in entries {
        let entry =
            entry.map_err(|e| config_invalid(format!("cannot scan configured directory: {e}")))?;
        let path = entry.path();
        if dir == root
            && let Some(name) = path.file_name().and_then(|n| n.to_str())
            && exclude_top_level.contains(&name)
        {
            continue;
        }
        let md = fs::symlink_metadata(&path)
            .map_err(|e| config_invalid(format!("cannot inspect configured path: {e}")))?;
        if is_special_entry(&md) {
            return Err(AppError::new(
                ErrorCode::UnsafeFilesystemEntry,
                "monitored tree contains a symlink, junction, reparse point, or mount point",
            ));
        }
        if md.is_dir() {
            scan_dir(&path, root, exclude_top_level)?;
        }
    }
    Ok(())
}

/// Canonical, containment-checked `project_root`/`content_root` for one wiki
/// (spec §6.1, plan Task 5 Step 4). Both resolve relative to `config_dir`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedWikiRoots {
    pub project_root: PathBuf,
    pub content_root: PathBuf,
}

pub fn resolve_wiki_roots(
    config_dir: &Path,
    wiki: &WikiConfig,
) -> Result<ResolvedWikiRoots, AppError> {
    let project_root_joined = joined_checked(config_dir, &wiki.project_root)?;
    let project_root = fs::canonicalize(&project_root_joined)
        .map_err(|e| config_invalid(format!("project_root cannot be resolved: {e}")))?;
    if !project_root.is_dir() {
        return Err(config_invalid("project_root must be a directory"));
    }

    let content_root_joined = joined_checked(config_dir, &wiki.content_root)?;
    let content_root = fs::canonicalize(&content_root_joined)
        .map_err(|e| config_invalid(format!("content_root cannot be resolved: {e}")))?;
    if !content_root.is_dir() {
        return Err(config_invalid("content_root must be a directory"));
    }
    if !content_root.starts_with(&project_root) {
        return Err(AppError::new(
            ErrorCode::PathOutsideAllowedRoot,
            "content_root must be contained by project_root",
        ));
    }

    scan_tree(&content_root, &[".claude", ".agents"])?;

    Ok(ResolvedWikiRoots {
        project_root,
        content_root,
    })
}

// ---------------------------------------------------------------------------
// Statically addressable artifacts (spec §8.1 step 6, §15.1)
// ---------------------------------------------------------------------------

fn require_regular_file(path: &Path, what: &str) -> Result<PathBuf, AppError> {
    let canonical = fs::canonicalize(path)
        .map_err(|_| entrypoint_invalid(format!("{what} is not statically addressable")))?;
    let md = fs::metadata(&canonical)
        .map_err(|_| entrypoint_invalid(format!("{what} is not statically addressable")))?;
    if !md.is_file() {
        return Err(entrypoint_invalid(format!("{what} is not a regular file")));
    }
    Ok(canonical)
}

fn require_dir(path: &Path, what: &str) -> Result<PathBuf, AppError> {
    let canonical = fs::canonicalize(path)
        .map_err(|_| entrypoint_invalid(format!("{what} is not statically addressable")))?;
    let md = fs::metadata(&canonical)
        .map_err(|_| entrypoint_invalid(format!("{what} is not statically addressable")))?;
    if !md.is_dir() {
        return Err(entrypoint_invalid(format!("{what} is not a directory")));
    }
    Ok(canonical)
}

/// Confirms the configured `project_skill`/`local_plugin` artifacts exist as the
/// shapes their load mode requires, before any provider invocation (spec §8.1
/// step 6; failures are `ENTRYPOINT_INVALID`). Also applies the unconditional,
/// unexcluded special-entry scan to the selected skill/plugin artifact tree
/// (spec §6.1).
pub fn resolve_and_check_artifact(
    config_dir: &Path,
    project_root: &Path,
    provider: &ProviderWikiConfig,
) -> Result<(), AppError> {
    match provider.load {
        LoadMode::ProjectSkill => {
            let skill_path_str = provider
                .skill_path
                .as_deref()
                .ok_or_else(|| entrypoint_invalid("project_skill requires skill_path"))?;
            let joined = joined_checked(project_root, skill_path_str)?;
            let skill_file = require_regular_file(&joined, "configured project skill")?;
            let skill_dir = skill_file
                .parent()
                .ok_or_else(|| entrypoint_invalid("skill_path has no parent directory"))?;
            scan_tree(skill_dir, &[])
        }
        LoadMode::LocalPlugin => {
            let plugin_dir_str = provider
                .plugin_dir
                .as_deref()
                .ok_or_else(|| entrypoint_invalid("local_plugin requires plugin_dir"))?;
            let plugin_joined = joined_checked(config_dir, plugin_dir_str)?;
            let plugin_dir = require_dir(&plugin_joined, "configured local plugin directory")?;

            let manifest = plugin_dir.join(".claude-plugin").join("plugin.json");
            require_regular_file(&manifest, "local plugin manifest")?;

            let skill_path_str = provider
                .skill_path
                .as_deref()
                .ok_or_else(|| entrypoint_invalid("local_plugin requires skill_path"))?;
            let skill_joined = joined_checked(&plugin_dir, skill_path_str)?;
            require_regular_file(&skill_joined, "configured plugin skill")?;

            scan_tree(&plugin_dir, &[])
        }
    }
}

/// The single top-level `.claude/settings.json`/`settings.local.json` key
/// admitted at a wiki's `project_root` (spec §6.1/§12 R-29, narrowed by
/// R-30). Everything else fails closed — see
/// [`check_claude_wiki_settings_surface`] for why this is an allowlist, not
/// a denylist, and why `permissions` was removed from it.
const ALLOWED_WIKI_SETTINGS_KEYS: &[&str] = &["enabledPlugins"];

/// Denies a Claude wiki whose `project_root/.claude/settings.json` or
/// `settings.local.json` declares any executable or reach-widening surface
/// (spec §6.1/§12/§15 R-29/R-30; mirrors the pre-existing `local_plugin`
/// lifecycle-component rejection in [`crate::probes::check_no_plugin_lifecycle_components`],
/// which already denies a *plugin's* hooks/MCP/settings by the same
/// principle — this extends it to the wiki's own project settings).
///
/// **Why this check exists at all**: Claude Code's `-p` (print) mode loads
/// `project_root/.claude/settings.json`/`settings.local.json` from whatever
/// directory it is launched in, *regardless of trust*, and several
/// documented keys beyond hooks execute a command or widen reach —
/// `apiKeyHelper`, `awsCredentialExport`, `awsAuthRefresh`,
/// `gcpAuthRefresh`, `otelHeadersHelper`, and `statusLine` all run a
/// configured command; `permissions.additionalDirectories` widens the tool
/// read/search surface beyond `project_root`; `env` can redirect API traffic
/// (e.g. `ANTHROPIC_BASE_URL`). `--settings {"disableAllHooks":true}` (spec
/// §10.2 R-27/R-28) only disables the `hooks` key — it does not bound any of
/// these. Confirmed live: a wiki declaring `apiKeyHelper` as a command
/// executed it even with `--settings {"disableAllHooks":true}` present
/// (`docs/verification/llm-wikis-execution.md`, Task 15 "review loop
/// iteration 3"). Any command such a key runs under `.claude/`/`.agents/`
/// is invisible to the mutation snapshot (spec §12 excludes those
/// directories, covered instead by `skill_fingerprint`), so detection after
/// the fact cannot be relied on — this must be a preflight gate.
///
/// **`permissions` was admitted in R-29 and removed in R-30**: R-29 allowed
/// `permissions.{allow,deny,defaultMode}` reasoning that `--tools
/// Read,Grep,Glob` bounds *tool names*, which is true but insufficient — a
/// permission rule such as `{"permissions":{"allow":["Read(/some/path/**)"]}}`
/// pre-authorizes the exposed `Read` tool against a specific path pattern,
/// which is not a tool-name concern at all. Rather than enumerate a safe
/// subset of permission-rule *values* (an open-ended, error-prone parsing
/// problem), `permissions` is denied entirely, key and values — consistent
/// with the deny-by-default posture below.
///
/// **Allowlist, not denylist, deliberately**: a denylist survives only the
/// keys enumerated today; a future Claude Code release could add another
/// executable/reach-widening setting this project has never heard of, and a
/// denylist would silently admit it. `enabledPlugins` is the sole admitted
/// key. It is admissible because plugins resolve from the *operator's own*
/// user-scope marketplace configuration (`~/.claude`), which a wiki's
/// project settings cannot add to or redirect — `enabledPlugins` only
/// toggles on/off a plugin the operator already trusted at the user level;
/// it cannot introduce a new, wiki-supplied plugin source. Any hooks that
/// enabled plugin itself declares are still killed by `--settings
/// {"disableAllHooks":true}` regardless. The real `harness-engineering`
/// wiki has exactly `{"enabledPlugins":{"llm-wiki@llm-wiki":true}}` and
/// resolves its entrypoint through the `project_skill` artifact regardless
/// (R-28's live four-arm experiment) — this key carries no execution or
/// reach-widening capability of its own, only a boolean toggle over
/// operator-trusted state. Its *value* is still validated below (every
/// entry must be a plain boolean) so this key cannot become a smuggling
/// vector for some other shape in a future settings-format change.
///
/// A missing settings file is not an error (most wikis have none). A
/// present-but-unparseable-as-a-JSON-object file fails closed rather than
/// being silently skipped, since this project cannot verify it is safe. A
/// settings path that is a symlink, junction, reparse point, directory, or
/// any other non-regular-file entry is rejected as `UNSAFE_FILESYSTEM_ENTRY`
/// rather than treated as absent — a dangling symlink resolves as "not
/// found" under a plain existence check, letting its target be created
/// *after* this check passes and *before* the provider reads it; `.claude/`
/// is excluded from the recursive special-entry scan elsewhere (spec §6.1),
/// so nothing else would ever catch this.
pub fn check_claude_wiki_settings_surface(project_root: &Path) -> Result<(), AppError> {
    for name in ["settings.json", "settings.local.json"] {
        let path = project_root.join(".claude").join(name);
        let md = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => {
                return Err(entrypoint_invalid(format!(
                    "wiki .claude/{name} could not be read: {e}"
                )));
            }
        };
        if is_special_entry(&md) {
            return Err(AppError::new(
                ErrorCode::UnsafeFilesystemEntry,
                format!(
                    "wiki .claude/{name} is a symlink, junction, reparse point, or mount point"
                ),
            ));
        }
        if !md.is_file() {
            return Err(entrypoint_invalid(format!(
                "wiki .claude/{name} is not a regular file"
            )));
        }
        let text = fs::read_to_string(&path).map_err(|e| {
            entrypoint_invalid(format!("wiki .claude/{name} could not be read: {e}"))
        })?;
        let value: serde_json::Value = serde_json::from_str(&text).map_err(|_| {
            entrypoint_invalid(format!(
                "wiki .claude/{name} is not valid JSON and cannot be safety-checked"
            ))
        })?;
        let obj = value.as_object().ok_or_else(|| {
            entrypoint_invalid(format!("wiki .claude/{name} is not a JSON object"))
        })?;
        for (key, val) in obj {
            if !ALLOWED_WIKI_SETTINGS_KEYS.contains(&key.as_str()) {
                return Err(entrypoint_invalid(format!(
                    "wiki .claude/{name} declares a rejected settings key: {key}"
                )));
            }
            // key == "enabledPlugins": validate its shape rather than trust
            // it unconditionally, so this admitted key cannot itself become
            // a smuggling vector for an unexpected value shape.
            let plugins = val.as_object().ok_or_else(|| {
                entrypoint_invalid(format!(
                    "wiki .claude/{name}'s enabledPlugins is not an object"
                ))
            })?;
            for (plugin_name, plugin_value) in plugins {
                if !plugin_value.is_boolean() {
                    return Err(entrypoint_invalid(format!(
                        "wiki .claude/{name}'s enabledPlugins.{plugin_name} value is not a boolean"
                    )));
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// `config init` (spec §5.1/§5.2)
// ---------------------------------------------------------------------------

/// The exact starter document `config init` writes: runtime and provider
/// defaults, an empty wiki registry, and a commented placeholder example. Never
/// the two-wiki registry from `config.example.toml` — that file documents the
/// real contract; this is what a fresh install actually gets.
pub const INIT_TEMPLATE: &str = r#"config_version = 1
default_agent = "claude"

[providers.claude]
executable = "claude"

[providers.codex]
executable = "codex"

[runtime]
timeout_seconds    = 180
max_question_bytes = 65536
max_stdout_bytes   = 1048576
max_stderr_bytes   = 65536

# Example wiki (edit the paths and prompt, then uncomment to register it):
#
# [wikis.example]
# title        = "Example Knowledge Base"
# project_root = "/absolute/path/to/example"
# content_root = "/absolute/path/to/example"
# agents       = ["claude"]
# query_prompt = "Use the wiki-query skill to answer from this wiki."
#
# [wikis.example.claude]
# load       = "project_skill"
# entrypoint = "/wiki-query"
# skill_path = ".claude/skills/wiki-query/SKILL.md"
"#;

/// Result of a `config init` attempt (path always known; `created` only true on
/// an actual exclusive-create write).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigInitOutcome {
    pub path: PathBuf,
    pub created: bool,
}

/// Exclusive-create only: never overwrites, never merges, never reads the
/// caller's current working directory for discovery.
pub fn init(path: &Path) -> Result<ConfigInitOutcome, AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| config_invalid(format!("cannot create configuration directory: {e}")))?;
    }
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => {
            file.write_all(INIT_TEMPLATE.as_bytes())
                .map_err(|e| config_invalid(format!("cannot write configuration file: {e}")))?;
            Ok(ConfigInitOutcome {
                path: path.to_path_buf(),
                created: true,
            })
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Err(AppError::new(
            ErrorCode::ConfigExists,
            "configuration file already exists",
        )),
        Err(e) => Err(config_invalid(format!(
            "cannot create configuration file: {e}"
        ))),
    }
}

/// The exact `operation: "config_init"` public envelope (spec plan Task 5 Step 6).
#[derive(Debug, Clone, Serialize)]
pub struct ConfigInitEnvelope {
    pub schema_version: &'static str,
    pub ok: bool,
    pub operation: &'static str,
    pub path: String,
    pub created: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<AppError>,
}

pub fn init_envelope(path: &Path) -> ConfigInitEnvelope {
    match init(path) {
        Ok(outcome) => ConfigInitEnvelope {
            schema_version: SCHEMA_VERSION,
            ok: true,
            operation: "config_init",
            path: outcome.path.display().to_string(),
            created: outcome.created,
            error: None,
        },
        Err(err) => ConfigInitEnvelope {
            schema_version: SCHEMA_VERSION,
            ok: false,
            operation: "config_init",
            path: path.display().to_string(),
            created: false,
            error: Some(err),
        },
    }
}

//! Public CLI: `clap` argument model and dispatch (spec §5.1-§5.3; plan Task 12).
//!
//! `main.rs` stays a thin boundary; every argument-shape decision, config-path
//! resolution, and envelope-rendering choice lives here. Argument parsing
//! failures (missing/invalid flags, unknown subcommands, bad `--agent`
//! values) are converted to `ARGUMENT_INVALID` and rendered through the same
//! one-JSON-document contract as any other failure (spec §14: "A failed
//! invocation still emits exactly one JSON envelope in `--json` mode").
//! `--help`/`--version` still print to stdout and exit 0 via clap's own
//! handling — those are not failures.
//!
//! Nothing here reads the caller's current working directory for config or
//! wiki discovery (spec §5.2): the config path is either the explicit
//! absolute `--config` override or the platform-native default path derived
//! from environment variables (`crate::config::default_config_path`), and
//! every wiki path resolves relative to the config file's own directory
//! (`crate::query`/`crate::doctor`, via `config_dir`).

use std::ffi::OsString;
use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};

use clap::error::ErrorKind;
use clap::{Parser, Subcommand, ValueEnum};

use crate::config::{
    Config, ConfigInitEnvelope, ConfigListEnvelope, ConfigValidateEnvelope, Platform, ProcessEnv,
    config_list_envelope, config_list_error_envelope, config_validate_envelope,
    config_validate_error_envelope, default_cache_path, default_config_path, init_envelope,
    validate_config_override,
};
use crate::doctor::{
    CheckStatus, DoctorEnvelope, DoctorRequest, ListEnvelope, doctor_error_envelope,
    list_error_envelope, run_doctor, run_list,
};
use crate::error::{AppError, ErrorCode, dominant_exit};
use crate::output::{Agent, QueryEnvelope, SCHEMA_VERSION, render_human, render_json};
use crate::probes::{FileProbeStore, QueryMode};
use crate::providers::RealProcessRunner;
use crate::query::{QueryRequest, QueryService};

// ---------------------------------------------------------------------------
// Argument model (spec §5.1)
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(name = "llm-wikis", version)]
struct Cli {
    /// Absolute path to the configuration file (overrides the platform default).
    // Absolute-only override for the platform-native config path (spec
    // §5.2). Rejected (as `ARGUMENT_INVALID`) when relative.
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,
    /// Emit exactly one JSON document instead of human-readable text.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: CliCommand,
}

#[derive(Subcommand)]
enum CliCommand {
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    List,
    Doctor {
        #[arg(long)]
        wiki: Option<String>,
        #[arg(long)]
        agent: Option<AgentArg>,
        #[arg(long)]
        live: bool,
    },
    Query {
        /// The wiki ID to query (required, exactly one).
        // Collected via `ArgAction::Append` (never silently overwritten by a
        // repeat) so a repeated `--wiki` is *detectable* after parsing,
        // rather than clap's default single-value behavior of quietly
        // keeping only the last occurrence (spec §5.1: "accepts one and
        // only one `--wiki`. Repeating it ... is an argument error").
        #[arg(long = "wiki", action = clap::ArgAction::Append, value_name = "ID")]
        wiki: Vec<String>,
        #[arg(long)]
        agent: Option<AgentArg>,
        /// The question text, supplied after a literal `--` separator.
        // `last = true` is what makes clap require that separator at all —
        // without it this would just be an ordinary (and, without `--`,
        // earlier-consumed) positional (spec §5.1).
        #[arg(last = true)]
        question: Option<String>,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    Init,
    /// Print the resolved configuration.
    // "Resolved" means after serde's own field-level defaulting
    // (`Config::load`'s output as-is) — never additionally
    // filesystem-canonicalized per wiki, which stays `doctor`'s job. clap
    // derives this variant's kebab-case name as `list`, which is
    // unambiguous alongside the pre-existing top-level `CliCommand::List`
    // (the wiki registry listing) because the two live at different
    // subcommand depths: `llm-wikis list` versus `llm-wikis config list` —
    // clap requires the full `config` prefix to reach this one, so there is
    // no parse-time collision.
    List,
    /// Validate the configuration without side effects.
    // `doctor` covers environment/provider verification and is unchanged by
    // this task.
    Validate,
}

/// The agent to use: `claude` or `codex`.
// A CLI-only mirror of [`Agent`] purely so `clap::ValueEnum` can be derived
// here without adding a clap dependency to `src/output.rs`. Variant names
// already lower-case to `claude`/`codex` under clap's default kebab-case
// rule, matching the CLI surface exactly.
#[derive(Clone, Copy, Debug, ValueEnum)]
enum AgentArg {
    Claude,
    Codex,
}

impl From<AgentArg> for Agent {
    fn from(value: AgentArg) -> Self {
        match value {
            AgentArg::Claude => Agent::Claude,
            AgentArg::Codex => Agent::Codex,
        }
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub fn run() -> i32 {
    run_from(std::env::args_os().collect())
}

fn run_from(args: Vec<OsString>) -> i32 {
    match Cli::try_parse_from(args.iter().cloned()) {
        Ok(cli) => dispatch(cli),
        Err(e) => handle_parse_error(&e, &args),
    }
}

/// clap parse failures (unknown subcommand, missing required flag, invalid
/// `--agent` value, ...) never reach a specific subcommand handler, so the
/// operation-specific envelope shapes (list/doctor/config_init) are not
/// available here. `--help`/`--version` are not failures at all — clap's own
/// `DisplayHelp`/`DisplayVersion` kinds print to stdout and exit 0 exactly as
/// `--version` alone must (plan Task 3). Every other kind becomes
/// `ARGUMENT_INVALID`, rendered as the generic query-shaped envelope (the
/// most complete public shape, with the nullable `wiki`/`agent` fields the
/// spec discusses for argument failures preceding resolution) so `--json`
/// still gets exactly one JSON document even when no subcommand was
/// successfully identified.
fn handle_parse_error(e: &clap::Error, args: &[OsString]) -> i32 {
    match e.kind() {
        ErrorKind::DisplayHelp
        | ErrorKind::DisplayVersion
        | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
            let _ = e.print();
            0
        }
        _ => {
            let json_mode = args.iter().any(|a| a == "--json");
            let mut message = first_line(&e.to_string());
            // PRD 08-07-first-run-config-and-query-ux-fixes D4: a bare
            // `llm-wikis query "question"` (missing the required `--`
            // separator, spec §5.1) reaches clap's generic
            // "unexpected argument" wording with no guidance toward the
            // correct shape. Scoped to the `query` subcommand's own usage
            // line (rather than every `UnknownArgument`/etc. clap failure
            // everywhere) by checking clap's own rendered `Usage:` line for
            // the `query` token -- clap always renders the usage for the
            // deepest subcommand matcher that was active when parsing
            // failed, so this only fires for a `query`-scoped parse error.
            if usage_line_mentions_query(e) {
                message.push_str(&format!(" ({QUERY_USAGE_HINT})"));
            }
            let err = AppError::new(ErrorCode::ArgumentInvalid, message);
            emit_query(json_mode, query_failure_envelope(None, None, err))
        }
    }
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or(s).trim().to_string()
}

/// The correct invocation shape, shown alongside `query`'s own argument
/// errors (PRD 08-07-first-run-config-and-query-ux-fixes D4). Guidance only
/// -- the `--` requirement itself and every other parsing rule is unchanged.
const QUERY_USAGE_HINT: &str = "try: llm-wikis query --wiki <id> -- \"<question>\"";

/// True when clap's own rendered error carries a `Usage:` line naming the
/// `query` subcommand -- i.e. the failure happened while matching arguments
/// *within* `query`, not some other subcommand or the root command. clap
/// always renders the usage line for the deepest matcher active at failure
/// time, so this is a precise (not merely `args`-token-scanning) test.
fn usage_line_mentions_query(e: &clap::Error) -> bool {
    e.to_string()
        .lines()
        .any(|l| l.trim_start().starts_with("Usage:") && l.contains(" query "))
}

fn dispatch(cli: Cli) -> i32 {
    let json = cli.json;
    let config_override = cli.config.as_deref();
    match cli.command {
        CliCommand::Config { action } => match action {
            ConfigAction::Init => run_config_init(json, config_override),
            ConfigAction::List => run_config_list(json, config_override),
            ConfigAction::Validate => run_config_validate(json, config_override),
        },
        CliCommand::List => run_list_command(json, config_override),
        CliCommand::Doctor { wiki, agent, live } => {
            run_doctor_command(json, config_override, wiki, agent.map(Agent::from), live)
        }
        CliCommand::Query {
            wiki,
            agent,
            question,
        } => run_query_command(
            json,
            config_override,
            wiki,
            agent.map(Agent::from),
            question,
        ),
    }
}

// ---------------------------------------------------------------------------
// Shared config/cache path resolution (spec §5.2, §15.1)
// ---------------------------------------------------------------------------

/// Resolves the config path this invocation should use: the absolute
/// `--config` override (validated), or the platform-native default derived
/// purely from environment variables — never the caller's current working
/// directory (spec §5.2).
fn resolve_config_path(override_path: Option<&Path>) -> Result<PathBuf, AppError> {
    match override_path {
        Some(p) => {
            validate_config_override(p)?;
            Ok(p.to_path_buf())
        }
        None => default_config_path(Platform::host(), &ProcessEnv).ok_or_else(|| {
            AppError::new(
                ErrorCode::ConfigInvalid,
                "cannot determine the platform configuration path: a required environment variable is unset",
            )
        }),
    }
}

fn resolve_cache_path() -> Result<PathBuf, AppError> {
    default_cache_path(Platform::host(), &ProcessEnv).ok_or_else(|| {
        AppError::new(
            ErrorCode::InternalError,
            "cannot determine the platform probe-cache path: a required environment variable is unset",
        )
    })
}

/// Every configured wiki path resolves relative to the config file's own
/// directory (spec §6.1), never the caller's cwd.
fn config_dir_of(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn render_json_generic<T: serde::Serialize>(value: &T) -> String {
    let mut rendered = serde_json::to_string(value).expect("public envelope always serializes");
    rendered.push('\n');
    rendered
}

/// The one human-readable rendering of a top-level command failure (spec
/// §5.1/§5.3, PRD 08-06-pre-0-1-0-cli-refinements D2/AC3): always stderr,
/// never stdout, so a pipe/redirect/CI consumer of stdout never sees error
/// text mixed into whatever partial human output preceded it. `--json` mode
/// never calls this — the failing envelope's `error` object is already part
/// of the single JSON document the `emit_*`/`run_config_*` functions write
/// to stdout instead. The single source of truth for the format string,
/// replacing what used to be five independently written
/// `println!("error: {} ({})", ...)` call sites.
fn eprint_error_line(err: &AppError) {
    eprintln!("error: {} ({})", err.code.as_str(), err.message);
}

fn error_code_from_str(code: &str) -> Option<ErrorCode> {
    ErrorCode::ALL.iter().copied().find(|c| c.as_str() == code)
}

// ---------------------------------------------------------------------------
// `config init` (spec §5.1)
// ---------------------------------------------------------------------------

fn run_config_init(json: bool, override_path: Option<&Path>) -> i32 {
    let envelope = match override_path {
        Some(p) => match validate_config_override(p) {
            Ok(()) => init_envelope(p),
            Err(err) => ConfigInitEnvelope {
                schema_version: SCHEMA_VERSION,
                ok: false,
                operation: "config_init",
                path: p.display().to_string(),
                created: false,
                error: Some(err),
            },
        },
        None => match default_config_path(Platform::host(), &ProcessEnv) {
            Some(p) => init_envelope(&p),
            None => ConfigInitEnvelope {
                schema_version: SCHEMA_VERSION,
                ok: false,
                operation: "config_init",
                path: String::new(),
                created: false,
                error: Some(AppError::new(
                    ErrorCode::ConfigInvalid,
                    "cannot determine the platform configuration path: a required environment variable is unset",
                )),
            },
        },
    };
    let exit = envelope
        .error
        .as_ref()
        .map(|e| e.code.exit_code())
        .unwrap_or(0);
    if json {
        print!("{}", render_json_generic(&envelope));
    } else if envelope.ok {
        println!("Created configuration at {}", envelope.path);
    } else if let Some(err) = &envelope.error {
        eprint_error_line(err);
    }
    exit as i32
}

// ---------------------------------------------------------------------------
// `config list` / `config validate` (PRD 08-06-pre-0-1-0-cli-refinements
// AC1, D7). `doctor` already covers environment verification (D1); these
// two subcommands are the remaining, narrower ask: print/validate the
// resolved configuration itself, without starting a provider. `config list`
// is a deliberately different command from the top-level `list` (which
// enumerates registered wikis, `print_list_human` below) — this one prints
// the whole resolved configuration document that produced that registry.
// ---------------------------------------------------------------------------

fn agent_lowercase(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "claude",
        Agent::Codex => "codex",
    }
}

fn run_config_list(json: bool, override_path: Option<&Path>) -> i32 {
    let envelope: ConfigListEnvelope = match resolve_config_path(override_path) {
        Ok(p) => config_list_envelope(&p),
        Err(e) => config_list_error_envelope(e),
    };
    let exit = envelope
        .error
        .as_ref()
        .map(|e| e.code.exit_code())
        .unwrap_or(0);
    if json {
        print!("{}", render_json_generic(&envelope));
    } else if let Some(config) = &envelope.config {
        print_config_list_human(config);
    } else if let Some(err) = &envelope.error {
        eprint_error_line(err);
    }
    exit as i32
}

fn print_config_list_human(config: &Config) {
    println!("config_version = {}", config.config_version);
    match config.default_agent {
        Some(a) => println!("default_agent = {}", agent_lowercase(a)),
        None => println!("default_agent = (none)"),
    }
    for (id, wiki) in &config.wikis {
        let agents: Vec<&'static str> = wiki.agents.iter().copied().map(agent_lowercase).collect();
        println!("{id} - {} [{}]", wiki.title, agents.join(","));
    }
}

fn run_config_validate(json: bool, override_path: Option<&Path>) -> i32 {
    let envelope: ConfigValidateEnvelope = match resolve_config_path(override_path) {
        Ok(p) => config_validate_envelope(&p),
        Err(e) => config_validate_error_envelope(e),
    };
    let exit = envelope
        .error
        .as_ref()
        .map(|e| e.code.exit_code())
        .unwrap_or(0);
    if json {
        print!("{}", render_json_generic(&envelope));
    } else if envelope.ok {
        println!("Configuration at {} is valid.", envelope.path);
    } else if let Some(err) = &envelope.error {
        eprint_error_line(err);
    }
    exit as i32
}

// ---------------------------------------------------------------------------
// `list` (spec §5.1, §5.3)
// ---------------------------------------------------------------------------

fn run_list_command(json: bool, override_path: Option<&Path>) -> i32 {
    let envelope = match resolve_config_path(override_path).and_then(|p| Config::load(&p)) {
        Ok(config) => run_list(&config),
        Err(e) => list_error_envelope(e),
    };
    let exit = envelope
        .error
        .as_ref()
        .map(|e| e.code.exit_code())
        .unwrap_or(0);
    if json {
        print!("{}", render_json_generic(&envelope));
    } else {
        print_list_human(&envelope);
    }
    exit as i32
}

fn print_list_human(envelope: &ListEnvelope) {
    if let Some(err) = &envelope.error {
        eprint_error_line(err);
        return;
    }
    for wiki in &envelope.wikis {
        println!("{} - {}", wiki.id, wiki.title);
    }
}

// ---------------------------------------------------------------------------
// `doctor` (spec §5.1, §5.3, §15)
// ---------------------------------------------------------------------------

fn run_doctor_command(
    json: bool,
    override_path: Option<&Path>,
    wiki: Option<String>,
    agent: Option<Agent>,
    live: bool,
) -> i32 {
    // Because live checks consume model quota, `--live` requires both
    // selectors explicitly (spec §5.1) — checked here, before `run_doctor`
    // is even called, so a registry that happens to resolve to exactly one
    // (wiki, agent) pair on its own can never make this requirement
    // optional.
    if live && (wiki.is_none() || agent.is_none()) {
        let err = AppError::new(
            ErrorCode::ArgumentInvalid,
            "doctor --live requires both --wiki and --agent",
        );
        return emit_doctor(json, doctor_error_envelope(live, err));
    }

    let config_path = match resolve_config_path(override_path) {
        Ok(p) => p,
        Err(e) => return emit_doctor(json, doctor_error_envelope(live, e)),
    };
    let config = match Config::load(&config_path) {
        Ok(c) => c,
        Err(e) => return emit_doctor(json, doctor_error_envelope(live, e)),
    };
    let cache_path = match resolve_cache_path() {
        Ok(p) => p,
        Err(e) => return emit_doctor(json, doctor_error_envelope(live, e)),
    };

    let request = DoctorRequest {
        config,
        config_dir: config_dir_of(&config_path),
        wiki,
        agent,
        live,
        temp_base: std::env::temp_dir(),
    };
    let store = FileProbeStore::new(cache_path);
    let envelope = run_doctor(request, RealProcessRunner, &store);
    emit_doctor(json, envelope)
}

fn emit_doctor(json: bool, envelope: DoctorEnvelope) -> i32 {
    let exit = doctor_exit_code(&envelope);
    if json {
        print!("{}", render_json_generic(&envelope));
    } else {
        print_doctor_human(&envelope);
    }
    exit as i32
}

/// The global error-to-exit mapping for a doctor matrix (spec §5.3): a
/// command-level failure uses its own code; otherwise the highest-precedence
/// exit class among every failed check wins (`70,7,6,5,4,3,2`, else `0`) —
/// exactly [`dominant_exit`]'s own ordering, reused rather than
/// reimplemented.
fn doctor_exit_code(envelope: &DoctorEnvelope) -> u8 {
    if let Some(err) = &envelope.error {
        return err.code.exit_code();
    }
    let codes: Vec<ErrorCode> = envelope
        .results
        .iter()
        .flat_map(|r| r.checks.iter())
        .filter(|c| c.status == CheckStatus::Fail)
        .filter_map(|c| c.code.as_deref())
        .filter_map(error_code_from_str)
        .collect();
    dominant_exit(&codes)
}

fn print_doctor_human(envelope: &DoctorEnvelope) {
    if let Some(err) = &envelope.error {
        eprint_error_line(err);
        return;
    }
    for result in &envelope.results {
        println!(
            "{} / {:?}: {}",
            result.wiki,
            result.agent,
            if result.ok { "ok" } else { "fail" }
        );
        for check in &result.checks {
            println!("  [{:?}] {}: {}", check.status, check.name, check.message);
        }
    }
}

// ---------------------------------------------------------------------------
// `query` (spec §5.1, §7.1, §13)
// ---------------------------------------------------------------------------

fn query_failure_envelope(
    wiki: Option<crate::output::WikiRef>,
    agent: Option<Agent>,
    error: AppError,
) -> QueryEnvelope {
    QueryEnvelope {
        schema_version: SCHEMA_VERSION,
        ok: false,
        operation: "query",
        wiki,
        agent,
        contract: None,
        knowledge_status: None,
        answer: None,
        citations: Vec::new(),
        gaps: Vec::new(),
        warnings: Vec::new(),
        duration_ms: 0,
        child_exit_code: None,
        raw_format: None,
        error: Some(error),
    }
}

fn argument_invalid_query(message: impl Into<String>) -> QueryEnvelope {
    query_failure_envelope(
        None,
        None,
        AppError::new(ErrorCode::ArgumentInvalid, message),
    )
}

fn emit_query(json: bool, envelope: QueryEnvelope) -> i32 {
    let exit = envelope
        .error
        .as_ref()
        .map(|e| e.code.exit_code())
        .unwrap_or(0);
    if json {
        print!("{}", render_json(&envelope));
    } else if let Some(err) = &envelope.error {
        // D2/AC3: the human-mode error line is a stream-routing decision,
        // not `render_human`'s concern (see that function's own doc
        // comment) — `render_human` is only ever called below, on the
        // success shape.
        eprint_error_line(err);
    } else {
        print!("{}", render_human(&envelope));
    }
    exit as i32
}

/// Reads stdin to completion. A genuine I/O failure is `INTERNAL_ERROR` (this
/// is an environment fault, not a caller mistake) — distinct from the
/// logical "ambiguous input"/"no input" cases below, which are
/// `ARGUMENT_INVALID`.
fn read_all_stdin() -> Result<Vec<u8>, AppError> {
    let mut buf = Vec::new();
    std::io::stdin()
        .read_to_end(&mut buf)
        .map_err(|e| AppError::new(ErrorCode::InternalError, format!("cannot read stdin: {e}")))?;
    Ok(buf)
}

/// Resolves the complete question bytes (spec §5.1): the positional after
/// `--`, or the complete stdin content when it is omitted. Supplying both is
/// rejected rather than merged; supplying neither (empty stdin *and* no
/// positional) is rejected too, since there is then no question at all.
///
/// Reading stdin is skipped entirely when a positional was given *and*
/// stdin is a terminal (an interactive caller typing a positional has no
/// piped input to spuriously collide with); every other combination reads
/// stdin to completion — always safe here because it is either already
/// piped/redirected (returns immediately) or, when no positional was given
/// at all, blocking on it is the documented stdin-fallback behavior itself.
fn resolve_question_bytes(positional: Option<String>) -> Result<Vec<u8>, AppError> {
    let stdin_is_terminal = std::io::stdin().is_terminal();
    let bytes = match (&positional, stdin_is_terminal) {
        (Some(text), true) => text.clone().into_bytes(),
        (Some(text), false) => {
            let piped = read_all_stdin()?;
            if !piped.is_empty() {
                return Err(AppError::new(
                    ErrorCode::ArgumentInvalid,
                    "the question cannot be supplied both as a positional argument and via stdin",
                ));
            }
            text.clone().into_bytes()
        }
        (None, _) => read_all_stdin()?,
    };
    if bytes.is_empty() {
        return Err(AppError::new(
            ErrorCode::ArgumentInvalid,
            "a question is required: supply it as a positional argument after -- or via stdin",
        ));
    }
    Ok(bytes)
}

#[allow(clippy::too_many_arguments)]
fn run_query_command(
    json: bool,
    override_path: Option<&Path>,
    wiki_values: Vec<String>,
    agent: Option<Agent>,
    question_positional: Option<String>,
) -> i32 {
    // spec §5.1: "accepts one and only one --wiki. Repeating it or supplying
    // `all` is an argument error." Checked before anything else is even
    // attempted, so it is always reached before any provider spawn.
    let wiki_id = match wiki_values.as_slice() {
        [one] if one != "all" => one.clone(),
        [] => {
            return emit_query(
                json,
                argument_invalid_query(format!(
                    "query requires exactly one --wiki ({QUERY_USAGE_HINT})"
                )),
            );
        }
        _ => {
            return emit_query(
                json,
                argument_invalid_query(format!(
                    "query accepts exactly one --wiki; repeating it or supplying \"all\" is invalid ({QUERY_USAGE_HINT})"
                )),
            );
        }
    };

    let question_bytes = match resolve_question_bytes(question_positional) {
        Ok(bytes) => bytes,
        Err(err) => return emit_query(json, query_failure_envelope(None, None, err)),
    };

    let config_path = match resolve_config_path(override_path) {
        Ok(p) => p,
        Err(e) => return emit_query(json, query_failure_envelope(None, None, e)),
    };
    let config = match Config::load(&config_path) {
        Ok(c) => c,
        Err(e) => return emit_query(json, query_failure_envelope(None, None, e)),
    };

    // spec §5.1: `--agent` is optional only when the wiki's derived
    // `default_agent` (same derivation `list` uses) is non-null. A wiki ID
    // this registry does not even contain is deliberately *not* handled here
    // — `QueryService::query` itself raises `WIKI_NOT_ALLOWED` for that
    // (rendered wiki:null/agent:null, per its own contract), so the
    // placeholder agent value below is never actually observed: wiki lookup
    // fails first, before any agent value is used for anything.
    let resolved_agent = match agent {
        Some(a) => a,
        None => match config.wikis.get(&wiki_id) {
            Some(wiki) => match config.default_agent.filter(|d| wiki.agents.contains(d)) {
                Some(a) => a,
                None => {
                    return emit_query(
                        json,
                        argument_invalid_query(
                            "--agent is required: no default_agent is configured and enabled for this wiki",
                        ),
                    );
                }
            },
            None => Agent::Claude,
        },
    };

    let cache_path = match resolve_cache_path() {
        Ok(p) => p,
        Err(e) => return emit_query(json, query_failure_envelope(None, None, e)),
    };

    let request = QueryRequest {
        config,
        config_dir: config_dir_of(&config_path),
        wiki_id,
        agent: resolved_agent,
        question: question_bytes,
        temp_base: std::env::temp_dir(),
    };
    let service = QueryService::new(RealProcessRunner, FileProbeStore::new(cache_path));
    let spinner = start_query_spinner();
    let envelope = match service.query(request, QueryMode::Enforced) {
        Ok(envelope) => envelope,
        Err(e) => query_failure_envelope(None, None, e),
    };
    // Cleared unconditionally, before either branch above touches
    // stdout/stderr again (PRD 08-06-pre-0-1-0-cli-refinements AC2:
    // "cleared before output," success or failure alike).
    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }
    emit_query(json, envelope)
}

/// Starts a stderr-only spinner for the duration of the blocking provider
/// call (AC2), but only when stderr is an interactive terminal — an
/// `assert_cmd`-driven test, a pipe, a redirect, or CI never satisfies this,
/// so the spinner branch is unreachable from any automated test and never
/// contaminates stdout/stderr byte-for-byte comparisons. `None` is returned,
/// not a disabled/no-op spinner, so a non-interactive run never even
/// constructs an `indicatif` handle.
fn start_query_spinner() -> Option<indicatif::ProgressBar> {
    if !std::io::stderr().is_terminal() {
        return None;
    }
    let pb = indicatif::ProgressBar::new_spinner();
    pb.set_draw_target(indicatif::ProgressDrawTarget::stderr());
    if let Ok(style) = indicatif::ProgressStyle::with_template("{spinner} querying...") {
        pb.set_style(style.tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ "));
    }
    pb.enable_steady_tick(std::time::Duration::from_millis(100));
    Some(pb)
}

//! External markdown viewer tests (issue #8; task 08-12 design.md §1-§2).
//!
//! These never invoke a real `leaf`: a per-platform fake viewer script stands
//! in, so the assertions describe *this wrapper's* contract — what argv it
//! sends, what it does with each failure shape — rather than the third-party
//! binary's behavior, and they hold on a machine where leaf is not installed.

use std::fs;
use std::path::{Path, PathBuf};

use llm_wikis::config::{MapEnv, ProcessEnv, ViewerBackend, ViewerConfig};
use llm_wikis::error::ErrorCode;
use llm_wikis::viewer::Viewer;

/// Writes a fake viewer that echoes its argv on the first line and then the
/// stdin it received, so a test can assert on both.
#[cfg(windows)]
fn fake_viewer(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("fake-viewer.cmd");
    fs::write(&path, format!("@echo off\r\n{body}\r\n")).unwrap();
    path
}

#[cfg(unix)]
fn fake_viewer(dir: &Path, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("fake-viewer.sh");
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[cfg(windows)]
const ECHO_ARGV_THEN_STDIN: &str = "echo ARGV=%1 %2\r\nfindstr \"^\"";
#[cfg(unix)]
const ECHO_ARGV_THEN_STDIN: &str = "echo \"ARGV=$1 $2\"\ncat";

#[cfg(windows)]
const FAIL_WITH_DIAGNOSTIC: &str = "echo boom: something went wrong 1>&2\r\nexit /b 1";
#[cfg(unix)]
const FAIL_WITH_DIAGNOSTIC: &str = "echo 'boom: something went wrong' >&2\nexit 1";

#[cfg(windows)]
const SUCCEED_SILENTLY: &str = "exit /b 0";
#[cfg(unix)]
const SUCCEED_SILENTLY: &str = "exit 0";

/// Appends this invocation's argv to `argv.log` beside the script, then
/// succeeds. Lets a test inspect what a call that discards its own output --
/// like `probe` -- actually sent.
#[cfg(windows)]
const LOG_ARGV: &str = "echo %1 %2>>\"%~dp0argv.log\"
echo rendered";
#[cfg(unix)]
const LOG_ARGV: &str = "echo \"$1 $2\" >> \"$(dirname \"$0\")/argv.log\"
echo rendered";

fn viewer_at(path: &Path) -> Viewer {
    let config = ViewerConfig {
        backend: ViewerBackend::Leaf,
        executable: Some(path.display().to_string()),
    };
    Viewer::resolve(&config, &ProcessEnv).expect("configured absolute path resolves")
}

#[test]
fn render_sends_inline_with_an_explicit_ansi_width_and_pipes_the_markdown_on_stdin() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = fake_viewer(tmp.path(), ECHO_ARGV_THEN_STDIN);

    let rendered = viewer_at(&fake)
        .render("# Heading\n", 120)
        .expect("fake viewer succeeds");

    // The width is passed explicitly rather than left to the viewer's own
    // terminal detection, which would see this pipe and silently downgrade.
    assert!(
        rendered.contains("ARGV=--inline ansi:120"),
        "unexpected argv: {rendered:?}"
    );
    assert!(
        rendered.contains("# Heading"),
        "the markdown must reach the child on stdin: {rendered:?}"
    );
}

/// The viewer's own floor for a usable render width; anything narrower is
/// clamped rather than forwarded.
#[test]
fn render_clamps_an_absurdly_narrow_width_to_the_viewer_minimum() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = fake_viewer(tmp.path(), ECHO_ARGV_THEN_STDIN);

    let rendered = viewer_at(&fake).render("x\n", 3).expect("succeeds");

    assert!(
        rendered.contains("ARGV=--inline ansi:20"),
        "unexpected argv: {rendered:?}"
    );
}

/// Doctor's probe renders a document instead of asking for a version string:
/// `--inline` postdates the viewer's first releases, so an older binary would
/// answer `--version` happily and then fail at the first real query.
#[test]
fn probe_exercises_inline_rather_than_a_version_flag() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = fake_viewer(tmp.path(), LOG_ARGV);

    viewer_at(&fake).probe().expect("probe succeeds");

    // The argv read here is the probe's own, recorded by the fake viewer.
    // Asserting on a later `render` call instead would leave a regression that
    // switched `probe` to `--version` completely undetected.
    let logged = fs::read_to_string(tmp.path().join("argv.log")).expect("probe invoked the viewer");
    assert!(
        logged.contains("--inline"),
        "probe must exercise --inline, sent: {logged:?}"
    );
    assert!(
        !logged.contains("--version"),
        "probe must not settle for a version string, sent: {logged:?}"
    );
}

#[test]
fn a_nonzero_exit_becomes_an_error_carrying_the_first_diagnostic_line() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = fake_viewer(tmp.path(), FAIL_WITH_DIAGNOSTIC);

    let err = viewer_at(&fake).render("# x\n", 80).unwrap_err();

    assert_eq!(err.code, ErrorCode::CliNotFound);
    assert!(
        err.message.contains("boom"),
        "the child's own diagnostic should survive: {:?}",
        err.message
    );
}

/// Empty output is a failure, not an empty answer: printing it would silently
/// swallow the model's response.
#[test]
fn empty_output_is_an_error_rather_than_an_empty_render() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = fake_viewer(tmp.path(), SUCCEED_SILENTLY);

    let err = viewer_at(&fake).render("# x\n", 80).unwrap_err();

    assert_eq!(err.code, ErrorCode::CliNotFound);
    assert!(err.message.contains("no output"), "{:?}", err.message);
}

#[test]
fn a_missing_viewer_binary_is_reported_as_cli_not_found() {
    let missing = if cfg!(windows) {
        r"C:\definitely\not\here\leaf.exe"
    } else {
        "/definitely/not/here/leaf"
    };
    let config = ViewerConfig {
        backend: ViewerBackend::Leaf,
        executable: Some(missing.to_string()),
    };

    let err = Viewer::resolve(&config, &ProcessEnv).unwrap_err();

    assert_eq!(err.code, ErrorCode::CliNotFound);
}

/// With no `executable` configured the bare default command is looked up on
/// `PATH` — including, on Windows, through `PATHEXT`.
#[test]
fn an_unset_executable_resolves_the_default_command_from_path() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = fake_viewer(tmp.path(), SUCCEED_SILENTLY);
    let default_name = if cfg!(windows) { "leaf.cmd" } else { "leaf" };
    fs::copy(&fake, tmp.path().join(default_name)).unwrap();
    let env = MapEnv(std::collections::HashMap::from([(
        "PATH".to_string(),
        tmp.path().display().to_string(),
    )]));

    let viewer = Viewer::resolve(&ViewerConfig::default(), &env).expect("default name resolves");

    assert!(
        viewer.path().to_string_lossy().contains("leaf"),
        "unexpected resolved path: {:?}",
        viewer.path()
    );
}

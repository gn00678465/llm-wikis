//! Step 6: prove platform config/cache directory resolution (spec §5.2,
//! §15.1, plan Task 2 Step 6). Uses an injected env/home resolver, not the
//! real process environment, so all three platforms can be exercised from
//! one machine.

use crate::report::Report;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Copy)]
enum Platform {
    Windows,
    LinuxXdgSet,
    LinuxXdgFallback,
    MacOs,
}

/// Pure function: given a platform, an injected env map, and an injected
/// home directory, resolve the config.toml and probes-v1.json paths. Never
/// reads the real process environment or the real cwd.
fn resolve(platform: Platform, env: &HashMap<&str, &str>, home: &str) -> (PathBuf, PathBuf) {
    match platform {
        Platform::Windows => {
            let appdata = env.get("APPDATA").expect("APPDATA injected");
            let localappdata = env.get("LOCALAPPDATA").expect("LOCALAPPDATA injected");
            (
                PathBuf::from(appdata).join("llm-wikis").join("config.toml"),
                PathBuf::from(localappdata).join("llm-wikis").join("probes-v1.json"),
            )
        }
        Platform::LinuxXdgSet | Platform::LinuxXdgFallback => {
            let config_base = env
                .get("XDG_CONFIG_HOME")
                .map(|s| PathBuf::from(s))
                .unwrap_or_else(|| PathBuf::from(home).join(".config"));
            let cache_base = env
                .get("XDG_CACHE_HOME")
                .map(|s| PathBuf::from(s))
                .unwrap_or_else(|| PathBuf::from(home).join(".cache"));
            (
                config_base.join("llm-wikis").join("config.toml"),
                cache_base.join("llm-wikis").join("probes-v1.json"),
            )
        }
        Platform::MacOs => (
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("llm-wikis")
                .join("config.toml"),
            PathBuf::from(home)
                .join("Library")
                .join("Caches")
                .join("llm-wikis")
                .join("probes-v1.json"),
        ),
    }
}

pub fn run() -> i32 {
    let mut r = Report::new("platform-dirs");

    // No-cwd-dependency proof: chdir the real process somewhere irrelevant
    // before resolving, then chdir back. `resolve()` never calls
    // env::current_dir(), but this demonstrates the result is unaffected in
    // practice, not just by code inspection.
    let original_cwd = std::env::current_dir().ok();
    if let Ok(tmp) = tempfile::tempdir() {
        let _ = std::env::set_current_dir(tmp.path());
    }

    // Windows
    let mut win_env = HashMap::new();
    win_env.insert("APPDATA", r"C:\Users\gn006\AppData\Roaming");
    win_env.insert("LOCALAPPDATA", r"C:\Users\gn006\AppData\Local");
    let (cfg, cache) = resolve(Platform::Windows, &win_env, r"C:\Users\gn006");
    r.check(
        "windows_config_path",
        cfg == PathBuf::from(r"C:\Users\gn006\AppData\Roaming\llm-wikis\config.toml"),
        format!("{}", cfg.display()),
    );
    r.check(
        "windows_cache_path",
        cache == PathBuf::from(r"C:\Users\gn006\AppData\Local\llm-wikis\probes-v1.json"),
        format!("{}", cache.display()),
    );

    // Linux/WSL with XDG_* explicitly set
    let mut linux_env_set = HashMap::new();
    linux_env_set.insert("XDG_CONFIG_HOME", "/home/u/.myconfig");
    linux_env_set.insert("XDG_CACHE_HOME", "/home/u/.mycache");
    let (cfg, cache) = resolve(Platform::LinuxXdgSet, &linux_env_set, "/home/u");
    r.check(
        "linux_xdg_set_config_path",
        cfg == PathBuf::from("/home/u/.myconfig/llm-wikis/config.toml"),
        format!("{}", cfg.display()),
    );
    r.check(
        "linux_xdg_set_cache_path",
        cache == PathBuf::from("/home/u/.mycache/llm-wikis/probes-v1.json"),
        format!("{}", cache.display()),
    );

    // Linux/WSL falling back to ~/.config, ~/.cache when XDG_* unset
    let linux_env_fallback: HashMap<&str, &str> = HashMap::new();
    let (cfg, cache) = resolve(Platform::LinuxXdgFallback, &linux_env_fallback, "/home/u");
    r.check(
        "linux_xdg_fallback_config_path",
        cfg == PathBuf::from("/home/u/.config/llm-wikis/config.toml"),
        format!("{}", cfg.display()),
    );
    r.check(
        "linux_xdg_fallback_cache_path",
        cache == PathBuf::from("/home/u/.cache/llm-wikis/probes-v1.json"),
        format!("{}", cache.display()),
    );

    // macOS — Application Support / Caches, both containing spaces.
    let macos_env: HashMap<&str, &str> = HashMap::new();
    let (cfg, cache) = resolve(Platform::MacOs, &macos_env, "/Users/u");
    let cfg_expected = PathBuf::from("/Users/u/Library/Application Support/llm-wikis/config.toml");
    let cache_expected = PathBuf::from("/Users/u/Library/Caches/llm-wikis/probes-v1.json");
    r.check(
        "macos_config_path_with_space",
        cfg == cfg_expected && cfg.display().to_string().contains(' '),
        format!("{}", cfg.display()),
    );
    r.check(
        "macos_cache_path",
        cache == cache_expected,
        format!("{}", cache.display()),
    );

    if let Some(cwd) = original_cwd {
        let _ = std::env::set_current_dir(cwd);
    }

    r.finish()
}

//! Disposable pre-implementation capability spikes for llm-wikis.
//! See spikes/README.md: this code is evidence only, never copied into
//! production. Subcommand-per-spike dispatch, per plan Task 2 Step 1.

mod bounded_pipes;
mod fixture;
mod mutation_hash;
mod platform_dirs;
mod process_tree;
mod provider_contract;
mod report;
mod stdin_boundary;
mod temp_artifacts;
mod windows_resolution;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).cloned().unwrap_or_default();
    let code = match cmd.as_str() {
        "__fixture" => fixture::run(&args[2..]),
        "stdin-boundary" => stdin_boundary::run(),
        "windows-resolution" => windows_resolution::run(),
        "platform-dirs" => platform_dirs::run(),
        "bounded-pipes" => bounded_pipes::run(),
        "process-tree" => process_tree::run(),
        "mutation-hash" => mutation_hash::run(),
        "temp-artifacts" => temp_artifacts::run(),
        "provider-contract" => provider_contract::run(&args[2..]),
        _ => {
            eprintln!(
                "usage: llm-wikis-spikes <stdin-boundary|windows-resolution|platform-dirs|bounded-pipes|process-tree|mutation-hash|temp-artifacts|provider-contract>"
            );
            2
        }
    };
    std::process::exit(code);
}

use std::process::ExitCode;

fn main() -> ExitCode {
    ExitCode::from(llm_wikis::cli::run() as u8)
}

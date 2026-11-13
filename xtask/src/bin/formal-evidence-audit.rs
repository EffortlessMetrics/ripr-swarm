//! Offline audit front door; native capture shares the DTO and predicates.
use xtask::formal_evidence::{USAGE, execute};

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["--help"] || args == ["-h"] {
        println!("{USAGE}");
        return std::process::ExitCode::SUCCESS;
    }
    match execute(&args)
        .and_then(|summary| serde_json::to_string_pretty(&summary).map_err(|e| e.to_string()))
    {
        Ok(output) => {
            println!("{output}");
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("formal-evidence-audit: {error}");
            std::process::ExitCode::from(2)
        }
    }
}

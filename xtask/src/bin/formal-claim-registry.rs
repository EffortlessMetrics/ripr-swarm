//! Thin command entry for the governed formal-claim registry owner.
use xtask::formal_registry::{USAGE, execute};

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if matches!(args.as_slice(), [flag] if flag == "--help" || flag == "-h") {
        println!("{USAGE}");
        return std::process::ExitCode::SUCCESS;
    }
    match execute(&args) {
        Ok(output) => {
            println!("{output}");
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("formal-claim-registry: {error}");
            std::process::ExitCode::from(2)
        }
    }
}

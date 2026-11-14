//! Opt-in #12906 measurement. The analyzer is supplied, never built or downloaded.
#[path = "../src/reports/dx_scoreboard/measure/resident_session.rs"]
mod resident_session;

fn main() -> std::process::ExitCode {
    match resident_session::main_entry() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("resident-session: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

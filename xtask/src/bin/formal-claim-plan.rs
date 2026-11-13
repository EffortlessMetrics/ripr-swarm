#[path = "formal_claim_plan/mod.rs"]
mod formal_claim_plan;

fn main() -> std::process::ExitCode {
    formal_claim_plan::entry()
}

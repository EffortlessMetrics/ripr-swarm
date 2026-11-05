#![forbid(unsafe_code)]
fn main() {
    let command: Option<fn() -> Result<(), String>> = match std::env::args().nth(1).as_deref() {
        Some("python-wheel-applicability") => Some(repo_policy::python_wheel::command),
        Some("capture-coverage-failure") => Some(repo_policy::coverage_diagnostics::command),
        Some("check-workflows") => Some(|| {
            repo_policy::verify_executable_identity()
                .and_then(|()| repo_policy::check_workflows_impl())
        }),
        Some("check-agent-skills") => Some(|| {
            repo_policy::verify_executable_identity()
                .and_then(|()| repo_policy::agent_skills::check())
        }),
        Some("check-executable-files") => Some(|| {
            repo_policy::verify_executable_identity()
                .and_then(|()| repo_policy::executable_files::check())
        }),
        Some("preflight") => Some(repo_policy::preflight),
        Some("verify-preflight") => Some(|| {
            repo_policy::verify_preflight(std::path::Path::new(
                "target/ripr/reports/policy-preflight.json",
            ))
        }),
        _ => None,
    };
    let result = match command {
        Some(command) => repo_policy::enter_workspace_root().and_then(|()| command()),
        None => Err("usage: cargo policy check-workflows | check-agent-skills | check-executable-files | preflight | verify-preflight | python-wheel-applicability <event> <base-sha> <head-sha> | capture-coverage-failure".into()),
    };
    if let Err(error) = result {
        eprintln!("xtask: {error}");
        std::process::exit(1);
    }
}

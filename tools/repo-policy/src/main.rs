fn main() {
    let command: Option<fn() -> Result<(), String>> = match std::env::args().nth(1).as_deref() {
        Some("check-workflows") => Some(|| {
            repo_policy::verify_executable_identity()
                .and_then(|()| repo_policy::check_workflows_impl())
        }),
        Some("check-agent-skills") => Some(|| {
            repo_policy::verify_executable_identity()
                .and_then(|()| repo_policy::agent_skills::check())
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
        None => Err("usage: cargo policy check-workflows | check-agent-skills | preflight | verify-preflight".into()),
    };
    if let Err(error) = result {
        eprintln!("xtask: {error}");
        std::process::exit(1);
    }
}

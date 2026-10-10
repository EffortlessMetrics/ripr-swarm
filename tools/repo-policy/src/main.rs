fn main() {
    let result = match std::env::args().nth(1).as_deref() {
        Some("check-workflows") => repo_policy::verify_executable_identity()
            .and_then(|()| repo_policy::check_workflows_impl()),
        Some("check-agent-skills") => repo_policy::verify_executable_identity()
            .and_then(|()| repo_policy::agent_skills::check()),
        Some("preflight") => repo_policy::preflight(),
        Some("verify-preflight") => repo_policy::verify_preflight(std::path::Path::new(
            "target/ripr/reports/policy-preflight.json",
        )),
        _ => Err("usage: cargo policy check-workflows | check-agent-skills | preflight".into()),
    };
    if let Err(error) = result {
        eprintln!("xtask: {error}");
        std::process::exit(1);
    }
}

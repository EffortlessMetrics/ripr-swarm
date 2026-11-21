//! Keep the scratch runner's wrapper and Cargo settings paired (#14685).
use yaml_rust2::{Yaml, YamlLoader};

const WORKFLOW: &str = include_str!("../../.github/workflows/rust-gates.yml");

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn jobs(workflow: &str) -> TestResult<yaml_rust2::yaml::Hash> {
    let documents = YamlLoader::load_from_str(workflow)?;
    let document = documents.first().ok_or("empty workflow")?;
    Ok(document["jobs"]
        .as_hash()
        .ok_or("missing workflow jobs")?
        .clone())
}

fn require_nonincremental(workflow: &str) -> TestResult {
    let mut found = false;
    for (job_id, job) in jobs(workflow)? {
        let steps = job["steps"].as_vec().into_iter().flatten();
        for scope in std::iter::once(&job).chain(steps) {
            let env = &scope["env"];
            if env["RUSTC_WRAPPER"].as_str() == Some("sccache") {
                found = true;
                let incremental = &env["CARGO_INCREMENTAL"];
                if incremental.as_str() != Some("0") && incremental != &Yaml::Integer(0) {
                    return Err(
                        format!("{job_id:?}: sccache env requires CARGO_INCREMENTAL=0").into(),
                    );
                }
            }
        }
    }
    assert!(found, "expected a sccache environment in rust-gates");
    Ok(())
}

#[test]
fn sccache_environments_disable_incremental_compilation() -> TestResult {
    require_nonincremental(WORKFLOW)
}

#[test]
fn a_second_job_cannot_borrow_the_first_jobs_setting() {
    let result = require_nonincremental(
        "jobs:\n  paired:\n    env: {RUSTC_WRAPPER: sccache, CARGO_INCREMENTAL: 0}\n  missing:\n    env: {RUSTC_WRAPPER: sccache}\n",
    );
    assert!(
        matches!(result, Err(error) if error.to_string().contains("sccache env requires CARGO_INCREMENTAL=0"))
    );
}

#[test]
fn scratch_setup_persists_nonincremental_compilation_with_the_wrapper() -> TestResult {
    let workflow_jobs = jobs(WORKFLOW)?;
    let steps = workflow_jobs
        .get(&Yaml::String("rust-gates".into()))
        .ok_or("missing rust-gates job")?["steps"]
        .as_vec()
        .ok_or("missing rust-gates steps")?;
    let setup = steps
        .iter()
        .find(|step| step["name"].as_str() == Some("Configure scratch environment"))
        .ok_or("scratch setup")?;
    let run = setup["run"].as_str().ok_or("scratch setup script")?;
    let (group, _) = run
        .split_once("} >> \"$GITHUB_ENV\"")
        .ok_or("scratch environment export")?;
    let (_, exports) = group.rsplit_once("{\n").ok_or("scratch export group")?;
    for key in ["CARGO_INCREMENTAL", "RUSTC_WRAPPER"] {
        assert!(
            exports
                .lines()
                .any(|line| line.trim() == format!("echo \"{key}=${key}\"")),
            "scratch setup must export {key}"
        );
    }
    Ok(())
}

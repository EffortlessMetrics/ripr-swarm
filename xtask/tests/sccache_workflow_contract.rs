//! Keep the scratch runner's wrapper and Cargo settings paired (#14685).
use yaml_rust2::{Yaml, YamlLoader};

const WORKFLOW: &str = include_str!("../../.github/workflows/rust-gates.yml");

fn jobs(workflow: &str) -> yaml_rust2::yaml::Hash {
    YamlLoader::load_from_str(workflow).unwrap()[0]["jobs"]
        .as_hash()
        .expect("workflow jobs")
        .clone()
}

fn assert_nonincremental(workflow: &str) {
    let mut found = false;
    for (job_id, job) in jobs(workflow) {
        let steps = job["steps"].as_vec().into_iter().flatten();
        for scope in std::iter::once(&job).chain(steps) {
            let env = &scope["env"];
            if env["RUSTC_WRAPPER"].as_str() == Some("sccache") {
                found = true;
                let incremental = &env["CARGO_INCREMENTAL"];
                assert!(
                    incremental.as_str() == Some("0") || incremental == &Yaml::Integer(0),
                    "{job_id:?}: sccache env requires CARGO_INCREMENTAL=0"
                );
            }
        }
    }
    assert!(found, "expected a sccache environment in rust-gates");
}

#[test]
fn sccache_environments_disable_incremental_compilation() {
    assert_nonincremental(WORKFLOW);
}

#[test]
#[should_panic(expected = "sccache env requires CARGO_INCREMENTAL=0")]
fn a_second_job_cannot_borrow_the_first_jobs_setting() {
    assert_nonincremental(
        "jobs:\n  paired:\n    env: {RUSTC_WRAPPER: sccache, CARGO_INCREMENTAL: 0}\n  missing:\n    env: {RUSTC_WRAPPER: sccache}\n",
    );
}

#[test]
fn scratch_setup_persists_nonincremental_compilation_with_the_wrapper() {
    let workflow_jobs = jobs(WORKFLOW);
    let steps = workflow_jobs[&Yaml::String("rust-gates".into())]["steps"]
        .as_vec()
        .unwrap();
    let setup = steps
        .iter()
        .find(|step| step["name"].as_str() == Some("Configure scratch environment"))
        .expect("scratch setup");
    let run = setup["run"].as_str().expect("scratch setup script");
    let (group, _) = run
        .split_once("} >> \"$GITHUB_ENV\"")
        .expect("scratch environment export");
    let (_, exports) = group.rsplit_once("{\n").expect("scratch export group");
    for key in ["CARGO_INCREMENTAL", "RUSTC_WRAPPER"] {
        assert!(
            exports
                .lines()
                .any(|line| line.trim() == format!("echo \"{key}=${key}\"")),
            "scratch setup must export {key}"
        );
    }
}

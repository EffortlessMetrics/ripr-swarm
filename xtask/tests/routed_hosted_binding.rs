//! Execute the actual hosted-proof binding shell with GitHub API fixtures.
#![cfg(unix)]
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const TREE: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const WORKFLOW_SHA: &str = "cccccccccccccccccccccccccccccccccccccccc";
const GATE_BLOB: &str = "8b9392b20ae5f64c5f84d9c0f9dec575db7efca0";
const WORKFLOW: &str = include_str!("../../.github/workflows/routed-rust.yml");

fn shell() -> String {
    let part = WORKFLOW
        .split_once("      - name: Bind single hosted proof to actual run and commands\n")
        .unwrap()
        .1;
    let body = part
        .split_once("        run: |\n")
        .unwrap()
        .1
        .split_once("\n      - name:")
        .unwrap()
        .0;
    body.lines()
        .map(|line| {
            if line.is_empty() {
                ""
            } else {
                line.strip_prefix("          ").unwrap()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}
struct Fixture {
    dir: PathBuf,
    run: serde_json::Value,
    job: serde_json::Value,
    log: String,
}
impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "ripr-hosted-binding-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        let source = shell();
        let names: Vec<String> = serde_json::from_str(
            source
                .split_once("required='")
                .unwrap()
                .1
                .split_once("'\n")
                .unwrap()
                .0,
        )
        .unwrap();
        let steps: Vec<_> = names
            .iter()
            .map(
                |name| serde_json::json!({"name":name,"status":"completed","conclusion":"success"}),
            )
            .collect();
        let run = serde_json::json!({"id":100,"run_attempt":1,"event":"merge_group","head_sha":SHA,"repository":{"full_name":"EffortlessMetrics/ripr-swarm"},"referenced_workflows":[{"path":"EffortlessMetrics/ripr-swarm/.github/workflows/rust-gates.yml@subject","sha":WORKFLOW_SHA}]});
        let job = serde_json::json!({"id":8,"run_id":100,"name":"Ripr Rust Small on GitHub Hosted / Rust gates","status":"completed","conclusion":"success","steps":steps});
        let log = source
            .split_once("done <<'COMMANDS'\n")
            .unwrap()
            .1
            .split_once("\nCOMMANDS")
            .unwrap()
            .0
            .to_string();
        fs::write(dir.join("binding.sh"), source).unwrap();
        executable(
            &dir.join("gh"),
            "#!/bin/sh\ncase \"$2\" in */attempts/1) cat \"$TESTDIR/run.json\";; *jobs\\?*) cat \"$TESTDIR/jobs.json\";; */jobs/8/logs) cat \"$TESTDIR/log\";; *) exit 1;; esac\n",
        );
        executable(
            &dir.join("git"),
            &format!(
                "#!/bin/sh\ncase \"$1\" in cat-file) exit 0;; show) echo '\"--tree\"';; rev-parse) case \"$2\" in *rust-gates.yml) echo {GATE_BLOB};; *) echo {TREE};; esac;; *) exit 1;; esac\n"
            ),
        );
        Self { dir, run, job, log }
    }
    fn execute(&self, subject: &str, child: &str) -> bool {
        fs::write(self.dir.join("run.json"), self.run.to_string()).unwrap();
        fs::write(
            self.dir.join("jobs.json"),
            serde_json::json!({"total_count":1,"jobs":[self.job]}).to_string(),
        )
        .unwrap();
        fs::write(self.dir.join("log"), &self.log).unwrap();
        let identity = serde_json::json!({"route":"github","runner_environment":"github-hosted","run_id":"100","run_attempt":"1","subject_sha":SHA,"subject_tree":TREE});
        let path = format!("{}:{}", self.dir.display(), std::env::var("PATH").unwrap());
        Command::new("bash")
            .arg(self.dir.join("binding.sh"))
            .env("PATH", path)
            .env("TESTDIR", &self.dir)
            .env("REPOSITORY", "EffortlessMetrics/ripr-swarm")
            .env("GITHUB_RUN_ID", "100")
            .env("GITHUB_RUN_ATTEMPT", "1")
            .env("EXPECTED_SUBJECT", subject)
            .env("RUN_HEAD", SHA)
            .env("EVENT_NAME", "merge_group")
            .env("CHILD_RESULT", child)
            .env("EXECUTION_IDENTITY", identity.to_string())
            .output()
            .unwrap()
            .status
            .success()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}
fn executable(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}
#[test]
fn authentic_complete_child_passes_and_missing_or_changed_evidence_refuses() {
    let mut f = Fixture::new();
    assert!(f.execute(SHA, "success"));
    assert!(!f.execute("dddddddddddddddddddddddddddddddddddddddd", "success"));
    assert!(!f.execute(SHA, "failure"));
    let original = f.job.clone();
    f.job["steps"][0]["conclusion"] = "failure".into();
    assert!(!f.execute(SHA, "success"));
    f.job = original.clone();
    f.job["steps"].as_array_mut().unwrap().pop();
    assert!(!f.execute(SHA, "success"));
    f.job = original.clone();
    let duplicate = f.job["steps"][0].clone();
    f.job["steps"].as_array_mut().unwrap().push(duplicate);
    assert!(!f.execute(SHA, "success"));
    f.job["steps"].as_array_mut().unwrap().last_mut().unwrap()["conclusion"] = "failure".into();
    assert!(!f.execute(SHA, "success"));
    f.job = original;
    let original_log = f.log.clone();
    f.log.clear();
    assert!(!f.execute(SHA, "success"));
    f.log = original_log;
    f.run["run_attempt"] = 2.into();
    assert!(!f.execute(SHA, "success"));
    f.run["run_attempt"] = 1.into();
    f.run["head_sha"] = "dddddddddddddddddddddddddddddddddddddddd".into();
    assert!(!f.execute(SHA, "success"));
    f.run["head_sha"] = SHA.into();
    let duplicate = f.run["referenced_workflows"][0].clone();
    f.run["referenced_workflows"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    assert!(!f.execute(SHA, "success"));
    f.run["referenced_workflows"].as_array_mut().unwrap().pop();
    f.run["referenced_workflows"][0]["path"] = "foreign/workflow".into();
    assert!(!f.execute(SHA, "success"));
}

#[test]
fn older_base_that_skips_trusted_matcher_does_not_qualify() {
    let f = Fixture::new();
    let path = f.dir.join("git");
    let original = fs::read_to_string(&path).unwrap();
    let git = original.replace("echo '\"--tree\"'", "echo old-matcher");
    assert_ne!(git, original);
    executable(&path, &git);
    assert!(!f.execute(SHA, "success"));
}
#[test]
fn gate_blob_pin_matches_the_reviewed_definition() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let out = Command::new("git")
        .current_dir(root)
        .args(["hash-object", ".github/workflows/rust-gates.yml"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(String::from_utf8(out.stdout).unwrap().trim(), GATE_BLOB);
}

#[test]
fn command_log_contract_retains_full_hosted_gate_coverage() {
    let source = shell();
    let required: Vec<String> = serde_json::from_str(
        source
            .split_once("required='")
            .unwrap()
            .1
            .split_once("'\n")
            .unwrap()
            .0,
    )
    .unwrap();
    for name in [
        "Observe child execution identity",
        "Formatting preflight",
        "Agent skill contracts",
        "Repository policy preflight",
        "Evidence promotion honesty",
        "Dependency policy",
        "Process policy",
        "Network policy",
        "All-features ripr target check",
        "Source workspace target check",
        "Source precommit",
        "Source agent skill contracts",
        "Strict workspace Clippy",
        "Golden output contracts",
        "Fixture contracts",
        "Capability producer inventory",
        "Capability registry docs block",
        "Verdict corpora",
        "Required Rust tests",
        "Source workspace tests",
        "Required Rust doctests",
        "PR evidence",
        "Source review-comments check",
        "Schema producer sweep",
    ] {
        assert_eq!(
            required.iter().filter(|item| item.as_str() == name).count(),
            1,
            "missing or duplicate required step {name}"
        );
    }
    let block = source
        .split_once("done <<'COMMANDS'\n")
        .unwrap()
        .1
        .split_once("\nCOMMANDS")
        .unwrap()
        .0;
    for command in [
        "cargo fmt --check",
        "cargo policy preflight",
        "cargo xtask ci-policy",
        "cargo xtask check-evidence-promotion-honesty",
        "cargo xtask check-dependencies",
        "cargo xtask check-process-policy",
        "cargo xtask check-network-policy",
        "cargo check -p ripr --all-targets --all-features",
        "cargo check --workspace --all-targets",
        "cargo xtask precommit",
        "cargo xtask check-agent-skills",
        "cargo clippy --workspace --all-targets -- -D warnings",
        "cargo xtask goldens check",
        "cargo xtask fixtures",
        "cargo xtask check-capability-producers",
        "cargo xtask capability-docs --check",
        "cargo xtask verdict-corpus check-all",
        "cargo nextest run --workspace --profile ci",
        "cargo nextest run --workspace",
        "cargo test --workspace --doc",
        "cargo xtask impacted-evidence --check",
        "cargo xtask ripr-pr-summary --check",
        "cargo xtask ripr-annotations --check",
        "cargo xtask schema-producer-sweep",
    ] {
        assert!(
            block.lines().any(|line| line == command),
            "missing exact command {command}"
        );
    }
}

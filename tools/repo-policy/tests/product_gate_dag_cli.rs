//! Real product-independent controller commands over an actual Git integration.
//! GitHub step outcomes below are simulated; this is not product gate execution.
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;

#[path = "../test_support.rs"]
mod support;

struct Fixture {
    root: PathBuf,
    binary: PathBuf,
    subject: serde_json::Value,
}
impl Fixture {
    fn git(&self, args: &[&str]) -> Result<String, String> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned());
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }
    fn new() -> Result<Self, String> {
        Self::with_format(true)
    }
    fn with_format(formatted: bool) -> Result<Self, String> {
        let source = support::workspace_root()?;
        // Keep this Git fixture outside the source checkout's Cargo-config
        // ancestry, as the real Actions workspace is. Cargo forces TMPDIR to
        // the source target directory, so std::env::temp_dir is not isolated.
        let temp = if cfg!(windows) {
            PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("missing native temporary root")?)
                .join("Temp")
        } else {
            PathBuf::from("/tmp")
        };
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let owner = temp.join(format!("ripr-dag-cli-{}-{stamp}", std::process::id()));
        fs::create_dir(&owner).map_err(|e| e.to_string())?;
        let root = owner.join("checkout");
        fs::create_dir(&root).map_err(|e| e.to_string())?;
        let mut fixture = Self {
            root,
            binary: support::policy_binary(env!("CARGO_BIN_EXE_repo-policy"))?,
            subject: serde_json::Value::Null,
        };
        for path in [
            ".github/workflows/rust-gates.yml",
            ".github/workflows/rust-gates-dag-branch.yml",
            ".github/workflows/rust-gates-dag.yml",
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain.toml",
            ".cargo/config.toml",
            ".config/nextest.toml",
        ] {
            let dest = fixture.root.join(path);
            fs::create_dir_all(dest.parent().ok_or("missing parent")?)
                .map_err(|e| e.to_string())?;
            fs::copy(source.join(path), dest).map_err(|e| e.to_string())?;
        }
        fs::write(fixture.root.join("Cargo.toml"), "[package]\nname=\"allocation-control\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[workspace]\n").map_err(|e| e.to_string())?;
        fs::create_dir(fixture.root.join("src")).map_err(|e| e.to_string())?;
        fs::write(
            fixture.root.join("src/lib.rs"),
            if formatted {
                "pub fn value() -> u32 {\n    1\n}\n"
            } else {
                "pub fn value()->u32{1}\n"
            },
        )
        .map_err(|e| e.to_string())?;
        fixture.git(&["init", "-b", "main"])?;
        fixture.git(&["config", "user.email", "fixture@example.invalid"])?;
        fixture.git(&["config", "user.name", "DAG fixture"])?;
        fixture.git(&["add", "."])?;
        fixture.git(&["-c", "commit.gpgsign=false", "commit", "-m", "base"])?;
        let base = fixture.git(&["rev-parse", "HEAD"])?;
        fixture.git(&["switch", "-c", "feature"])?;
        fs::write(fixture.root.join("payload.txt"), "subject\n").map_err(|e| e.to_string())?;
        fixture.git(&["add", "payload.txt"])?;
        fixture.git(&["-c", "commit.gpgsign=false", "commit", "-m", "subject"])?;
        let head = fixture.git(&["rev-parse", "HEAD"])?;
        fixture.git(&["switch", "main"])?;
        fixture.git(&[
            "-c",
            "commit.gpgsign=false",
            "merge",
            "--no-ff",
            "feature",
            "-m",
            "actual integration",
        ])?;
        fixture.subject = serde_json::json!({"version":3,"kind":"merge-group","repository":"EffortlessMetrics/ripr-swarm","request_id":"42-1","run_id":"42","run_attempt":"1","base":base,"head":head,"merge":fixture.git(&["rev-parse","HEAD"])?,"tree":fixture.git(&["rev-parse","HEAD^{tree}"])?,"parents":fixture.git(&["show","-s","--format=%P","HEAD"])?.split_whitespace().collect::<Vec<_>>(),"workflow_generation":base});
        Ok(fixture)
    }
    fn call(&self, args: &[&str], overrides: &[(&str, &str)]) -> Result<Output, String> {
        let mut cmd = Command::new(&self.binary);
        cmd.arg("product-gate-dag")
            .args(args)
            .current_dir(&self.root);
        // Remove Cargo's test-runner configuration, then explicitly test the
        // unsupported overrides against the real executable, never a stub.
        for (key, _) in std::env::vars_os() {
            let name = key.to_string_lossy();
            if name.starts_with("CARGO_PROFILE_")
                || name.starts_with("CARGO_BUILD_")
                || name.starts_with("CARGO_TARGET_")
                || matches!(
                    name.as_ref(),
                    "CARGO_HOME"
                        | "CARGO_INCREMENTAL"
                        | "RUSTC"
                        | "RUSTDOC"
                        | "RUSTFLAGS"
                        | "RUSTDOCFLAGS"
                        | "CARGO_ENCODED_RUSTFLAGS"
                        | "CARGO_ENCODED_RUSTDOCFLAGS"
                        | "RUSTC_WRAPPER"
                        | "RUSTC_WORKSPACE_WRAPPER"
                )
            {
                cmd.env_remove(key);
            }
        }
        // dtolnay/rust-toolchain@683b588c487a69d01f6c6ddf40f036ff682cc2c8
        // emits CARGO_HOME and CARGO_INCREMENTAL=0 before every staged job.
        let home = self.root.join("action-home");
        fs::create_dir_all(home.join(".cargo")).map_err(|e| e.to_string())?;
        let home_key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
        let rustup = std::env::var_os("RUSTUP_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os(home_key).map(|p| PathBuf::from(p).join(".rustup")))
            .ok_or("missing original rustup home")?;
        cmd.env(home_key, &home)
            .env("CARGO_HOME", home.join(".cargo"))
            .env("CARGO_INCREMENTAL", "0")
            .env("RUSTUP_HOME", rustup);
        cmd.env("RIPR_DAG_SUBJECT", self.subject.to_string())
            .env("GITHUB_REPOSITORY", "EffortlessMetrics/ripr-swarm")
            .env("GITHUB_RUN_ID", "42")
            .env("GITHUB_RUN_ATTEMPT", "1")
            .envs(overrides.iter().copied());
        cmd.output().map_err(|e| e.to_string())
    }
}
fn success(output: Output) -> Result<(), String> {
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}
#[test]
fn native_controller_refuses_configuration_stale_subject_and_incomplete_results()
-> Result<(), String> {
    for workflow in [
        include_str!("../../../.github/workflows/rust-gates-dag.yml"),
        include_str!("../../../.github/workflows/rust-gates-dag-branch.yml"),
    ] {
        assert!(
            workflow.contains("dtolnay/rust-toolchain@683b588c487a69d01f6c6ddf40f036ff682cc2c8"),
            "re-evaluate action environment when the toolchain action changes"
        );
    }
    let fixture = Fixture::new()?;
    success(fixture.call(&["plan", "plan.json"], &[])?)?;
    success(fixture.call(&["verify", "plan.json"], &[])?)?;
    // Effective Cargo configuration is never ignored: an inherited or
    // extensionless file, including an untracked one, must refuse admission.
    for path in [
        "action-home/.cargo/config",
        "action-home/.cargo/config.toml",
        ".cargo/config",
        "../.cargo/config.toml",
    ] {
        let path = fixture.root.join(path);
        fs::create_dir_all(path.parent().ok_or("config parent")?).map_err(|e| e.to_string())?;
        fs::write(&path, "[build]\nrustflags = [\"--cfg=unreviewed\"]\n")
            .map_err(|e| e.to_string())?;
        assert!(
            !fixture
                .call(&["verify", "plan.json"], &[])?
                .status
                .success(),
            "inherited config must refuse"
        );
        assert!(
            !fixture
                .call(&["plan", "refused-config.json"], &[])?
                .status
                .success(),
            "inherited config must refuse admission"
        );
        assert!(!fixture.root.join("refused-config.json").exists());
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    for (key, value) in [
        ("CARGO_HOME", "/other/cargo"),
        ("CARGO_INCREMENTAL", "1"),
        ("rustdocflags", "--cfg=unreviewed"),
    ] {
        assert!(
            !fixture
                .call(&["verify", "plan.json"], &[(key, value)])?
                .status
                .success(),
            "incompatible override {key}"
        );
    }
    let plan: serde_json::Value = serde_json::from_slice(
        &fs::read(fixture.root.join("plan.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let rows = plan["rows"].as_array().ok_or("missing rows")?;
    assert_eq!(rows.len(), 24);
    for key in [
        "RUSTFLAGS",
        "RUSTDOCFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_ENCODED_RUSTDOCFLAGS",
        "RUSTC_WORKSPACE_WRAPPER",
        "RUSTC_WRAPPER",
        "CARGO_PROFILE_TEST_DEBUG",
        "CARGO_PROFILE_DEV_OPT_LEVEL",
        "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS",
        "CARGO_BUILD_TARGET",
        "CARGO_INCREMENTAL",
        "CARGO_HOME",
    ] {
        let output = fixture.call(&["verify", "plan.json"], &[(key, "")])?;
        assert!(!output.status.success(), "{key}");
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("unsupported build configuration override"),
            "{key}"
        );
        let output = fixture.call(&["plan", "refused.json"], &[(key, "")])?;
        assert!(!output.status.success(), "{key}");
        assert!(!fixture.root.join("refused.json").exists());
    }
    assert!(
        !fixture
            .call(&["verify", "plan.json"], &[("GITHUB_RUN_ATTEMPT", "2")])?
            .status
            .success()
    );
    let branches = [
        "preflight",
        "policy",
        "base_matcher",
        "all_features",
        "clippy",
        "doctests",
        "tests",
        "corpus",
        "pr_schema",
    ];
    for branch in branches {
        let dir = format!("results/rust-dag-result-{branch}-42-1");
        fs::create_dir_all(fixture.root.join(&dir)).map_err(|e| e.to_string())?;
        let steps: serde_json::Map<String, serde_json::Value> = rows.iter().map(|row| Ok((row["step"].as_str().ok_or("step")?.to_owned(),serde_json::json!({"outcome":if row["branch"] == branch {"success"} else {"skipped"}})))).collect::<Result<_,String>>()?;
        success(fixture.call(
            &[
                "record",
                "plan.json",
                branch,
                &format!("{dir}/result-{branch}.json"),
                "0",
            ],
            &[(
                "RIPR_DAG_STEPS",
                &serde_json::Value::Object(steps).to_string(),
            )],
        )?)?;
    }
    success(fixture.call(&["finalize", "plan.json", "results"], &[])?)?;
    let result = fixture
        .root
        .join("results/rust-dag-result-tests-42-1/result-tests.json");
    let original = fs::read(&result).map_err(|e| e.to_string())?;
    for outcome in ["failure", "cancelled", "skipped", "missing"] {
        let mut value: serde_json::Value =
            serde_json::from_slice(&original).map_err(|e| e.to_string())?;
        let preparation = value["rows"]
            .as_array_mut()
            .ok_or("rows")?
            .iter_mut()
            .find(|row| row["step"] == "workspace_preparation")
            .ok_or("preparation missing")?;
        preparation["outcome"] = outcome.into();
        fs::write(&result, value.to_string()).map_err(|e| e.to_string())?;
        assert!(
            !fixture
                .call(&["finalize", "plan.json", "results"], &[])?
                .status
                .success(),
            "{outcome}"
        );
    }
    fs::write(&result, original).map_err(|e| e.to_string())?;
    fs::rename(&result, fixture.root.join("held-result.json")).map_err(|e| e.to_string())?;
    assert!(
        !fixture
            .call(&["finalize", "plan.json", "results"], &[])?
            .status
            .success()
    );
    fs::write(fixture.root.join("payload.txt"), "dirty subject\n").map_err(|e| e.to_string())?;
    assert!(
        !fixture
            .call(&["verify", "plan.json"], &[])?
            .status
            .success()
    );
    fs::remove_dir_all(fixture.root.parent().ok_or("fixture owner")?).map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn cheap_failure_refuses_allocation_before_any_product_producer() -> Result<(), String> {
    for formatted in [false, true] {
        let fixture = Fixture::with_format(formatted)?;
        let admitted = fixture.call(&["plan", "plan.json"], &[])?;
        assert!(admitted.status.success(), "plan must admit the fixture");
        assert!(
            admitted.stdout.is_empty(),
            "planning alone must not allocate costly branches"
        );
        let formatting = Command::new("cargo")
            .args(["fmt", "--check"])
            .current_dir(&fixture.root)
            .output()
            .map_err(|e| e.to_string())?;
        assert_eq!(
            formatting.status.success(),
            formatted,
            "real rustfmt must discriminate the seeded source"
        );
        let outcome = if formatting.status.success() {
            "success"
        } else {
            "failure"
        };
        let steps = serde_json::json!({"formatting":{"outcome":outcome},"agent_skills":{"outcome":"success"}}).to_string();
        success(fixture.call(
            &["record", "plan.json", "preflight", "preflight.json", "0"],
            &[("RIPR_DAG_STEPS", &steps)],
        )?)?;
        let allocation = fixture.call(&["allocate", "plan.json", "preflight.json"], &[])?;
        assert_eq!(
            allocation.status.success(),
            formatted,
            "no expensive branch may allocate after cheap failure"
        );
        if formatted {
            let matrix: serde_json::Value =
                serde_json::from_slice(&allocation.stdout).map_err(|e| e.to_string())?;
            let branches = matrix["include"].as_array().ok_or("missing allocation")?;
            let actual: std::collections::BTreeSet<_> = branches
                .iter()
                .filter_map(|row| row["branch"].as_str())
                .collect();
            assert_eq!(
                actual,
                [
                    "policy",
                    "base_matcher",
                    "all_features",
                    "clippy",
                    "doctests",
                    "tests",
                    "corpus",
                    "pr_schema"
                ]
                .into_iter()
                .collect()
            );
            assert_eq!(branches.len(), 8);
        } else {
            assert!(
                allocation.stdout.is_empty(),
                "failure must emit no allocation"
            );
        }
        let receipt = fs::read(fixture.root.join("preflight.json")).map_err(|e| e.to_string())?;
        for bad in ["failure", "skipped", "cancelled", "missing", "pending"] {
            let mut value: serde_json::Value =
                serde_json::from_slice(&receipt).map_err(|e| e.to_string())?;
            value["rows"][1]["outcome"] = bad.into();
            fs::write(fixture.root.join("preflight.json"), value.to_string())
                .map_err(|e| e.to_string())?;
            let denied = fixture.call(&["allocate", "plan.json", "preflight.json"], &[])?;
            assert!(
                !denied.status.success() && denied.stdout.is_empty(),
                "{bad} must not allocate"
            );
        }
        fs::remove_dir_all(fixture.root.parent().ok_or("fixture owner")?)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

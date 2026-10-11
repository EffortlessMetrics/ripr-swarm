use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    if from.is_dir() {
        std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
        for entry in std::fs::read_dir(from).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            copy(&entry.path(), &to.join(entry.file_name()))?;
        }
    } else {
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::copy(from, to).map_err(|e| e.to_string())?;
    }
    Ok(())
}

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Result<Self, String> {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let root = source.join("target").join(format!(
            "policy-frontdoor-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos()
        ));
        Self::from_paths(
            &source,
            root,
            &[
                "Cargo.toml",
                "Cargo.lock",
                "rust-toolchain.toml",
                "tools/repo-policy",
                ".github",
                ".agents",
                ".claude",
                "AGENTS.md",
                "AGENTS.override.md",
                "CLAUDE.md",
                "docs/ARCHITECTURE.md",
                "policy",
                "fixtures/boundary_gap/expected",
            ],
        )
    }
    fn from_paths(source: &Path, root: PathBuf, paths: &[&str]) -> Result<Self, String> {
        let fixture = Self(root);
        for path in paths {
            copy(&source.join(path), &fixture.0.join(path))?;
        }
        Ok(fixture)
    }
    fn command(&self, argument: &str) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_repo-policy"));
        command.arg(argument).current_dir(&self.0);
        command
    }
    fn run(&self, command: &str) -> Result<Output, String> {
        self.command(command).output().map_err(|e| e.to_string())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_dir_all(&self.0) {
            eprintln!("retain fixture {}: {e}", self.0.display());
        }
    }
}

#[test]
fn nested_frontdoors_use_runtime_workspace_root() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let nested = fixture.0.join("tools/repo-policy/src");
    for command in [
        "check-workflows",
        "check-agent-skills",
        "preflight",
        "verify-preflight",
    ] {
        let root_output = fixture.run(command)?;
        assert!(
            root_output.status.success(),
            "{}",
            String::from_utf8_lossy(&root_output.stderr)
        );
        let nested_output = fixture
            .command(command)
            .current_dir(&nested)
            .output()
            .map_err(|e| e.to_string())?;
        assert!(
            nested_output.status.success(),
            "{}",
            String::from_utf8_lossy(&nested_output.stderr)
        );
        assert_eq!(nested_output.stdout, root_output.stdout);
        assert_eq!(nested_output.stderr, root_output.stderr);
    }
    assert!(
        fixture
            .0
            .join("target/ripr/reports/policy-preflight.json")
            .is_file()
    );
    assert!(!nested.join("target").exists());
    let incidental = fixture.0.join("scratch/nested");
    std::fs::create_dir_all(&incidental).map_err(|e| e.to_string())?;
    for (file, text) in [
        ("Cargo.lock", "incidental lock"),
        ("rust-toolchain.toml", "incidental override"),
        ("Cargo.toml", "[package]\nname = \"nested\"\n"),
    ] {
        std::fs::write(incidental.join(file), text).map_err(|e| e.to_string())?;
    }
    assert!(
        fixture
            .command("preflight")
            .current_dir(&incidental)
            .output()
            .map_err(|e| e.to_string())?
            .status
            .success()
    );
    assert!(!incidental.join("target").exists());
    std::fs::write(
        fixture.0.join(".github/workflows/broken.yml"),
        "jobs:\n  broken:\n    steps:\n      - run: echo broken\n",
    )
    .map_err(|e| e.to_string())?;
    for command in ["check-workflows", "preflight", "verify-preflight"] {
        let root_output = fixture.run(command)?;
        let nested_output = fixture
            .command(command)
            .current_dir(&nested)
            .output()
            .map_err(|e| e.to_string())?;
        assert!(!root_output.status.success());
        assert_eq!(nested_output.status.code(), root_output.status.code());
        assert_eq!(nested_output.stderr, root_output.stderr);
    }
    assert!(
        !fixture
            .0
            .join("target/ripr/reports/policy-preflight.json")
            .exists()
    );
    std::fs::remove_file(fixture.0.join(".github/workflows/broken.yml"))
        .map_err(|e| e.to_string())?;
    let skill = fixture.0.join(".agents/skills/build-candidate/SKILL.md");
    let original = std::fs::read_to_string(&skill).map_err(|e| e.to_string())?;
    std::fs::write(
        &skill,
        original.replace(
            "candidate_contract:one_writer_worktree",
            "candidate_contract:wrong_writer",
        ),
    )
    .map_err(|e| e.to_string())?;
    for command in ["check-agent-skills", "preflight"] {
        let root_output = fixture.run(command)?;
        let nested_output = fixture
            .command(command)
            .current_dir(&nested)
            .output()
            .map_err(|e| e.to_string())?;
        assert!(!root_output.status.success());
        assert_eq!(nested_output.status.code(), root_output.status.code());
        assert_eq!(nested_output.stdout, root_output.stdout);
        assert_eq!(nested_output.stderr, root_output.stderr);
    }
    std::fs::write(skill, original).map_err(|e| e.to_string())?;
    assert!(fixture.run("preflight")?.status.success());
    let source = fixture.0.join("tools/repo-policy/src/main.rs");
    let mut bytes = std::fs::read(&source).map_err(|e| e.to_string())?;
    bytes.extend_from_slice(b"\n// deliberate stale source control\n");
    std::fs::write(source, bytes).map_err(|e| e.to_string())?;
    let output = fixture
        .command("preflight")
        .current_dir(&nested)
        .output()
        .map_err(|e| e.to_string())?;
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("stale or belongs to a different source tree")
    );
    assert!(
        !fixture
            .0
            .join("target/ripr/reports/policy-preflight.json")
            .exists()
    );
    Ok(())
}

#[test]
fn damaged_workspace_and_outside_directory_do_not_use_enclosing_source() -> Result<(), String> {
    let fixture = Fixture::new()?;
    assert!(fixture.run("preflight")?.status.success());
    std::fs::remove_file(fixture.0.join("tools/repo-policy/Cargo.toml"))
        .map_err(|e| e.to_string())?;
    let output = fixture
        .command("preflight")
        .current_dir(fixture.0.join("tools/repo-policy/src"))
        .output()
        .map_err(|e| e.to_string())?;
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("stale or belongs to a different source tree")
    );
    assert!(
        !fixture
            .0
            .join("target/ripr/reports/policy-preflight.json")
            .exists()
    );
    let manifest = fixture.0.join("Cargo.toml");
    let original = std::fs::read_to_string(&manifest).map_err(|e| e.to_string())?;
    assert!(original.contains("[workspace]\n"));
    std::fs::remove_dir_all(fixture.0.join("tools/repo-policy")).map_err(|e| e.to_string())?;
    let receipt = fixture.0.join("target/ripr/reports/policy-preflight.json");
    for header in [
        "[workspace] # comment",
        "[ workspace ] # comment",
        "[\"workspace\"] # comment",
        "['workspace'] # comment",
    ] {
        std::fs::write(
            &manifest,
            original.replace("[workspace]\n", &format!("{header}\n")),
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(&receipt, "obsolete control receipt").map_err(|e| e.to_string())?;
        let output = fixture
            .command("preflight")
            .current_dir(fixture.0.join(".agents"))
            .output()
            .map_err(|e| e.to_string())?;
        assert!(
            !output.status.success(),
            "damaged workspace skipped: {header}"
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("policy source identity:"));
        assert!(
            !receipt.exists(),
            "damaged workspace retained its receipt: {header}"
        );
    }
    let filesystem_root = Path::new(std::path::MAIN_SEPARATOR_STR);
    assert!(
        !filesystem_root.join("Cargo.toml").exists(),
        "outside control requires a filesystem root without a workspace"
    );
    let output = fixture
        .command("check-workflows")
        .current_dir(filesystem_root)
        .output()
        .map_err(|e| e.to_string())?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("policy workspace root not found"));
    Ok(())
}

#[test]
fn real_frontdoors_accept_then_reject_workflow_and_agent_contracts() -> Result<(), String> {
    let fixture = Fixture::new()?;
    for command in ["check-workflows", "check-agent-skills"] {
        let output = fixture.run(command)?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let path = fixture.0.join(".github/workflows/broken.yml");
    std::fs::write(
        &path,
        "jobs:\n  broken:\n    steps:\n      - run: echo broken\n",
    )
    .map_err(|e| e.to_string())?;
    let output = fixture.run("check-workflows")?;
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("missing workflow budget for .github/workflows/broken.yml")
    );
    std::fs::remove_file(path).map_err(|e| e.to_string())?;
    let path = fixture.0.join(".agents/skills/build-candidate/SKILL.md");
    let original = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    std::fs::write(
        &path,
        original.replace(
            "candidate_contract:one_writer_worktree",
            "candidate_contract:wrong_writer",
        ),
    )
    .map_err(|e| e.to_string())?;
    let output = fixture.run("check-agent-skills")?;
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(
            &std::fs::read(fixture.0.join("target/ripr/reports/agent-skills.json"))
                .map_err(|e| e.to_string())?
        )
        .contains("candidate_contract:one_writer_worktree")
    );
    Ok(())
}

#[test]
fn missing_and_unreadable_required_policy_inputs_fail() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let path = fixture.0.join("policy/workflow_allowlist.txt");
    std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    let output = fixture.run("check-workflows")?;
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("failed to read policy/workflow_allowlist.txt")
    );
    std::fs::create_dir(&path).map_err(|e| e.to_string())?;
    let output = fixture.run("check-workflows")?;
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("failed to read policy/workflow_allowlist.txt")
    );
    std::fs::remove_file(fixture.0.join("AGENTS.md")).map_err(|e| e.to_string())?;
    assert!(!fixture.run("check-agent-skills")?.status.success());
    std::fs::create_dir(fixture.0.join("AGENTS.md")).map_err(|e| e.to_string())?;
    assert!(!fixture.run("check-agent-skills")?.status.success());
    Ok(())
}

#[test]
fn retained_binary_rejects_changed_implementation_and_wrong_source() -> Result<(), String> {
    let fixture = Fixture::new()?;
    assert!(fixture.run("preflight")?.status.success());
    let receipt = fixture.0.join("target/ripr/reports/policy-preflight.json");
    assert!(receipt.exists());
    let path = fixture.0.join("tools/repo-policy/src/agent_skills.rs");
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    std::fs::write(&path, format!("{text}\n// changed implementation\n"))
        .map_err(|e| e.to_string())?;
    let output = fixture.run("check-agent-skills")?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("policy executable is stale"));
    let output = fixture.run("preflight")?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("policy executable is stale"));
    assert!(
        !receipt.exists(),
        "stale producer retained a success receipt"
    );
    std::fs::remove_file(path).map_err(|e| e.to_string())?;
    assert!(!fixture.run("check-workflows")?.status.success());
    Ok(())
}

#[test]
fn preflight_receipt_rejects_changed_ignored_inputs_missing_and_failed_producers()
-> Result<(), String> {
    let fixture = Fixture::new()?;
    let output = fixture.run("preflight")?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(fixture.run("verify-preflight")?.status.success());
    let receipt = fixture.0.join("target/ripr/reports/policy-preflight.json");
    // This file is outside any Git inventory; it must still invalidate proof.
    std::fs::write(
        fixture.0.join(".github/workflows/ignored.yml"),
        "jobs: {}\n",
    )
    .map_err(|e| e.to_string())?;
    assert!(!fixture.run("verify-preflight")?.status.success());
    assert!(!fixture.run("preflight")?.status.success());
    assert!(
        !receipt.exists(),
        "failed producer retained a success receipt"
    );
    std::fs::remove_file(fixture.0.join(".github/workflows/ignored.yml"))
        .map_err(|e| e.to_string())?;
    assert!(!fixture.run("verify-preflight")?.status.success());
    assert!(fixture.run("preflight")?.status.success());
    std::fs::write(&receipt, "{\"schema\":1,\"checks\":[]}\n").map_err(|e| e.to_string())?;
    assert!(!fixture.run("verify-preflight")?.status.success());
    Ok(())
}

#[test]
fn partial_fixture_is_removed_after_copy_error() -> Result<(), String> {
    let source = Fixture::new()?;
    let root = source.0.join("partial-copy");
    let missing = "missing-copy-input";
    assert!(!source.0.join(missing).exists());
    assert!(source.0.join("Cargo.toml").is_file());
    let result = Fixture::from_paths(&source.0, root.clone(), &["Cargo.toml", missing]);
    assert!(
        result.is_err(),
        "missing source must fail after copying Cargo.toml"
    );
    assert!(
        !root.exists(),
        "failed copy retained its partial destination"
    );
    Ok(())
}

#[test]
fn runtime_compiler_selection_rejects_a_changed_compiler() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let run = |compiler: &str| {
        fixture
            .command("preflight")
            .env("RUSTC", compiler)
            .output()
            .map_err(|e| e.to_string())
    };
    let output = run("rustc")?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = fixture.0.join("target/ripr/reports/policy-preflight.json");
    assert!(receipt.is_file());
    let output = run(env!("CARGO_BIN_EXE_repo-policy"))?;
    assert!(
        !output.status.success(),
        "changed RUSTC must invalidate the producer"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("xtask: policy executable toolchain differs")
    );
    assert!(
        !receipt.exists(),
        "changed compiler retained a success receipt"
    );
    Ok(())
}

#[test]
fn unavailable_compiler_reports_executable_and_recovery() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let empty_path = fixture.0.join("empty-path");
    std::fs::create_dir(&empty_path).map_err(|e| e.to_string())?;
    let missing_compiler = empty_path.join("missing-rustc");
    for compiler in [None, Some(missing_compiler.as_os_str())] {
        assert!(fixture.run("preflight")?.status.success());
        let receipt = fixture.0.join("target/ripr/reports/policy-preflight.json");
        assert!(receipt.is_file());
        let mut command = fixture.command("preflight");
        command.env("PATH", &empty_path).env_remove("RUSTC");
        if let Some(compiler) = compiler {
            command.env("RUSTC", compiler);
        }
        let output = command.output().map_err(|e| e.to_string())?;
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        let selected = compiler.unwrap_or_else(|| std::ffi::OsStr::new("rustc"));
        assert!(
            error.contains(&format!("could not execute {selected:?} -vV:")),
            "{error}"
        );
        assert!(
            error.contains("repository-pinned Rust toolchain"),
            "{error}"
        );
        assert!(error.contains("rust-toolchain.toml"), "{error}");
        assert!(error.contains("rerun with cargo policy"), "{error}");
        assert!(!error.contains("toolchain differs"), "{error}");
        assert!(!receipt.exists(), "unavailable compiler retained a receipt");
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn non_executable_compiler_preserves_permission_error() -> Result<(), String> {
    let fixture = Fixture::new()?;
    // A directory cannot be executed, even when the test runs as root.
    let compiler = fixture.0.join("compiler-directory");
    std::fs::create_dir(&compiler).map_err(|e| e.to_string())?;
    let output = fixture
        .command("preflight")
        .env("RUSTC", &compiler)
        .output()
        .map_err(|e| e.to_string())?;
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains(&format!(
            "could not execute {:?} -vV:",
            compiler.as_os_str()
        )),
        "{error}"
    );
    assert!(error.contains("Permission denied"), "{error}");
    assert!(!error.contains("not found"), "{error}");
    assert!(!error.contains("toolchain differs"), "{error}");
    Ok(())
}

#[test]
fn usage_lists_the_receipt_verification_frontdoor() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let output = fixture.run("unknown-command")?;
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.starts_with("xtask: usage:"));
    assert!(error.contains("verify-preflight"));
    Ok(())
}

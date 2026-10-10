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
        for path in [
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
        ] {
            copy(&source.join(path), &root.join(path))?;
        }
        Ok(Self(root))
    }
    fn run(&self, command: &str) -> Result<Output, String> {
        Command::new(env!("CARGO_BIN_EXE_repo-policy"))
            .arg(command)
            .current_dir(&self.0)
            .output()
            .map_err(|e| e.to_string())
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

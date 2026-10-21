#[path = "common/executable_index.rs"]
mod executable_index;

#[path = "../test_support.rs"]
mod test_support;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[path = "common/git_environment.rs"]
mod git_environment;

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
        let source = test_support::workspace_root()?;
        let root = test_support::scratch("policy-frontdoor")?;
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
    fn git(&self, args: &[&str]) -> Result<String, String> {
        let mut command = Command::new("git");
        command.args(args).current_dir(&self.0);
        git_environment::isolate_git_environment(&mut command);
        let output = command.output().map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(format!(
                "fixture git {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
    fn executable() -> Result<Self, String> {
        let fixture = Self::new()?;
        executable_index::initialize(&fixture.0, |args| fixture.git(args).map(|_| ()))?;
        Ok(fixture)
    }
    fn command(&self, argument: &str) -> Result<Command, String> {
        let mut command = Command::new(test_support::policy_binary(env!(
            "CARGO_BIN_EXE_repo-policy"
        ))?);
        command.arg(argument).current_dir(&self.0);
        git_environment::isolate_git_environment(&mut command);
        command.env(
            "GIT_CEILING_DIRECTORIES",
            self.0.parent().unwrap_or(&self.0),
        );
        Ok(command)
    }
    fn run(&self, command: &str) -> Result<Output, String> {
        self.command(command)?.output().map_err(|e| e.to_string())
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
            .command(command)?
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
            .command("preflight")?
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
            .command(command)?
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
            .command(command)?
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
        .command("preflight")?
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
        .command("preflight")?
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
            .command("preflight")?
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
        .command("check-workflows")?
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
    let run = |compiler: &std::ffi::OsStr| {
        fixture
            .command("preflight")?
            .env("RUSTC", compiler)
            .output()
            .map_err(|e| e.to_string())
    };
    let output = run(std::ffi::OsStr::new("rustc"))?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = fixture.0.join("target/ripr/reports/policy-preflight.json");
    assert!(receipt.is_file());
    let output = run(test_support::policy_binary(env!("CARGO_BIN_EXE_repo-policy"))?.as_os_str())?;
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
        let mut command = fixture.command("preflight")?;
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
        assert!(
            error
                .trim_end()
                .ends_with("; rerun with cargo policy preflight"),
            "{error}"
        );
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
        .command("preflight")?
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

#[test]
fn executable_frontdoor_matches_predecessor_decisions_and_report_bytes() -> Result<(), String> {
    let fixture = Fixture::executable()?;
    let allowlist = fixture.0.join("policy/executable_allowlist.txt");
    let report = fixture.0.join("target/ripr/reports/executable-files.md");
    for (case, rows, expected) in [
        (
            "allowed",
            "allowed\ndemoted\nunregistered\n",
            include_str!("executable-files/allowed.md"),
        ),
        (
            "unregistered",
            "allowed\ndemoted\n",
            include_str!("executable-files/unregistered.md"),
        ),
        (
            "missing",
            "allowed\ndemoted\nunregistered\ndeleted\n",
            include_str!("executable-files/missing.md"),
        ),
        (
            "demoted",
            "allowed\ndemoted\nunregistered\n",
            include_str!("executable-files/demoted.md"),
        ),
        (
            "mixed_order",
            "z-missing\nallowed\ndemoted\na-missing\n",
            include_str!("executable-files/mixed_order.md"),
        ),
    ] {
        if case == "demoted" {
            fixture.git(&["update-index", "--chmod=-x", "demoted"])?;
        }
        std::fs::write(&allowlist, rows).map_err(|e| e.to_string())?;
        let output = fixture.run("check-executable-files")?;
        assert_eq!(
            output.status.success(),
            case == "allowed",
            "case {case}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            std::fs::read(&report).map_err(|e| e.to_string())?,
            expected.as_bytes(),
            "case {case}"
        );
        if case == "allowed" {
            assert_eq!(
                output.stdout,
                b"check-executable-files: pass (target/ripr/reports/executable-files.md)\n"
            );
            assert!(output.stderr.is_empty());
        } else {
            assert!(output.stdout.is_empty());
            assert!(String::from_utf8_lossy(&output.stderr).starts_with("xtask: check-executable-files failed; see target/ripr/reports/executable-files.md\n"));
        }
        let nested = fixture
            .command("check-executable-files")?
            .current_dir(fixture.0.join("tools/repo-policy/src"))
            .output()
            .map_err(|e| e.to_string())?;
        assert_eq!(nested.status.code(), output.status.code());
        assert_eq!(nested.stdout, output.stdout);
        assert_eq!(nested.stderr, output.stderr);
    }
    std::fs::remove_file(&allowlist).map_err(|e| e.to_string())?;
    for name in ["allowed", "unregistered"] {
        fixture.git(&["update-index", "--chmod=-x", name])?;
    }
    assert!(fixture.run("check-executable-files")?.status.success());
    assert_eq!(
        std::fs::read(&report).map_err(|e| e.to_string())?,
        include_bytes!("executable-files/missing_optional.md")
    );
    std::fs::create_dir(&allowlist).map_err(|e| e.to_string())?;
    let output = fixture.run("check-executable-files")?;
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("failed to read policy/executable_allowlist.txt")
    );
    std::fs::remove_dir(&allowlist).map_err(|e| e.to_string())?;
    std::fs::remove_dir_all(fixture.0.join(".git")).map_err(|e| e.to_string())?;
    let output = fixture.run("check-executable-files")?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("git ls-files --stage failed with"));
    Ok(())
}

#[test]
fn executable_policy_reads_index_modes_even_with_a_verified_two_check_receipt() -> Result<(), String>
{
    let fixture = Fixture::executable()?;
    std::fs::write(
        fixture.0.join("policy/executable_allowlist.txt"),
        "allowed\ndemoted\nunregistered\n",
    )
    .map_err(|e| e.to_string())?;
    assert!(fixture.run("check-executable-files")?.status.success());
    let output = fixture.run("preflight")?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let path = fixture.0.join("target/ripr/reports/policy-preflight.json");
    let receipt = std::fs::read(&path).map_err(|e| e.to_string())?;
    let value: serde_json::Value = serde_json::from_slice(&receipt).map_err(|e| e.to_string())?;
    assert_eq!(
        value["checks"],
        serde_json::json!(["check-workflows", "check-agent-skills"])
    );
    let permissions = std::fs::metadata(fixture.0.join("demoted"))
        .map_err(|e| e.to_string())?
        .permissions();
    fixture.git(&["update-index", "--chmod=-x", "demoted"])?;
    assert!(
        fixture
            .git(&["ls-files", "--stage", "demoted"])?
            .starts_with("100644 ")
    );
    assert_eq!(
        std::fs::metadata(fixture.0.join("demoted"))
            .map_err(|e| e.to_string())?
            .permissions(),
        permissions
    );
    assert!(fixture.run("verify-preflight")?.status.success());
    assert_eq!(std::fs::read(&path).map_err(|e| e.to_string())?, receipt);
    let output = fixture.run("check-executable-files")?;
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("demoted allowlist entry is stale: mode is 100644")
    );
    fixture.git(&["update-index", "--chmod=+x", "demoted"])?;
    assert!(fixture.run("check-executable-files")?.status.success());
    Ok(())
}

#[cfg(unix)]
#[test]
fn unstaged_filesystem_permissions_do_not_change_executable_policy() -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::executable()?;
    std::fs::write(
        fixture.0.join("policy/executable_allowlist.txt"),
        "allowed\ndemoted\nunregistered\n",
    )
    .map_err(|e| e.to_string())?;
    let before = fixture.git(&["ls-files", "--stage"])?;
    assert!(before.contains("100755 "));
    assert!(fixture.run("check-executable-files")?.status.success());
    std::fs::set_permissions(
        fixture.0.join("ordinary"),
        std::fs::Permissions::from_mode(0o755),
    )
    .map_err(|e| e.to_string())?;
    std::fs::set_permissions(
        fixture.0.join("allowed"),
        std::fs::Permissions::from_mode(0o644),
    )
    .map_err(|e| e.to_string())?;
    assert_eq!(fixture.git(&["ls-files", "--stage"])?, before);
    assert!(fixture.run("check-executable-files")?.status.success());
    fixture.git(&["update-index", "--chmod=+x", "ordinary"])?;
    let output = fixture.run("check-executable-files")?;
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("checked-in executable file is not allowlisted: ordinary")
    );
    Ok(())
}

#[test]
fn executable_frontdoor_rejects_modified_implementation_and_wrong_source() -> Result<(), String> {
    let fixture = Fixture::executable()?;
    std::fs::write(
        fixture.0.join("policy/executable_allowlist.txt"),
        "allowed\ndemoted\nunregistered\n",
    )
    .map_err(|e| e.to_string())?;
    assert!(fixture.run("check-executable-files")?.status.success());
    for relative in ["tools/repo-policy/src/executable_files.rs", "Cargo.toml"] {
        let path = fixture.0.join(relative);
        let original = std::fs::read(&path).map_err(|e| e.to_string())?;
        let mut changed = original.clone();
        changed.extend_from_slice(b"\n// wrong-source control\n");
        std::fs::write(&path, changed).map_err(|e| e.to_string())?;
        let output = fixture.run("check-executable-files")?;
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("policy executable is stale or belongs to a different source tree")
        );
        std::fs::write(path, original).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[test]
fn workflow_input_reuse_keeps_unbudgeted_nested_yaml_in_scratch_checks() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let baseline = fixture.run("check-workflows")?;
    assert!(
        baseline.status.success(),
        "{}",
        String::from_utf8_lossy(&baseline.stderr)
    );
    let report = fixture.0.join("target/ripr/reports/workflows.md");
    let original_report = std::fs::read(&report).map_err(|e| e.to_string())?;
    let nested = fixture.0.join(".github/workflows/nested");
    std::fs::create_dir(&nested).map_err(|e| e.to_string())?;
    let path = nested.join("unbudgeted.yaml");
    std::fs::write(
        &path,
        "jobs:\n  bad:\n    steps:\n      - run: find /mnt/ci-scratch -mtime +3 -delete\n",
    )
    .map_err(|e| e.to_string())?;
    let failed = fixture.run("check-workflows")?;
    assert!(!failed.status.success());
    let error = String::from_utf8_lossy(&failed.stderr);
    assert!(
        error.contains("missing workflow budget for .github/workflows/nested/unbudgeted.yaml"),
        "{error}"
    );
    assert!(
        error.contains(
            ".github/workflows/nested/unbudgeted.yaml:4: selects /mnt/ci-scratch entries by age"
        ),
        "{error}"
    );
    // A non-YAML file must not enter either workflow denominator.
    std::fs::rename(&path, nested.join("unbudgeted.txt")).map_err(|e| e.to_string())?;
    let restored = fixture.run("check-workflows")?;
    assert!(
        restored.status.success(),
        "{}",
        String::from_utf8_lossy(&restored.stderr)
    );
    assert_eq!(
        std::fs::read(report).map_err(|e| e.to_string())?,
        original_report
    );
    Ok(())
}

#[test]
fn workflow_input_reuse_propagates_discovery_and_action_read_errors() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let workflows = fixture.0.join(".github/workflows");
    let saved = fixture.0.join(".github/saved-workflows");
    std::fs::rename(&workflows, &saved).map_err(|e| e.to_string())?;
    let output = fixture.run("check-workflows")?;
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("failed to inspect .github/workflows")
    );
    std::fs::rename(saved, workflows).map_err(|e| e.to_string())?;
    let action = fixture
        .0
        .join(repo_policy::ci_scratch::SCRATCH_LEASE_ACTION_PATH);
    let saved = action.with_extension("saved");
    std::fs::rename(&action, &saved).map_err(|e| e.to_string())?;
    std::fs::create_dir(&action).map_err(|e| e.to_string())?;
    let output = fixture.run("check-workflows")?;
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("failed to read .github/actions/ci-scratch-lease/action.yml")
    );
    Ok(())
}

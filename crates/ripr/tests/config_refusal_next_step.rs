//! `ripr config validate` refusals for a wrong `ripr.toml` name the fix, not
//! just the rule that failed. Each case writes one bad `ripr.toml` into an
//! empty scratch directory and pins the whole refusal clause, so a regression
//! that drops the next step fails here rather than only in the looser
//! action-word floor (`refusal_next_step.rs`).

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct ScratchDir(PathBuf);

impl Drop for ScratchDir {
    fn drop(&mut self) {
        if let Ok(()) = std::fs::remove_dir_all(&self.0) {}
    }
}

const CASES: &[(&str, &str)] = &[
    (
        "[[overrides]]\ncrate = \"alpha\"\npath = \"crates/alpha/**\"\nseverity.findings.weakly_exposed = \"info\"\n",
        "[[overrides]] entry 1 must set exactly one of crate or path, not both; remove one of them, or split the entry in two",
    ),
    (
        "[[overrides]]\nseverity.findings.weakly_exposed = \"info\"\n",
        "[[overrides]] entry 1 must set crate (Cargo package name) or path (repository-relative glob); add one of them to the entry",
    ),
    (
        "[[overrides]]\npath = \"crates/alpha/**\"\n",
        "[[overrides]] entry 1 must override severity or oracles; set at least one severity or oracles value in the entry, for example `severity.findings.weakly_exposed = \"info\"`, or remove the entry",
    ),
    (
        "[languages.rust]\ngenerated_file_patterns = [\"../generated/**/*.rs\"]\n",
        "languages.rust.generated_file_patterns `../generated/**/*.rs` must stay within the repository; remove the `..` segments and use a repository-relative path",
    ),
    (
        "[languages.rust]\ngenerated_file_patterns = [\"\"]\n",
        "languages.rust.generated_file_patterns must not be empty; give a repository-relative path",
    ),
    (
        "[languages.rust]\ngenerated_file_patterns = [\".\"]\n",
        "languages.rust.generated_file_patterns must identify a file pattern, not `.`; name the generated files, for example `src/generated/**`",
    ),
    (
        "[profiles.bun_ub]\ntest_roots = [\"test/**/*.ts\"]\nbridge_hints = \"\"\n",
        "profiles.bun_ub.bridge_hints must not be empty; give a repository-relative path",
    ),
    (
        "[languages.python]\nhandwritten_files = [\"src/*.py\"]\n",
        "languages.python.handwritten_files must contain exact file paths, not glob patterns; name each file by its repository-relative path",
    ),
    (
        "[languages.python]\nhandwritten_files = [\"src/app.txt\"]\n",
        "languages.python.handwritten_files must identify a Python `.py` file; remove `src/app.txt` or name a file with that extension",
    ),
    (
        "[lsp]\ndiagnostic_profile = \"quiet\"\n",
        "invalid lsp.diagnostic_profile \"quiet\"; set it to \"actionable\" or \"full\"",
    ),
    (
        "[[overrides]]\ncrate = \" \"\nseverity.findings.weakly_exposed = \"info\"\n",
        "[[overrides]] crate must not be empty; give the Cargo package name the override applies to, or use `path` instead",
    ),
];

#[test]
fn config_validate_refusals_name_their_fix() -> Result<(), String> {
    let mut failures = Vec::new();
    for (toml, expected) in CASES {
        let dir = std::env::temp_dir().join(format!(
            "ripr-config-refusal-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _guard = ScratchDir(dir.clone());
        std::fs::create_dir_all(&dir).map_err(|error| format!("create scratch dir: {error}"))?;
        std::fs::write(dir.join("ripr.toml"), toml)
            .map_err(|error| format!("write ripr.toml: {error}"))?;
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .current_dir(&dir)
            .args(["config", "validate", "--root", "."])
            .output()
            .map_err(|error| format!("spawn ripr: {error}"))?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        if output.status.code() != Some(2) || !stderr.contains(expected) {
            failures.push(format!(
                "ripr.toml:\n{toml}expected exit 2 and `{expected}`, got {:?}:\n{stderr}",
                output.status.code()
            ));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n---\n"))
    }
}

/// Following the override refusal's own example clears it, so the advice is
/// a working repair rather than a restatement of the rule.
#[test]
fn config_validate_accepts_the_override_repair_it_suggests() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!(
        "ripr-config-refusal-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _guard = ScratchDir(dir.clone());
    std::fs::create_dir_all(&dir).map_err(|error| format!("create scratch dir: {error}"))?;
    std::fs::write(
        dir.join("ripr.toml"),
        "[[overrides]]\npath = \"crates/alpha/**\"\nseverity.findings.weakly_exposed = \"info\"\n",
    )
    .map_err(|error| format!("write ripr.toml: {error}"))?;
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(&dir)
        .args(["config", "validate", "--root", "."])
        .output()
        .map_err(|error| format!("spawn ripr: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "expected the suggested override to validate, got {:?}:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

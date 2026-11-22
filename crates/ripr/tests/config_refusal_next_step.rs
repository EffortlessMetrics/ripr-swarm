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
        "[[overrides]] entry 1 must override severity or oracles; add a severity or oracles table to the entry, or remove it",
    ),
    (
        "[languages.rust]\ngenerated_file_patterns = [\"../generated/**/*.rs\"]\n",
        "languages.rust.generated_file_patterns `../generated/**/*.rs` must stay within the repository; remove the `..` segments and use a repository-relative path",
    ),
    (
        "[languages.rust]\ngenerated_file_patterns = [\"\"]\n",
        "languages.rust.generated_file_patterns must not be empty; give a repository-relative path, or remove the entry",
    ),
    (
        "[languages.python]\nhandwritten_files = [\"src/*.py\"]\n",
        "languages.python.handwritten_files must contain exact file paths, not glob patterns; name each file by its repository-relative path",
    ),
    (
        "[languages.python]\nhandwritten_files = [\"src/app.txt\"]\n",
        "languages.python.handwritten_files must identify a Python `.py` file; remove `src/app.txt` or name a file with that extension",
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

//! Mid-merge `ripr check` and `ripr doctor`: the conflict region of an
//! unmerged Rust file is not read as changes, so no finding is offered from it
//! (a wrong actionable signal is worse than a missed one), the run is
//! incomplete (exit 2), and doctor names the in-progress merge with the next
//! step. The live loader reads the worktree with zero-context hunks, which
//! split `<<<<<<<` from `=======`/`>>>>>>>` when the line between them equals
//! the base; this drives the real binary so that production path is exercised.
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn git(root: &Path, args: &[&str]) -> Result<std::process::ExitStatus, String> {
    Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .map(|output| output.status)
        .map_err(|error| format!("spawn git {args:?}: {error}"))
}

fn git_ok(root: &Path, args: &[&str]) -> Result<(), String> {
    git(root, args)?
        .success()
        .then_some(())
        .ok_or_else(|| format!("git {args:?} failed"))
}

const LIB: &str = "pub fn is_adult(age: u32) -> bool {\n    age >= 18\n}\n\n\
#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn adult() {\n        \
assert!(is_adult(30));\n    }\n}\n";

/// A Cargo crate whose `main` and `feat` branches both edit the one body line,
/// merged so that `src/lib.rs` is left unmerged with the unchanged signature
/// line before and the unchanged closing brace after the conflict region.
fn conflicted_merge(label: &str) -> Result<Scratch, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "ripr-conflicted-merge-{label}-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    let scratch = Scratch(root.clone());
    git_ok(&root, &["init", "-q", "-b", "main"])?;
    git_ok(&root, &["config", "user.email", "ripr@example.invalid"])?;
    git_ok(&root, &["config", "user.name", "ripr tests"])?;
    git_ok(&root, &["config", "merge.conflictStyle", "merge"])?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"conflicted\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(root.join(".gitignore"), "/target\n/ripr.toml\n")
        .map_err(|error| error.to_string())?;
    std::fs::write(root.join("src/lib.rs"), LIB).map_err(|error| error.to_string())?;
    git_ok(&root, &["add", "."])?;
    git_ok(&root, &["commit", "-q", "-m", "base"])?;
    git_ok(&root, &["checkout", "-q", "-b", "feat"])?;
    std::fs::write(root.join("src/lib.rs"), LIB.replace(">= 18", "> 18"))
        .map_err(|error| error.to_string())?;
    git_ok(&root, &["commit", "-q", "-am", "feat"])?;
    git_ok(&root, &["checkout", "-q", "main"])?;
    std::fs::write(root.join("src/lib.rs"), LIB.replace(">= 18", ">= 21"))
        .map_err(|error| error.to_string())?;
    git_ok(&root, &["commit", "-q", "-am", "main"])?;
    if git(&root, &["merge", "feat"])?.success() {
        return Err("fixture: the merge was expected to conflict".to_string());
    }
    let unmerged = Command::new("git")
        .current_dir(&root)
        .args(["ls-files", "-u"])
        .output()
        .map_err(|error| error.to_string())?;
    if unmerged.stdout.is_empty() {
        return Err("fixture: expected an unmerged src/lib.rs".to_string());
    }
    let text = std::fs::read_to_string(root.join("src/lib.rs")).map_err(|e| e.to_string())?;
    if !text.contains("<<<<<<<") || !text.contains(">>>>>>>") {
        return Err("fixture: expected conflict markers in src/lib.rs".to_string());
    }
    Ok(scratch)
}

fn ripr(root: &Path, args: &[&str]) -> Result<Output, String> {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(args)
        .arg("--root")
        .arg(root)
        .output()
        .map_err(|error| format!("spawn ripr {args:?}: {error}"))
}

#[test]
fn conflicted_rust_file_offers_no_findings_and_check_exits_incomplete() -> Result<(), String> {
    let scratch = conflicted_merge("check")?;
    let json = ripr(&scratch.0, &["check", "--format", "json"])?;
    let stdout = String::from_utf8_lossy(&json.stdout);
    // The incomplete-analysis exit contract (docs/EXIT_CODES.md) is unchanged.
    assert_eq!(json.status.code(), Some(2), "stdout: {stdout}");
    assert!(
        stdout.contains("\"kind\":\"unresolved_conflict_markers\""),
        "the conflict limitation must stay typed and visible: {stdout}"
    );
    assert!(
        stdout.contains("\"findings\":0") && stdout.contains("\"probes\":0"),
        "no finding or probe may come from a conflict region: {stdout}"
    );

    let human = ripr(&scratch.0, &["check"])?;
    let text = String::from_utf8_lossy(&human.stdout);
    assert_eq!(human.status.code(), Some(2), "stdout: {text}");
    assert!(text.contains("0 changed behaviors checked"), "{text}");
    assert!(
        !text.contains("Next step:") && !text.contains("Changed behavior:"),
        "no actionable repair signal from conflicted lines: {text}"
    );
    Ok(())
}

#[test]
fn doctor_names_the_in_progress_merge_and_its_next_step() -> Result<(), String> {
    let scratch = conflicted_merge("doctor")?;
    let output = ripr(&scratch.0, &["doctor"])?;
    let text = String::from_utf8_lossy(&output.stdout);
    for needle in ["mid-merge", "git merge --continue", "git merge --abort"] {
        assert!(text.contains(needle), "doctor must say {needle:?}: {text}");
    }

    // Negative control: the same repository after the merge is aborted says
    // nothing about an operation in progress.
    git_ok(&scratch.0, &["merge", "--abort"])?;
    let clean = ripr(&scratch.0, &["doctor"])?;
    let clean_text = String::from_utf8_lossy(&clean.stdout);
    assert!(!clean_text.contains("mid-merge"), "{clean_text}");
    Ok(())
}

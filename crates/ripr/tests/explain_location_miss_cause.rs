//! #14077: a `file:line` selector that matches no finding says why.
//!
//! `ripr explain` and `ripr context --at` used to answer six different causes
//! with one sentence. Each cause the run can establish now has its own
//! clause, built from the run's finding list, its analyzed root and the
//! producer's outcome counts:
//!
//! - the file has a finding at another line: the finding lines are named;
//! - no file exists at that path under the analyzed root: the root is named;
//! - the file exists and has no finding: the reason findings attach where
//!   they do, with the run's changed-file, changed-line and candidate counts;
//!   an incomplete run says instead that the file may not have been analyzed.
//!
//! The run carries counts, not the per-file changed-line set, so no clause
//! says whether the named line itself changed.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const CARGO_TOML: &str = "[package]\nname = \"lm\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
const OLD_SRC: &str = "pub fn price(qty: u32, unit: u32) -> u32 {\n    if qty > 10 {\n        qty * unit - 5\n    } else {\n        qty * unit\n    }\n}\n";
const NEW_SRC: &str = "pub fn price(qty: u32, unit: u32) -> u32 {\n    if qty >= 10 {\n        qty * unit - 5\n    } else {\n        qty * unit\n    }\n}\n// a trailing comment\n";
const TEST_SRC: &str =
    "use lm::price;\n#[test]\nfn small() {\n    assert_eq!(price(2, 3), 6);\n}\n";

const OTHER_LINE: &str = "No finding is at line";
const NO_FILE: &str = "No file exists at";
const NO_FINDING_IN_FILE: &str = "exists, but this run has no finding anywhere in it";
const CLAUSES: [&str; 3] = [OTHER_LINE, NO_FILE, NO_FINDING_IN_FILE];

struct Repo(PathBuf);

impl Repo {
    /// One committed change: the predicate at `src/lib.rs:2` and a comment
    /// appended at `src/lib.rs:8`. `tests/t.rs` and `Cargo.toml` are unchanged.
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-location-miss-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        let repo = Self(root);
        std::fs::create_dir_all(repo.0.join("src")).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(repo.0.join("tests")).map_err(|e| e.to_string())?;
        repo.write(".gitignore", "/target/\n")?;
        repo.write("Cargo.toml", CARGO_TOML)?;
        repo.write("src/lib.rs", OLD_SRC)?;
        repo.write("tests/t.rs", TEST_SRC)?;
        repo.git(&["-c", "init.templateDir=", "init", "-q"])?;
        repo.git(&["config", "user.email", "t@example.com"])?;
        repo.git(&["config", "user.name", "t"])?;
        repo.git(&["config", "commit.gpgSign", "false"])?;
        repo.git(&["add", "-A"])?;
        repo.git(&["commit", "-q", "--no-gpg-sign", "-m", "base"])?;
        repo.write("src/lib.rs", NEW_SRC)?;
        repo.git(&["commit", "-q", "--no-gpg-sign", "-am", "change"])?;
        Ok(repo)
    }

    fn write(&self, name: &str, text: &str) -> Result<(), String> {
        std::fs::write(self.0.join(name), text).map_err(|e| e.to_string())
    }

    fn git(&self, args: &[&str]) -> Result<(), String> {
        let output = Command::new("git")
            .current_dir(&self.0)
            .args(args)
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "git {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }

    /// Exit code and combined output of one `ripr` run at the repository root.
    fn ripr(&self, args: &[&str]) -> Result<(Option<i32>, String), String> {
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .current_dir(&self.0)
            .env("RIPR_QUIET", "1")
            .args(args)
            .output()
            .map_err(|e| e.to_string())?;
        Ok((
            output.status.code(),
            format!(
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
        ))
    }

    fn explain(&self, selector: &str) -> Result<(Option<i32>, String), String> {
        self.ripr(&["explain", "--base", "HEAD~1", selector])
    }

    fn context(&self, selector: &str) -> Result<(Option<i32>, String), String> {
        self.ripr(&["context", "--base", "HEAD~1", "--at", selector])
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _cleanup = std::fs::remove_dir_all(&self.0);
    }
}

/// Requires a miss (exit 2, the original sentence kept) carrying exactly the
/// one expected cause clause, or none when `expected` is `None`.
fn require_miss(
    label: &str,
    selector: &str,
    run: &(Option<i32>, String),
    expected: Option<&str>,
) -> Result<(), String> {
    let (code, text) = run;
    if *code != Some(2) {
        return Err(format!(
            "{label} {selector}: expected exit 2, got {code:?}: {text}"
        ));
    }
    if !text.contains(&format!("no finding matched {selector:?}")) {
        return Err(format!(
            "{label} {selector}: the miss sentence is gone: {text}"
        ));
    }
    for clause in CLAUSES {
        let wanted = expected == Some(clause);
        if text.contains(clause) != wanted {
            return Err(format!(
                "{label} {selector}: clause {clause:?} present={} but wanted={wanted}: {text}",
                !wanted
            ));
        }
    }
    Ok(())
}

fn require_contains(label: &str, text: &str, needle: &str) -> Result<(), String> {
    if text.contains(needle) {
        Ok(())
    } else {
        Err(format!("{label}: expected {needle:?} in: {text}"))
    }
}

#[test]
fn the_fixture_has_its_one_finding_at_the_changed_predicate() -> Result<(), String> {
    let repo = Repo::create()?;
    let (code, text) = repo.explain("src/lib.rs:2")?;
    if code != Some(0) {
        return Err(format!("src/lib.rs:2 must select the finding: {text}"));
    }
    require_contains("hit", &text, "qty >= 10")?;
    for clause in CLAUSES {
        if text.contains(clause) {
            return Err(format!("a hit carries no miss clause {clause:?}: {text}"));
        }
    }
    Ok(())
}

#[test]
fn a_miss_in_a_file_with_a_finding_names_the_finding_line() -> Result<(), String> {
    let repo = Repo::create()?;
    // An unchanged line, a changed comment line and a line past the end of
    // the file all sit in the file whose finding is at line 2.
    for (selector, line) in [
        ("src/lib.rs:3", 3),
        ("src/lib.rs:8", 8),
        ("src/lib.rs:999", 999),
    ] {
        for (label, run) in [
            ("explain", repo.explain(selector)?),
            ("context", repo.context(selector)?),
        ] {
            require_miss(label, selector, &run, Some(OTHER_LINE))?;
            require_contains(
                label,
                &run.1,
                &format!(
                    "No finding is at line {line}, but this run has a finding at another line of that file: it is at src/lib.rs:2. Retry with one of those locations"
                ),
            )?;
            require_contains(label, &run.1, "to list available finding ids")?;
        }
    }
    // A suffix spelling names the same file, as it does on a hit.
    let suffix = repo.explain("lib.rs:3")?;
    require_miss("explain", "lib.rs:3", &suffix, Some(OTHER_LINE))?;
    require_contains("explain", &suffix.1, "it is at src/lib.rs:2")
}

#[test]
fn a_miss_on_a_path_that_is_not_a_file_names_the_analyzed_root() -> Result<(), String> {
    let repo = Repo::create()?;
    for (label, run) in [
        ("explain", repo.explain("src/nope.rs:5")?),
        ("context", repo.context("src/nope.rs:5")?),
    ] {
        require_miss(label, "src/nope.rs:5", &run, Some(NO_FILE))?;
        require_contains(
            label,
            &run.1,
            "No file exists at \"src/nope.rs\" under the analyzed root `",
        )?;
        require_contains(
            label,
            &run.1,
            "a file location is read relative to that root",
        )?;
    }
    // A directory is not a file location either.
    let directory = repo.explain("src:1")?;
    require_miss("explain", "src:1", &directory, Some(NO_FILE))
}

#[test]
fn a_miss_in_an_existing_file_without_findings_gives_the_run_counts() -> Result<(), String> {
    let repo = Repo::create()?;
    // An unchanged test file and a file in no analyzed language.
    for selector in ["tests/t.rs:4", "Cargo.toml:1"] {
        for (label, run) in [
            ("explain", repo.explain(selector)?),
            ("context", repo.context(selector)?),
        ] {
            require_miss(label, selector, &run, Some(NO_FINDING_IN_FILE))?;
            require_contains(
                label,
                &run.1,
                "ripr reports a finding only on a changed line whose behavior it checks",
            )?;
            require_contains(
                label,
                &run.1,
                "This run read 1 changed file(s) and 3 changed line(s), 1 of them candidate line(s) ripr considers",
            )?;
        }
    }
    Ok(())
}

/// An incomplete run may never have analyzed the file, so it must not give
/// the "nothing there to report" reason. Rust is switched off here, so the
/// changed `src/lib.rs` has no finding because it was not analyzed.
#[test]
fn a_miss_in_an_incomplete_run_does_not_claim_the_file_was_analyzed() -> Result<(), String> {
    let repo = Repo::create()?;
    repo.write("ripr.toml", "[languages]\nenabled = [\"python\"]\n")?;
    for (label, run) in [
        ("explain", repo.explain("src/lib.rs:2")?),
        ("context", repo.context("src/lib.rs:2")?),
    ] {
        require_miss(label, "src/lib.rs:2", &run, Some(NO_FINDING_IN_FILE))?;
        require_contains(
            label,
            &run.1,
            "The analysis was incomplete, so that file may not have been analyzed",
        )?;
        for complete_only in ["ripr reports a finding only on", "This run read"] {
            if run.1.contains(complete_only) {
                return Err(format!(
                    "{label}: an incomplete run must not say {complete_only:?}: {}",
                    run.1
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn ids_malformed_locations_and_paths_leaving_the_root_get_no_cause_clause() -> Result<(), String> {
    let repo = Repo::create()?;
    // `Cargo.toml` exists at the root, so `src/../Cargo.toml` would read as
    // an existing file if a path with `..` were probed; it is not.
    for selector in [
        "probe:not-a-real-id",
        "gap:not-a-real-gap",
        "0123456789abcdef",
        "src/lib.rs:02",
        "src/../Cargo.toml:1",
        "../outside.rs:1",
        "/etc/hostname:1",
    ] {
        let run = repo.explain(selector)?;
        require_miss("explain", selector, &run, None)?;
    }
    Ok(())
}

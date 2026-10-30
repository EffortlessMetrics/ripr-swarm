//! #10702: `ripr check` and `ripr repair` agree on whether a static gap
//! remains, or the check screen prints the one reason they differ.
//!
//! - an `exposed` finding says "nothing to repair here", never "missing
//!   evidence" or "no seam", while repair reports the seam as gripped;
//! - check never says it found no seam in the finding's file when repair
//!   lists seams in that file.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const CARGO_TOML: &str = "[package]\nname = \"ar\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

struct Repo(PathBuf);

impl Repo {
    fn create(old_src: &str, new_src: &str, test: &str) -> Result<Self, String> {
        Self::create_with(old_src, new_src, test, true)
    }

    /// The change stays in the working tree, so the diff a bare `ripr check`
    /// reads is available to `ripr repair` too.
    fn create_uncommitted(old_src: &str, new_src: &str, test: &str) -> Result<Self, String> {
        Self::create_with(old_src, new_src, test, false)
    }

    fn create_with(
        old_src: &str,
        new_src: &str,
        test: &str,
        commit_change: bool,
    ) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-check-repair-agree-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        let repo = Self(root);
        std::fs::create_dir_all(repo.0.join("src")).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(repo.0.join("tests")).map_err(|e| e.to_string())?;
        repo.write(".gitignore", "/target/\n")?;
        repo.write("Cargo.toml", CARGO_TOML)?;
        repo.write("src/lib.rs", old_src)?;
        repo.write("tests/t.rs", test)?;
        repo.git(&["-c", "init.templateDir=", "init", "-q"])?;
        repo.git(&["config", "user.email", "t@example.com"])?;
        repo.git(&["config", "user.name", "t"])?;
        repo.git(&["config", "commit.gpgSign", "false"])?;
        repo.git(&["add", "-A"])?;
        repo.git(&["commit", "-q", "--no-gpg-sign", "-m", "base"])?;
        repo.write("src/lib.rs", new_src)?;
        if commit_change {
            repo.git(&["commit", "-q", "--no-gpg-sign", "-am", "change"])?;
        }
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

    fn ripr(&self, args: &[&str]) -> Result<String, String> {
        self.ripr_status(args).map(|(_, text)| text)
    }

    fn ripr_status(&self, args: &[&str]) -> Result<(Option<i32>, String), String> {
        let output: Output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .current_dir(&self.0)
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
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn exposed_change_says_nothing_to_repair_in_both_commands() -> Result<(), String> {
    let repo = Repo::create(
        "pub fn dbl(x: i32) -> i32 {\n    x * 2\n}\n",
        "pub fn dbl(x: i32) -> i32 {\n    x * 3\n}\n",
        "use ar::dbl;\n\n#[test]\nfn exact() {\n    assert_eq!(dbl(2), 6);\n}\n",
    )?;
    let check = repo.ripr(&["check", "--base", "HEAD~1"])?;
    assert!(
        check.contains("Static exposure: exposed"),
        "precondition, check reads exposed:\n{check}"
    );
    assert!(
        check.contains("no repair start offered: nothing to repair here")
            && check.contains("already notice this change (exposed)"),
        "check must say there is nothing to repair:\n{check}"
    );
    for leak in ["no seam in this finding's file", "missing", "found no seam"] {
        let advisory: Vec<&str> = check
            .lines()
            .filter(|line| line.contains("no repair start offered"))
            .collect();
        assert!(
            advisory.iter().all(|line| !line.contains(leak)),
            "an exposed finding must not print {leak:?}: {advisory:?}"
        );
    }
    let repair = repo.ripr(&["repair", "--root", "."])?;
    assert!(
        repair.contains("already grip it (exposed)") && !repair.contains("missing evidence"),
        "repair must agree that no gap remains:\n{repair}"
    );
    Ok(())
}

#[test]
fn check_never_reports_no_seam_where_repair_lists_seams_in_the_file() -> Result<(), String> {
    let repo = Repo::create(
        "pub fn label(n: u32) -> &'static str {\n    match n {\n        0 => \"zero\",\n        1 => \"one\",\n        _ => \"many\",\n    }\n}\n",
        "pub fn label(n: u32) -> &'static str {\n    match n {\n        0 => \"zero\",\n        1 => \"uno\",\n        _ => \"many\",\n    }\n}\n",
        "use ar::label;\n\n#[test]\nfn labels() {\n    assert!(!label(1).is_empty());\n}\n",
    )?;
    let repair = repo.ripr(&["repair", "--root", "."])?;
    assert!(
        repair.contains("visible seams under"),
        "precondition, repair lists seams in src/lib.rs:\n{repair}"
    );
    let check = repo.ripr(&["check", "--base", "HEAD~1"])?;
    assert!(
        !check.contains("found no seam") && !check.contains("no seam in this finding's file"),
        "check must not claim there is no seam while repair lists seams:\n{check}"
    );
    assert!(
        check.contains(
            "seams in this finding's file, but could not tie one to this finding's changed line"
        ),
        "check must name the one reason it cannot bind a seam:\n{check}"
    );
    Ok(())
}

const GRIPPED_BOUNDARY_OLD: &str =
    "pub fn f(x: i32) -> i32 {\n    let y = if x > 10 { 1 } else { 2 };\n    y\n}\n";
const GRIPPED_BOUNDARY_NEW: &str =
    "pub fn f(x: i32) -> i32 {\n    let y = if x >= 10 { 1 } else { 2 };\n    y\n}\n";
const GRIPPED_BOUNDARY_TEST: &str = "use ar::f;\n\n#[test]\nfn a() {\n    assert_eq!(f(20), 1);\n    assert_eq!(f(10), 1);\n    assert_eq!(f(9), 2);\n}\n";

#[test]
fn gap_finding_on_a_whole_gripped_seam_names_the_disagreement_not_no_binding() -> Result<(), String>
{
    let repo = Repo::create(
        GRIPPED_BOUNDARY_OLD,
        GRIPPED_BOUNDARY_NEW,
        GRIPPED_BOUNDARY_TEST,
    )?;
    let repair = repo.ripr(&["repair", "--root", "."])?;
    assert!(
        repair.contains("already grip it (exposed)"),
        "precondition, repair reads the whole seam as gripped:\n{repair}"
    );
    let check = repo.ripr(&["check", "--base", "HEAD~1"])?;
    assert!(
        !check.contains("Static exposure: exposed"),
        "precondition, check reads a gap or limit, not exposed:\n{check}"
    );
    assert!(
        check.contains("`ripr check` reads a gap or limit for this change")
            && check.contains("conflicting evidence"),
        "check must name the one difference from repair:\n{check}"
    );
    assert!(
        !check.contains("could not tie one to this finding's changed line"),
        "a gripped seam that names the finding is not a missing binding:\n{check}"
    );
    Ok(())
}

#[test]
fn unrelated_gripped_seam_does_not_stand_in_for_the_findings_seam() -> Result<(), String> {
    // `g` is fully gripped but is not the changed owner; the changed `f`
    // boundary is not gripped by any test, so the finding's seam is not
    // gripped and the disagreement wording must not appear.
    let old = format!("{GRIPPED_BOUNDARY_OLD}\npub fn g(x: i32) -> i32 {{\n    x * 2\n}}\n");
    let new = format!("{GRIPPED_BOUNDARY_NEW}\npub fn g(x: i32) -> i32 {{\n    x * 2\n}}\n");
    let repo = Repo::create(
        &old,
        &new,
        "use ar::{f, g};\n\n#[test]\nfn a() {\n    assert_eq!(g(2), 4);\n    assert_eq!(f(20), 1);\n}\n",
    )?;
    let check = repo.ripr(&["check", "--base", "HEAD~1"])?;
    assert!(
        !check.contains("`ripr check` reads a gap or limit for this change"),
        "an unrelated gripped seam is not this finding's seam:\n{check}"
    );
    Ok(())
}

const LOOP_OLD: &str = "pub fn sum_to(n: u32) -> u32 {\n    let mut acc = 0;\n    for i in 0..n {\n        acc += i;\n    }\n    acc\n}\n";
const LOOP_NEW: &str = "pub fn sum_to(n: u32) -> u32 {\n    let mut acc = 0;\n    for i in 1..n {\n        acc += i;\n    }\n    acc\n}\n";
const LOOP_TEST: &str =
    "use ar::sum_to;\n\n#[test]\nfn sum_to_zero() {\n    assert_eq!(sum_to(0), 0);\n}\n";

/// #11111: with no diff to scope to, bare repair's whole-seam "already grip
/// it (exposed)" names the seam it is about, so it is not read as a verdict
/// on the changed loop line that `ripr check` cannot classify.
#[test]
fn whole_seam_gripped_refusal_names_its_seam_not_the_change() -> Result<(), String> {
    let repo = Repo::create(LOOP_OLD, LOOP_NEW, LOOP_TEST)?;
    let check = repo.ripr(&["check", "--base", "HEAD~1"])?;
    assert!(
        check.contains("Changed behavior: for i in 1..n")
            && !check.contains("Static exposure: exposed"),
        "precondition, check does not read the changed loop as exposed:\n{check}"
    );
    let (code, repair) = repo.ripr_status(&["repair", "--root", "."])?;
    assert_eq!(code, Some(3), "repair must not start:\n{repair}");
    assert!(
        repair.contains("seam src/lib.rs:6 `acc`: ")
            && repair.contains("already grip it (exposed)"),
        "the gripped reason must name the seam it judges:\n{repair}"
    );
    Ok(())
}

const LARGEST_OLD: &str = "pub fn largest(xs: &[u32]) -> Option<u32> {\n    let mut it = xs.iter();\n    let mut best = *it.next()?;\n    for x in it {\n        if *x > best {\n            best = *x;\n        }\n    }\n    Some(best)\n}\n\npub fn dbl(x: i32) -> i32 {\n    x * 2\n}\n";
const LARGEST_NEW: &str = "pub fn largest(xs: &[u32]) -> Option<u32> {\n    let mut it = xs.iter();\n    let mut best = *it.next()?;\n    for x in it {\n        if *x >= best {\n            best = *x;\n        }\n    }\n    Some(best)\n}\n\npub fn dbl(x: i32) -> i32 {\n    x * 2\n}\n";
const LARGEST_TEST: &str = "use ar::{dbl, largest};\n\n#[test]\nfn largest_of_three() {\n    assert_eq!(largest(&[1, 5, 3]), Some(5));\n}\n\n#[test]\nfn doubles() {\n    assert_eq!(dbl(2), 4);\n}\n";

/// #11111: when the diff a bare `ripr check` reads is available, a gripped
/// seam the diff does not touch is counted, not cited: repair never says
/// "exposed" about an unrelated seam while check reads the change as a gap
/// or limit.
#[test]
fn bare_repair_does_not_cite_an_unrelated_gripped_seam_for_the_change() -> Result<(), String> {
    let repo = Repo::create_uncommitted(LARGEST_OLD, LARGEST_NEW, LARGEST_TEST)?;
    let check = repo.ripr(&["check"])?;
    assert!(
        check.contains("Changed behavior: *x >= best")
            && !check.contains("Static exposure: exposed"),
        "precondition, check reads the changed comparison as not exposed:\n{check}"
    );
    let (code, repair) = repo.ripr_status(&["repair", "--root", "."])?;
    assert_eq!(code, Some(3), "repair must not start:\n{repair}");
    assert!(
        !repair.contains("already grip it (exposed)"),
        "an unrelated gripped seam must not stand in for the change:\n{repair}"
    );
    assert!(
        repair.contains("outside your changed lines"),
        "the omitted gripped seams must be counted, not hidden:\n{repair}"
    );
    Ok(())
}

/// #11111 (contradiction 1): a bool owner whose whole tail is the changed
/// comparison carries a predicate and a return-value seam. When `ripr check`
/// reads both as exposed, bare repair must not start a repair on the
/// return-value seam: both commands read the change as gripped.
#[test]
fn exposed_boolean_tail_is_gripped_for_both_check_and_repair() -> Result<(), String> {
    let repo = Repo::create(
        "pub fn is_adult(age: u32) -> bool {\n    age > 18\n}\n",
        "pub fn is_adult(age: u32) -> bool {\n    age >= 18\n}\n",
        "use ar::is_adult;\n\n#[test]\nfn adult_at_boundary() {\n    assert!(is_adult(18));\n    assert!(!is_adult(17));\n}\n",
    )?;
    let check = repo.ripr(&["check", "--base", "HEAD~1"])?;
    assert!(
        check.contains("Static exposure: exposed")
            && check.contains("no repair start offered: nothing to repair here"),
        "precondition, check reads the change as exposed:\n{check}"
    );
    let (code, repair) = repo.ripr_status(&["repair", "--root", "."])?;
    assert_eq!(
        code,
        Some(3),
        "repair must not start on a change check reads as exposed:\n{repair}"
    );
    assert!(
        repair.contains("already grip it (exposed)") && !repair.contains("repair_ready=true"),
        "repair must agree that no gap remains:\n{repair}"
    );
    Ok(())
}

/// Assert the fixture holds the intended shape before any downstream claim.
fn assert_fixture_changes(repo: &Repo, needle: &str) -> Result<(), String> {
    let source = std::fs::read_to_string(repo.0.join("src/lib.rs")).map_err(|e| e.to_string())?;
    assert!(
        source.contains(needle),
        "fixture must hold the changed source {needle:?}:\n{source}"
    );
    Ok(())
}

/// #11111 group B: a changed constant whose consumer seam is a different,
/// unchanged line. `check` offers no repair start (no gap ID); bare `repair`
/// must refuse with the same plain reason instead of starting on the
/// consumer's seam, which static evidence does not tie to the change.
#[test]
fn string_const_change_check_and_bare_repair_both_decline() -> Result<(), String> {
    let repo = Repo::create_uncommitted(
        "pub const GREETING: &str = \"hello\";\n\npub fn greet(name: &str) -> String {\n    format!(\"{GREETING}, {name}\")\n}\n",
        "pub const GREETING: &str = \"hi\";\n\npub fn greet(name: &str) -> String {\n    format!(\"{GREETING}, {name}\")\n}\n",
        "use ar::greet;\n\n#[test]\nfn greet_mentions_name() {\n    assert!(greet(\"bob\").contains(\"bob\"));\n}\n",
    )?;
    assert_fixture_changes(&repo, "GREETING: &str = \"hi\"")?;
    let check = repo.ripr(&["check"])?;
    assert!(
        check.contains("Changed behavior: pub const GREETING")
            && check.contains("no repair start offered: this finding has no gap ID"),
        "precondition, check offers no repair start for the constant:\n{check}"
    );
    let (code, repair) = repo.ripr_status(&["repair", "--root", "."])?;
    assert_eq!(code, Some(3), "repair must not start:\n{repair}");
    assert!(
        !repair.contains("Repair prepared for seam"),
        "repair must not prepare a repair check declined:\n{repair}"
    );
    assert!(
        repair.contains("not on a changed line")
            && repair.contains("cannot tie your change to it")
            && repair.contains("`ripr check` reports the same"),
        "repair must give the plain reason:\n{repair}"
    );
    Ok(())
}

/// #11111 group B: a private helper's body changed; its public caller's seam
/// is the only eligible one but sits on an unchanged line. `check` cannot tie
/// a seam to the changed line; bare `repair` must refuse too.
#[test]
fn private_helper_change_check_and_bare_repair_both_decline() -> Result<(), String> {
    let repo = Repo::create_uncommitted(
        "fn tax(v: u32) -> u32 {\n    v / 10\n}\n\npub fn gross(v: u32) -> u32 {\n    v + tax(v)\n}\n",
        "fn tax(v: u32) -> u32 {\n    v / 5\n}\n\npub fn gross(v: u32) -> u32 {\n    v + tax(v)\n}\n",
        "use ar::gross;\n\n#[test]\nfn gross_exceeds_net() {\n    assert!(gross(100) > 100);\n}\n",
    )?;
    assert_fixture_changes(&repo, "v / 5")?;
    let check = repo.ripr(&["check"])?;
    assert!(
        check.contains("could not tie one to this finding's changed line"),
        "precondition, check cannot tie a seam to the change:\n{check}"
    );
    let (code, repair) = repo.ripr_status(&["repair", "--root", "."])?;
    assert_eq!(code, Some(3), "repair must not start:\n{repair}");
    assert!(
        !repair.contains("Repair prepared for seam")
            && repair.contains("not on a changed line")
            && repair.contains("`ripr check` reports the same"),
        "repair must decline with the plain reason:\n{repair}"
    );
    Ok(())
}

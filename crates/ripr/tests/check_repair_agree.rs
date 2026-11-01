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

    /// A two-package workspace (`wscore` owns `discount`, `app` calls it),
    /// with the change optionally left uncommitted. `files` are
    /// (relative path, text) pairs written before the base commit; the
    /// `wscore/src/lib.rs` text is rewritten to `new_src` afterwards.
    fn create_workspace(
        files: &[(&str, &str)],
        new_src: &str,
        commit_change: bool,
    ) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-check-repair-agree-ws-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        let repo = Self(root);
        repo.write(".gitignore", "/target/\n")?;
        for (name, text) in files {
            if let Some(parent) = std::path::Path::new(name).parent() {
                std::fs::create_dir_all(repo.0.join(parent)).map_err(|e| e.to_string())?;
            }
            repo.write(name, text)?;
        }
        repo.git(&["-c", "init.templateDir=", "init", "-q"])?;
        repo.git(&["config", "user.email", "t@example.com"])?;
        repo.git(&["config", "user.name", "t"])?;
        repo.git(&["config", "commit.gpgSign", "false"])?;
        repo.git(&["add", "-A"])?;
        repo.git(&["commit", "-q", "--no-gpg-sign", "-m", "base"])?;
        repo.write("wscore/src/lib.rs", new_src)?;
        if commit_change {
            repo.git(&["commit", "-q", "--no-gpg-sign", "-am", "change"])?;
        }
        Ok(repo)
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
    for leak in [
        "no spot in this finding's file",
        "no repairable spot in this finding's file",
        "no spot in this finding's file",
        "missing",
        "found no seam",
        "found no repairable spot",
    ] {
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
        repair.contains("already check it") && !repair.contains("missing evidence"),
        "repair must agree that no gap remains:\n{repair}"
    );
    Ok(())
}

#[test]
fn check_never_reports_no_seam_where_repair_lists_seams_in_the_file() -> Result<(), String> {
    let repo = Repo::create(
        "pub fn label(n: u32) -> &'static str {\n    match n {\n        0 => \"zero\",\n        1 => \"one\",\n        _ => \"many\",\n    }\n}\n",
        "pub fn label(n: u32) -> &'static str {\n    match n {\n        0 => \"zero\",\n        1 => \"uno\",\n        _ => \"many\",\n    }\n}\n",
        // The test takes the wildcard arm, so no test runs a literal arm
        // and repair lists seams instead of starting one (a test passing
        // `label(1)` is a match-arm repair start since #11111).
        "use ar::label;\n\n#[test]\nfn labels() {\n    assert!(!label(7).is_empty());\n}\n",
    )?;
    let repair = repo.ripr(&["repair", "--root", "."])?;
    assert!(
        repair.contains("spots in the code under"),
        "precondition, repair lists seams in src/lib.rs:\n{repair}"
    );
    let check = repo.ripr(&["check", "--base", "HEAD~1"])?;
    assert!(
        !check.contains("found no spot") && !check.contains("no spot in this finding's file"),
        "check must not claim there is no seam while repair lists seams:\n{check}"
    );
    assert!(
        check.contains(
            "spots in this finding's file, but could not tie one to this finding's changed line"
        ),
        "check must name the one reason it cannot bind a seam:\n{check}"
    );
    Ok(())
}

#[test]
fn untied_count_does_not_claim_gripped_sibling_is_repairable() -> Result<(), String> {
    let sibling = "\npub fn doubled(x: i32) -> i32 {\n    x * 2\n}\n";
    let old = format!(
        "pub fn label(n: u32) -> &'static str {{\n    match n {{\n        0 => \"zero\",\n        1 => \"one\",\n        _ => \"many\",\n    }}\n}}\n{sibling}"
    );
    let new = old.replace("1 => \"one\"", "1 => \"uno\"");
    let repo = Repo::create(
        &old,
        &new,
        "use ar::{doubled, label};\n\n#[test]\nfn labels() {\n    assert!(!label(1).is_empty());\n}\n\n#[test]\nfn doubled_exact() {\n    assert_eq!(doubled(2), 4);\n}\n",
    )?;
    let repair = repo.ripr(&["repair", "--root", "."])?;
    assert!(
        repair.contains("already grip it"),
        "gripped sibling precondition:\n{repair}"
    );
    let check = repo.ripr(&["check", "--base", "HEAD~1"])?;
    assert!(
        check.contains("spots in this finding's file, but could not tie one"),
        "unbound precondition:\n{check}"
    );
    assert!(
        !check.contains("repairable spots in this finding's file"),
        "count must not assert eligibility:\n{check}"
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
        repair.contains("already check it"),
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

/// #11111: with no diff to scope to, bare repair's whole-seam "already check
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
        repair.contains("the code at src/lib.rs:6 `acc`: ") && repair.contains("already check it"),
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
        !repair.contains("already check it"),
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
        repair.contains("already check it") && !repair.contains("Ready to repair."),
        "repair must agree that no gap remains:\n{repair}"
    );
    Ok(())
}

const CONST_OLD: &str = "pub const GREETING: &str = \"hello\";\n\npub fn greet(name: &str) -> String {\n    format!(\"{GREETING}, {name}\")\n}\n\npub fn shout(name: &str) -> String {\n    format!(\"{name}!\")\n}\n";
const CONST_NEW: &str = "pub const GREETING: &str = \"howdy\";\n\npub fn greet(name: &str) -> String {\n    format!(\"{GREETING}, {name}\")\n}\n\npub fn shout(name: &str) -> String {\n    format!(\"{name}!\")\n}\n";
const CONST_TEST: &str = "use ar::{greet, shout};\n\n#[test]\nfn greets() {\n    assert!(greet(\"a\").contains(\"a\"));\n    assert!(!shout(\"a\").is_empty());\n}\n";

/// #11111 (group 2): a changed string constant has no seam of its own, so
/// `ripr check` offers no repair start for it ("no gap ID"). Bare repair must
/// not start, or offer to choose, a repair on the unchanged functions that
/// merely sit in the same file: that is a different change than the one
/// check reported.
#[test]
fn changed_constant_with_no_seam_does_not_start_repair_elsewhere() -> Result<(), String> {
    let repo = Repo::create_uncommitted(CONST_OLD, CONST_NEW, CONST_TEST)?;
    let check = repo.ripr(&["check", "--base", "HEAD"])?;
    assert!(
        check.contains("Changed behavior: pub const GREETING")
            && check.contains("no repair start offered: this finding has no gap ID"),
        "precondition, check offers no repair start for the constant:\n{check}"
    );
    let (code, repair) = repo.ripr_status(&["repair", "--root", "."])?;
    assert_eq!(
        code,
        Some(3),
        "repair must not start a repair the changed finding does not have:\n{repair}"
    );
    assert!(
        !repair.contains("choose the change to repair")
            && !repair.contains("different repairs")
            && !repair.contains("repair_ready=true")
            && !repair.contains("Repair prepared"),
        "repair must not offer unrelated seams as the change's repair:\n{repair}"
    );
    assert!(
        repair.contains("your changed lines (src/lib.rs:1)")
            && repair.contains("not offered")
            && repair.contains("`ripr repair <seam-id>`"),
        "repair must say why nothing is offered and how to name a seam:\n{repair}"
    );
    Ok(())
}

/// The scoping applies only when the diff changes Rust production lines:
/// a diff that touches nothing but a test file keeps bare repair's
/// whole-repository selection, since nothing says which seam is "the change".
#[test]
fn test_only_diff_keeps_the_whole_repository_selection() -> Result<(), String> {
    let repo = Repo::create_uncommitted(CONST_OLD, CONST_OLD, CONST_TEST)?;
    repo.write("tests/t.rs", &format!("{CONST_TEST}\n// touched\n"))?;
    let (code, repair) = repo.ripr_status(&["repair", "--root", "."])?;
    assert_eq!(
        code,
        Some(3),
        "several seams still need a choice:\n{repair}"
    );
    assert!(
        repair.contains("ripr can start 2 different repairs")
            && !repair.contains("your changed lines")
            && !repair.contains("not offered"),
        "a test-only diff must not scope the selection:\n{repair}"
    );
    Ok(())
}

/// #11111 (review note on the return-value widening): only a tail that is
/// exactly the one boundary comparison shares the predicate seam's verdict.
/// With `age >= 18 && age < 65` the assertions pin the lower boundary but not
/// the upper operand, so check does not read the change as exposed and repair
/// must keep the whole-tail return-value seam as a repair candidate rather
/// than refusing it as gripped.
#[test]
fn compound_boolean_tail_is_not_read_as_exposed_by_repair() -> Result<(), String> {
    let repo = Repo::create(
        "pub fn is_adult(age: u32) -> bool {\n    age > 18 && age < 65\n}\n",
        "pub fn is_adult(age: u32) -> bool {\n    age >= 18 && age < 65\n}\n",
        "use ar::is_adult;\n\n#[test]\nfn adult() {\n    assert!(is_adult(19));\n    assert!(!is_adult(18));\n}\n",
    )?;
    let check = repo.ripr(&["check", "--base", "HEAD~1"])?;
    assert!(
        !check.contains("nothing to repair here"),
        "precondition, check does not read the compound change as exposed:\n{check}"
    );
    let (code, repair) = repo.ripr_status(&["repair", "--root", "."])?;
    assert_eq!(code, Some(3), "several candidates need a choice:\n{repair}");
    assert!(
        repair.contains("`age >= 18 && age < 65`") && !repair.contains("already check it"),
        "the whole-tail seam must stay a candidate, not read as gripped:\n{repair}"
    );
    Ok(())
}

// Group C / D fixtures (#11111). Every case is built twice: with the change
// committed (a bare `ripr repair` has no diff to scope to) and left in the
// working tree (the diff a bare `ripr check` reads is available).

const ERR_OLD: &str = "#[derive(Debug, PartialEq)]\npub enum ParseError {\n    Empty,\n    TooLong,\n}\n\npub fn parse(s: &str) -> Result<usize, ParseError> {\n    if s.is_empty() {\n        return Err(ParseError::Empty);\n    }\n    if s.len() > 10 {\n        return Err(ParseError::Empty);\n    }\n    Ok(s.len())\n}\n";
const ERR_NEW: &str = "#[derive(Debug, PartialEq)]\npub enum ParseError {\n    Empty,\n    TooLong,\n}\n\npub fn parse(s: &str) -> Result<usize, ParseError> {\n    if s.is_empty() {\n        return Err(ParseError::Empty);\n    }\n    if s.len() > 10 {\n        return Err(ParseError::TooLong);\n    }\n    Ok(s.len())\n}\n";
const ERR_TEST: &str = "use ar::{parse, ParseError};\n\n#[test]\nfn long_input_is_too_long() {\n    assert_eq!(parse(\"abcdefghijklmnop\"), Err(ParseError::TooLong));\n}\n";

const WS_ROOT: &str = "[workspace]\nmembers = [\"wscore\", \"app\"]\nresolver = \"2\"\n";
const WS_APP_TOML: &str = "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nwscore = { path = \"../wscore\" }\n";
const WS_CORE_TOML: &str =
    "[package]\nname = \"wscore\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
const WS_APP_SRC: &str = "pub fn checkout(price: u32) -> u32 {\n    wscore::discount(price)\n}\n";
const WS_OLD: &str = "pub fn discount(price: u32) -> u32 {\n    if price > 100 {\n        price - 10\n    } else {\n        price\n    }\n}\n";
const WS_NEW: &str = "pub fn discount(price: u32) -> u32 {\n    if price >= 100 {\n        price - 10\n    } else {\n        price\n    }\n}\n";
const WS_CORE_TEST: &str = "use wscore::discount;\n\n#[test]\nfn discount_at_boundary() {\n    assert_eq!(discount(100), 90);\n    assert_eq!(discount(50), 50);\n}\n";
const WS_APP_TEST: &str = "use app::checkout;\n\n#[test]\nfn checkout_at_boundary() {\n    assert_eq!(checkout(100), 90);\n}\n";

fn workspace_repo(core_tests: bool, commit_change: bool) -> Result<Repo, String> {
    let mut files = vec![
        ("Cargo.toml", WS_ROOT),
        ("app/Cargo.toml", WS_APP_TOML),
        ("app/src/lib.rs", WS_APP_SRC),
        ("wscore/Cargo.toml", WS_CORE_TOML),
        ("wscore/src/lib.rs", WS_OLD),
    ];
    if core_tests {
        files.push(("wscore/tests/t.rs", WS_CORE_TEST));
    } else {
        files.push(("app/tests/t.rs", WS_APP_TEST));
    }
    Repo::create_workspace(&files, WS_NEW, commit_change)
}

/// The repair text for a fixture, with the refusal asserted (exit 3).
fn refused_repair(repo: &Repo) -> Result<String, String> {
    let (code, repair) = repo.ripr_status(&["repair", "--root", "."])?;
    assert_eq!(code, Some(3), "repair must not start:\n{repair}");
    Ok(repair)
}

/// Words that were internal jargon in the not-ready text (#11111).
fn assert_plain(repair: &str) {
    for jargon in [
        "repair-eligible",
        "repair target under",
        "current policy",
        "visible seam",
        "(exposed)",
    ] {
        assert!(
            !repair.contains(jargon),
            "not-ready text must be plain, found {jargon:?}:\n{repair}"
        );
    }
}

/// Group C: `ripr check` reads the change as exposed, so a bare repair that
/// can read the same diff says only that it is already gripped. It must not
/// ask for evidence or a `tests/` layout for code the change never touched.
#[test]
fn exposed_error_variant_change_repair_says_only_gripped() -> Result<(), String> {
    let committed = Repo::create(ERR_OLD, ERR_NEW, ERR_TEST)?;
    let check = committed.ripr(&["check", "--base", "HEAD~1"])?;
    assert!(
        check.contains("Static exposure: exposed")
            && check.contains("Changed behavior: return Err(ParseError::TooLong)"),
        "precondition, check reads the changed error as exposed:\n{check}"
    );
    // Negative control: with no diff to scope to, the same fixture does have
    // seams whose route is not ready, so the scoped output below is a
    // decision and not an absence of seams.
    let unscoped = refused_repair(&committed)?;
    assert!(
        unscoped.contains("missing evidence"),
        "precondition, the fixture has not-ready seams:\n{unscoped}"
    );
    assert_plain(&unscoped);

    let repo = Repo::create_uncommitted(ERR_OLD, ERR_NEW, ERR_TEST)?;
    let check = repo.ripr(&["check"])?;
    assert!(
        check.contains("Static exposure: exposed"),
        "precondition, check reads the working-tree change as exposed:\n{check}"
    );
    let repair = refused_repair(&repo)?;
    // Since #11277 the changed return-value seam is graded gripped directly, so
    // the wrapper wording is not expected here (the wrapper match is pinned by
    // `wrapper_match_is_exact_and_never_looser`). This still discriminates the
    // all-gripped-only presentation: the unscoped control above has not-ready
    // spots, the scoped output must not.
    assert!(
        repair.contains("already check it")
            && !repair.contains("missing evidence")
            && !repair.contains("tests/ layout")
            && !repair.contains("repair route not ready"),
        "a gripped change must be reported as gripped only:\n{repair}"
    );
    assert_plain(&repair);
    Ok(())
}

#[test]
fn exposed_workspace_change_repair_says_only_gripped() -> Result<(), String> {
    let committed = workspace_repo(true, true)?;
    let check = committed.ripr(&["check", "--base", "HEAD~1"])?;
    assert!(
        check.contains("Static exposure: exposed")
            && check.contains("Changed behavior: price >= 100"),
        "precondition, check reads the changed comparison as exposed:\n{check}"
    );
    let unscoped = refused_repair(&committed)?;
    assert!(
        unscoped.contains("tests/ layout") && unscoped.contains("already check it"),
        "precondition, the other package lacks a tests/ folder:\n{unscoped}"
    );
    assert_plain(&unscoped);

    let repo = workspace_repo(true, false)?;
    let check = repo.ripr(&["check"])?;
    assert!(
        check.contains("Static exposure: exposed"),
        "precondition, check reads the working-tree change as exposed:\n{check}"
    );
    let repair = refused_repair(&repo)?;
    assert!(
        repair.contains("already check it")
            && !repair.contains("missing evidence")
            && !repair.contains("tests/ layout")
            && !repair.contains("repair route not ready"),
        "a gripped change must be reported as gripped only:\n{repair}"
    );
    assert_plain(&repair);
    Ok(())
}

const BUILDER_OLD: &str = "#[derive(Default)]\npub struct Req {\n    pub url: String,\n    pub timeout: u32,\n}\n\nimpl Req {\n    pub fn new(url: &str) -> Self {\n        Req {\n            url: url.to_string(),\n            timeout: 30,\n        }\n    }\n\n    pub fn timeout(mut self, t: u32) -> Self {\n        self.timeout = t;\n        self\n    }\n}\n";
const BUILDER_NEW: &str = "#[derive(Default)]\npub struct Req {\n    pub url: String,\n    pub timeout: u32,\n}\n\nimpl Req {\n    pub fn new(url: &str) -> Self {\n        Req {\n            url: url.to_string(),\n            timeout: 30,\n        }\n    }\n\n    pub fn timeout(mut self, t: u32) -> Self {\n        self.timeout = t * 2;\n        self\n    }\n}\n";
const BUILDER_TEST: &str = "use ar::Req;\n\n#[test]\nfn builder_sets_url() {\n    let r = Req::new(\"http://x\").timeout(10);\n    assert_eq!(r.url, \"http://x\");\n}\n";

const OPTION_OLD: &str = "pub fn first_even(xs: &[i32]) -> Option<i32> {\n    xs.iter().copied().find(|x| x % 2 == 0)\n}\n";
const OPTION_NEW: &str = "pub fn first_even(xs: &[i32]) -> Option<i32> {\n    xs.iter().copied().find(|x| x % 2 == 0).or(Some(0))\n}\n";
const OPTION_TEST: &str = "use ar::first_even;\n\n#[test]\nfn none_when_all_odd() {\n    assert_eq!(first_even(&[1, 3]), None);\n    assert_eq!(first_even(&[1, 4]), Some(4));\n}\n";

const GENERIC_OLD: &str = "pub fn largest<T: PartialOrd + Copy>(xs: &[T]) -> Option<T> {\n    let mut it = xs.iter();\n    let mut best = *it.next()?;\n    for x in it {\n        if *x > best {\n            best = *x;\n        }\n    }\n    Some(best)\n}\n";
const GENERIC_NEW: &str = "pub fn largest<T: PartialOrd + Copy>(xs: &[T]) -> Option<T> {\n    let mut it = xs.iter();\n    let mut best = *it.next()?;\n    for x in it {\n        if *x >= best {\n            best = *x;\n        }\n    }\n    Some(best)\n}\n";
const GENERIC_TEST: &str = "use ar::largest;\n\n#[test]\nfn largest_of_three() {\n    assert_eq!(largest(&[1, 5, 3]), Some(5));\n}\n";

/// Group D: `ripr check` reads the change as limited or unknown, so the
/// repair refusal never claims the change is gripped. A gripped seam that is
/// a different check is counted as such, not cited as the verdict.
fn assert_limited_change_is_not_called_gripped(
    old: &str,
    new: &str,
    test: &str,
    changed_line: &str,
    gripped_note: &str,
) -> Result<(), String> {
    let committed = Repo::create(old, new, test)?;
    let check = committed.ripr(&["check", "--base", "HEAD~1"])?;
    assert!(
        check.contains(changed_line) && !check.contains("Static exposure: exposed"),
        "precondition, check does not read {changed_line:?} as exposed:\n{check}"
    );
    // No diff to scope to: a gripped seam is named as itself, in plain words,
    // and says it speaks only to that spot.
    let unscoped = refused_repair(&committed)?;
    assert_plain(&unscoped);
    assert!(
        unscoped.contains("already check it"),
        "precondition, the fixture has a gripped spot:\n{unscoped}"
    );
    assert!(
        unscoped.contains("this speaks only to this spot, not to what you changed"),
        "a gripped seam must say it is not the verdict on the change:\n{unscoped}"
    );

    let repo = Repo::create_uncommitted(old, new, test)?;
    let check = repo.ripr(&["check"])?;
    assert!(
        check.contains(changed_line) && !check.contains("Static exposure: exposed"),
        "precondition, check reads the working-tree change as not exposed:\n{check}"
    );
    let repair = refused_repair(&repo)?;
    assert!(
        !repair.contains("already check it"),
        "a limited change must not be called gripped:\n{repair}"
    );
    // The gripped spot is counted, not cited: either as a different check
    // when it sits on the changed lines, or as outside them. Main listed it
    // by name as "already check it".
    assert!(
        repair.contains(gripped_note),
        "the gripped spot must be counted, not cited:\n{repair}"
    );
    // Location-specific: a spot counted as outside the change must not also
    // read as a different check on it, and the reverse.
    let on_change = gripped_note.contains("different check");
    assert_eq!(
        repair.contains("different check than the one still open"),
        on_change,
        "the gripped spot must be counted only where it sits:\n{repair}"
    );
    assert_eq!(
        repair.contains("outside your changed lines")
            && repair.contains("already noticed by existing tests and not listed"),
        !on_change,
        "the gripped spot must be counted only where it sits:\n{repair}"
    );
    assert_plain(&repair);
    Ok(())
}

#[test]
fn limited_loop_change_is_not_called_gripped_by_repair() -> Result<(), String> {
    assert_limited_change_is_not_called_gripped(
        LOOP_OLD,
        LOOP_NEW,
        LOOP_TEST,
        "Changed behavior: for i in 1..n",
        "outside your changed lines is already noticed by existing tests and not listed",
    )
}

#[test]
fn limited_builder_change_is_not_called_gripped_by_repair() -> Result<(), String> {
    assert_limited_change_is_not_called_gripped(
        BUILDER_OLD,
        BUILDER_NEW,
        BUILDER_TEST,
        "Changed behavior: self.timeout = t * 2",
        "outside your changed lines are already noticed by existing tests and not listed",
    )
}

#[test]
fn limited_option_change_is_not_called_gripped_by_repair() -> Result<(), String> {
    assert_limited_change_is_not_called_gripped(
        OPTION_OLD,
        OPTION_NEW,
        OPTION_TEST,
        "Changed behavior: x % 2 == 0",
        "different check than the one still open",
    )
}

#[test]
fn limited_generic_change_is_not_called_gripped_by_repair() -> Result<(), String> {
    assert_limited_change_is_not_called_gripped(
        GENERIC_OLD,
        GENERIC_NEW,
        GENERIC_TEST,
        "Changed behavior: *x >= best",
        "outside your changed lines is already noticed by existing tests and not listed",
    )
}

#[test]
fn unreached_workspace_change_is_not_called_gripped_by_repair() -> Result<(), String> {
    let committed = workspace_repo(false, true)?;
    let check = committed.ripr(&["check", "--base", "HEAD~1"])?;
    assert!(
        check.contains("Static exposure: no path")
            && check.contains("Changed behavior: price >= 100"),
        "precondition, check finds no path from the other package's test:\n{check}"
    );
    let unscoped = refused_repair(&committed)?;
    assert_plain(&unscoped);
    assert!(
        unscoped.contains(
            "ripr does not start an automatic repair here because it could not establish"
        ) && !unscoped.contains("this kind of finding"),
        "the class refusal must be plain and name what ripr could not establish:\n{unscoped}"
    );

    let repo = workspace_repo(false, false)?;
    let check = repo.ripr(&["check"])?;
    assert!(
        check.contains("Static exposure: no path"),
        "precondition, check reads the working-tree change as no path:\n{check}"
    );
    let repair = refused_repair(&repo)?;
    assert!(
        !repair.contains("already check it"),
        "a change with no static path must not be called gripped:\n{repair}"
    );
    assert_plain(&repair);
    Ok(())
}

const GUARD_OLD: &str = "pub fn check_age(age: u32) -> u32 {\n    if age > 150 {\n        panic!(\"too old\");\n    }\n    age\n}\n";
const GUARD_NEW: &str = "pub fn check_age(age: u32) -> u32 {\n    if age > 120 {\n        panic!(\"too old\");\n    }\n    age\n}\n";
const GUARD_TEST: &str =
    "use ar::check_age;\n\n#[test]\n#[should_panic]\nfn too_old() {\n    check_age(200);\n}\n";

/// #11111 (group 4, panic-guard): the changed guard is a named static
/// limitation, while the unchanged `age` tail is a separate seam that is not
/// ready. Repair used to print both reasons unlabelled, so one change read as
/// "no gap here" and "missing assertion" at once. With the diff available the
/// unchanged seam is counted, not listed, and the changed seam's reason names
/// its seam.
#[test]
fn bare_repair_does_not_list_an_unchanged_seams_missing_evidence_for_the_change()
-> Result<(), String> {
    let repo = Repo::create_uncommitted(GUARD_OLD, GUARD_NEW, GUARD_TEST)?;
    let check = repo.ripr(&["check", "--base", "HEAD"])?;
    assert!(
        check.contains("ripr does not report a gap here"),
        "precondition, check names the static limitation:\n{check}"
    );
    let (code, repair) = repo.ripr_status(&["repair", "--root", "."])?;
    assert_eq!(code, Some(3), "repair must not start:\n{repair}");
    assert!(
        repair.contains("the code at src/lib.rs:2 `age > 120`: ripr does not report a gap here"),
        "the changed seam's reason must name its seam:\n{repair}"
    );
    assert!(
        !repair.contains("a test assertion that checks the changed return value"),
        "the unchanged tail's missing evidence must not read as the change's verdict:\n{repair}"
    );
    assert!(
        repair.contains("outside your changed lines cannot start a repair either"),
        "the unchanged blocked seam must be counted, not hidden:\n{repair}"
    );
    Ok(())
}

/// The same shape with the change committed: no diff is available, so repair
/// keeps the whole-repository view, and each reason names the seam it is
/// about instead of sitting beside the other as a second verdict.
#[test]
fn whole_repository_refusals_name_the_seam_each_reason_is_about() -> Result<(), String> {
    let repo = Repo::create(GUARD_OLD, GUARD_NEW, GUARD_TEST)?;
    let (code, repair) = repo.ripr_status(&["repair", "--root", "."])?;
    assert_eq!(code, Some(3), "repair must not start:\n{repair}");
    assert!(
        repair.contains("the code at src/lib.rs:2 `age > 120`: ripr does not report a gap here")
            && repair.contains("the code at src/lib.rs:5 `age`: repair route not ready"),
        "each refusal must name its own seam:\n{repair}"
    );
    assert!(
        !repair.contains("outside your changed lines"),
        "without a diff nothing is scoped:\n{repair}"
    );
    Ok(())
}

const EXACT_ERR_OLD: &str = "#[derive(Debug, PartialEq)]\npub enum E {\n    Neg,\n    Bad,\n}\n\npub fn fail(x: i32) -> Result<i32, E> {\n    return Err(E::Neg);\n}\n";
const EXACT_ERR_NEW: &str = "#[derive(Debug, PartialEq)]\npub enum E {\n    Neg,\n    Bad,\n}\n\npub fn fail(x: i32) -> Result<i32, E> {\n    return Err(E::Bad);\n}\n";

/// A changed `return Err(E::Bad)` whose test pins exactly `E::Bad`: the
/// error-variant seam is strongly gripped and check reads the change as
/// exposed, so repair must grade the return-value seam over the same
/// expression as gripped too, not as "missing evidence" (#11111,
/// error-variant-strong).
#[test]
fn exact_err_variant_pin_grips_the_return_value_seam_for_check_and_repair() -> Result<(), String> {
    let repo = Repo::create(
        EXACT_ERR_OLD,
        EXACT_ERR_NEW,
        "use ar::{fail, E};\n\n#[test]\nfn fails_bad() {\n    assert!(matches!(fail(1), Err(E::Bad)));\n}\n",
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
        repair.contains(
            "the code at src/lib.rs:8 `return Err(E::Bad)`: existing tests already check it"
        ) && !repair.contains("missing evidence")
            && !repair.contains("repair_ready=true"),
        "repair must agree that no gap remains on the return-value seam:\n{repair}"
    );
    Ok(())
}

/// Discriminating negative: a pin on the sibling variant does not grip the
/// changed `Err(E::Bad)`, so the return-value seam keeps its missing-evidence
/// refusal exactly as before and no start is added.
#[test]
fn sibling_err_variant_pin_leaves_the_return_value_seam_weak() -> Result<(), String> {
    let repo = Repo::create(
        EXACT_ERR_OLD,
        EXACT_ERR_NEW,
        "use ar::{fail, E};\n\n#[test]\nfn fails_neg() {\n    assert!(matches!(fail(1), Err(E::Neg)));\n}\n",
    )?;
    let (_, repair) = repo.ripr_status(&["repair", "--root", "."])?;
    assert!(
        repair.contains(
            "the code at src/lib.rs:8 `return Err(E::Bad)`: repair route not ready (missing evidence: a test assertion that checks the changed return value)"
        ),
        "a sibling-variant pin must not grip the changed return value:\n{repair}"
    );
    assert!(
        !repair.contains("repair_ready=true"),
        "no start is added:\n{repair}"
    );
    Ok(())
}

// Not-ready burn-down (#11111): the closing step of a bare-repair refusal
// follows what `ripr check` says about the change.

/// An exposed change: repair says there is nothing to do, not that `ripr
/// check` names a gap to test by hand.
#[test]
fn exposed_change_refusal_says_nothing_to_do() -> Result<(), String> {
    let repo = Repo::create_uncommitted(
        "pub fn dbl(x: i32) -> i32 {\n    x * 2\n}\n",
        "pub fn dbl(x: i32) -> i32 {\n    x * 3\n}\n",
        "use ar::dbl;\n\n#[test]\nfn exact() {\n    assert_eq!(dbl(2), 6);\n}\n",
    )?;
    let check = repo.ripr(&["check"])?;
    assert!(
        check.contains("Static exposure: exposed"),
        "precondition, check reads exposed:\n{check}"
    );
    let repair = refused_repair(&repo)?;
    assert!(
        repair.contains("Nothing to do for this change: the existing tests already check it")
            && !repair.contains("names a gap in it to test by hand"),
        "an exposed change must close with nothing to do:\n{repair}"
    );
    Ok(())
}

/// A change on lines with no repairable code (a loop range) leads with that
/// and closes with a hand-written test, not a generic repository route.
#[test]
fn change_with_no_repairable_code_says_so_first() -> Result<(), String> {
    let repo = Repo::create_uncommitted(LOOP_OLD, LOOP_NEW, LOOP_TEST)?;
    let repair = refused_repair(&repo)?;
    assert!(
        repair
            .contains("your changed lines (src/lib.rs:3) touch no code ripr can start a repair on")
            && repair.contains("Next: write a test for this change by hand")
            && !repair.contains("names a gap in it to test by hand"),
        "the refusal must lead with the change and close with a hand-written test:\n{repair}"
    );
    Ok(())
}

/// A route-not-ready async owner: repair names why no fill-in skeleton is
/// offered (as `ripr check` does) instead of a stub command that refuses.
#[test]
fn async_route_refusal_does_not_offer_a_stub_that_refuses() -> Result<(), String> {
    let repo = Repo::create_uncommitted(
        "pub async fn fetch_len(s: &str) -> usize {\n    s.len()\n}\n",
        "pub async fn fetch_len(s: &str) -> usize {\n    s.len() + 1\n}\n",
        "use ar::fetch_len;\n\n#[test]\nfn builds_future() {\n    let _f = fetch_len(\"abc\");\n}\n",
    )?;
    let repair = refused_repair(&repo)?;
    assert!(
        repair.contains("ripr cannot write a fill-in skeleton here because the owner is async")
            && !repair.contains("--kind return_value"),
        "repair must not offer a stub the stub command refuses:\n{repair}"
    );
    let stub = repo.ripr(&[
        "agent",
        "stub",
        "--root",
        ".",
        "--at",
        "src/lib.rs:2",
        "--kind",
        "return_value",
    ])?;
    assert!(
        stub.contains("async"),
        "precondition, the stub command itself refuses the async owner:\n{stub}"
    );
    Ok(())
}

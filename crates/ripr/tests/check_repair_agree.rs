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
        self.ripr_status_env(args, &[])
    }

    fn ripr_status_env(
        &self,
        args: &[&str],
        env: &[(&str, &str)],
    ) -> Result<(Option<i32>, String), String> {
        let output: Output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .current_dir(&self.0)
            .args(args)
            .envs(env.iter().copied())
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
    let repo = Repo::create_uncommitted(
        &old,
        &new,
        // Only `doubled` is gripped; the changed `label` arm has no matching
        // test input, so check counts untied spots in the file while repair
        // counts the gripped sibling outside the changed lines.
        "use ar::doubled;\n\n#[test]\nfn doubled_works() {\n    assert_eq!(doubled(2), 4);\n}\n",
    )?;
    let repair = repo.ripr(&["repair", "--root", "."])?;
    assert!(
        repair.contains("outside your changed lines is already noticed by existing tests"),
        "gripped sibling precondition:\n{repair}"
    );
    let check = repo.ripr(&["check", "--base", "HEAD"])?;
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
    // The whole-seam reading: an empty range (#16082) keeps repair unscoped.
    let repair = repo.ripr(&["repair", "--root", ".", "--base", "HEAD"])?;
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

/// #11111: with no diff to scope to (an empty `--base HEAD` range; a bare
/// repair here follows check's last-commit rescope, #16082), repair's
/// whole-seam "already check it (exposed)" names the seam it is about, so it is not read as a verdict
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
    let (code, repair) = repo.ripr_status(&["repair", "--root", ".", "--base", "HEAD"])?;
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

/// A reader that pads the constant (`{GREETING:>8}`) can hide the change for
/// some inputs, so the changed constant keeps its own finding with no seam
/// (#11400 follows only a plain `{GREETING}` in a whole-body `format!`).
const CONST_PADDED_OLD: &str = "pub const GREETING: &str = \"hello\";\n\npub fn greet(name: &str) -> String {\n    format!(\"{GREETING:>8}, {name}\")\n}\n\npub fn shout(name: &str) -> String {\n    format!(\"{name}!\")\n}\n";
const CONST_PADDED_NEW: &str = "pub const GREETING: &str = \"howdy\";\n\npub fn greet(name: &str) -> String {\n    format!(\"{GREETING:>8}, {name}\")\n}\n\npub fn shout(name: &str) -> String {\n    format!(\"{name}!\")\n}\n";

/// #11111 (group 2): a changed string constant has no seam of its own, so
/// `ripr check` offers no repair start for it ("no gap ID"). Bare repair must
/// not start, or offer to choose, a repair on the unchanged functions that
/// merely sit in the same file: that is a different change than the one
/// check reported.
#[test]
fn changed_constant_with_no_seam_does_not_start_repair_elsewhere() -> Result<(), String> {
    let repo = Repo::create_uncommitted(CONST_PADDED_OLD, CONST_PADDED_NEW, CONST_TEST)?;
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

/// #11400: when the only reader returns the constant whole in one
/// `format!`, check names that returned value and offers a start, and bare
/// repair selects the same seam, not `shout` in the same file.
#[test]
fn changed_constant_returned_whole_selects_the_same_repair_in_both() -> Result<(), String> {
    let repo = Repo::create_uncommitted(CONST_OLD, CONST_NEW, CONST_TEST)?;
    let check = repo.ripr(&["check", "--base", "HEAD"])?;
    let id = check
        .lines()
        .find_map(|line| {
            let rest = line.trim_start().strip_prefix("ripr repair ")?;
            let id = rest.split_whitespace().next()?;
            (id.len() == 16 && id.bytes().all(|b| b.is_ascii_hexdigit())).then_some(id)
        })
        .ok_or_else(|| format!("check must offer a repair start for greet:\n{check}"))?;
    assert!(
        check.contains("Changed behavior: format!(\"{GREETING}, {name}\")"),
        "check must name greet's returned value:\n{check}"
    );
    // The change is uncommitted, so repair selects the seam and then asks
    // for the commit before starting; the selection is what must agree. The
    // refusal names the façade rerun for that seam (#14394).
    let repair = repo.ripr(&["repair", "--root", "."])?;
    assert!(
        repair.contains(&format!("ripr repair {id} --root "))
            && !repair.contains("shout")
            && !repair.contains("choose the change to repair"),
        "repair must select the seam check offered ({id}):\n{repair}"
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
/// #11401: the unreached fixture. A plain `wscore::discount(price)` call now
/// reads noticed in `ripr check` (RIPR-SPEC-0407), so the dependency is
/// declared under `[dev-dependencies]` only here: library code cannot see it,
/// the call is not a crate edge, and the change keeps no static path.
const WS_DEV_ONLY_APP_TOML: &str = "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dev-dependencies]\nwscore = { path = \"../wscore\" }\n";
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

/// The workspace whose only test is in `app` and reaches `wscore` through a
/// dev-only dependency ripr does not follow: no static path
/// (#11401).
fn unreached_workspace_repo(commit_change: bool) -> Result<Repo, String> {
    Repo::create_workspace(
        &[
            ("Cargo.toml", WS_ROOT),
            ("app/Cargo.toml", WS_DEV_ONLY_APP_TOML),
            ("app/src/lib.rs", WS_APP_SRC),
            ("wscore/Cargo.toml", WS_CORE_TOML),
            ("wscore/src/lib.rs", WS_OLD),
            ("app/tests/t.rs", WS_APP_TEST),
        ],
        WS_NEW,
        commit_change,
    )
}

/// The whole-repository repair text for a committed fixture, with the
/// refusal asserted (exit 3). `--base HEAD` names an empty range: a bare
/// repair on this clean default branch would follow check's last-commit
/// rescope (#16082) and scope to the change.
fn refused_repair(repo: &Repo) -> Result<String, String> {
    let (code, repair) = repo.ripr_status(&["repair", "--root", ".", "--base", "HEAD"])?;
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
    // #15278: a gripped spot on the changed lines that check itself reads as a
    // gap is reopened with check's reading, so it is neither a different check
    // nor outside the change.
    if gripped_note.starts_with("`ripr check` reads this changed code") {
        assert!(
            !repair.contains("different check than the one still open"),
            "a spot check reads as open is not a different check:\n{repair}"
        );
        assert!(
            !(repair.contains("outside your changed lines")
                && repair.contains("already noticed by existing tests and not listed")),
            "a spot check reads as open on the change is not counted outside it:\n{repair}"
        );
        assert_plain(&repair);
        return Ok(());
    }
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
        // #14266: the weak `.or(Some(0))` return gap leads Start here over
        // the unknown predicate on the same line; both are non-exposed.
        "Changed behavior: xs.iter().copied().find(|x| x % 2 == 0).or(Some(0))",
        "`ripr check` reads this changed code as weak",
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
    let committed = unreached_workspace_repo(true)?;
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

    let repo = unreached_workspace_repo(false)?;
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

/// The same shape with an empty range (`--base HEAD`; a bare repair on this
/// clean default branch follows check's last-commit rescope, #16082): no
/// diff is available, so repair keeps the whole-repository view, and each reason names the seam it is
/// about instead of sitting beside the other as a second verdict.
#[test]
fn whole_repository_refusals_name_the_seam_each_reason_is_about() -> Result<(), String> {
    let repo = Repo::create(GUARD_OLD, GUARD_NEW, GUARD_TEST)?;
    let (code, repair) = repo.ripr_status(&["repair", "--root", ".", "--base", "HEAD"])?;
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

/// #15278: the regex `is_word_byte` shape. Tests elsewhere in the repository
/// pin the whole match seam, so the seam inventory reads it as already gripped,
/// while `ripr check` reads the changed arm as weak (no test input selects the
/// changed digit range). Bare repair must not say "the existing tests already
/// check it, and `ripr check` reports the same" while check reports a gap.
const WORD_OLD: &str = "pub fn is_word_byte(c: u8) -> bool {\n    match c {\n        b'_' | b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' => true,\n        _ => false,\n    }\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn word_byte() {\n        assert!(is_word_byte(b'a'));\n        assert!(!is_word_byte(b'-'));\n    }\n}\n";
const WORD_TEST: &str = "use ar::is_word_byte;\n\n#[test]\nfn pins() {\n    assert_eq!(is_word_byte(b'a'), true);\n    assert_eq!(is_word_byte(b'-'), false);\n}\n";

#[test]
fn repair_never_says_check_reports_the_same_while_check_reports_a_weak_gap() -> Result<(), String> {
    let new = WORD_OLD.replace("b'0'..=b'9'", "b'1'..=b'9'");
    let repo = Repo::create(WORD_OLD, &new, WORD_TEST)?;
    let check = repo.ripr(&["check", "--base", "HEAD~1"])?;
    assert!(
        check.contains("Static exposure: weak"),
        "precondition, check reports a weak gap on the changed arm:\n{check}"
    );
    let (code, repair) = repo.ripr_status(&["repair", "--root", ".", "--base", "HEAD~1"])?;
    assert_eq!(code, Some(3), "repair must not start:\n{repair}");
    assert!(
        !repair.contains("Nothing to do")
            && !repair.contains("reports the same")
            && !repair.contains("already check it"),
        "repair must not call a change check reads as weak already checked:\n{repair}"
    );
    assert!(
        repair.contains("`ripr check` reads this changed code as weak")
            && repair.contains("write or strengthen the test by hand"),
        "repair must name check's reading and the by-hand next step:\n{repair}"
    );
    Ok(())
}

// #15278 item 1: the same disagreement on the explicit surfaces. `a + b` to
// `a - b` with only `total(0, 0) == 0` is weak for check (the inputs cannot
// tell the two apart), while the inventory reads the returned value as pinned.
const SUM_OLD: &str = "pub fn total(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
const SUM_NEW: &str = "pub fn total(a: i32, b: i32) -> i32 {\n    a - b\n}\n";
const SUM_TEST: &str =
    "use ar::total;\n\n#[test]\nfn zero() {\n    assert_eq!(total(0, 0), 0);\n}\n";

/// The id of the one finding `ripr check --format json` reports.
fn only_finding_id(repo: &Repo) -> Result<String, String> {
    let json = repo.ripr(&["check", "--format", "json"])?;
    let (_, tail) = json
        .split_once("\"id\": \"probe:")
        .ok_or_else(|| format!("check JSON names no probe finding:\n{json}"))?;
    let (id, _) = tail
        .split_once('"')
        .ok_or_else(|| format!("unterminated finding id:\n{json}"))?;
    Ok(format!("probe:{id}"))
}

fn weak_sum_repo() -> Result<Repo, String> {
    let repo = Repo::create_uncommitted(SUM_OLD, SUM_NEW, SUM_TEST)?;
    let check = repo.ripr(&["check"])?;
    assert!(
        check.contains("Static exposure: weak"),
        "precondition, check reports a weak gap on the changed return:\n{check}"
    );
    Ok(repo)
}

#[test]
fn repair_by_finding_id_never_says_already_checked_while_check_reports_a_weak_gap()
-> Result<(), String> {
    let repo = weak_sum_repo()?;
    let id = only_finding_id(&repo)?;
    let (code, repair) = repo.ripr_status(&["repair", &id, "--root", "."])?;
    assert_eq!(code, Some(3), "repair must not start:\n{repair}");
    assert!(
        !repair.contains("already check it") && !repair.contains("no gap to repair"),
        "repair must not call check's own weak finding already checked:\n{repair}"
    );
    assert!(
        repair.contains("`ripr check` reads this changed code as weak")
            && repair.contains("write or strengthen the test by hand"),
        "repair must name check's reading and the by-hand next step:\n{repair}"
    );
    Ok(())
}

#[test]
fn stub_never_says_tests_already_discriminate_while_check_reports_a_weak_gap() -> Result<(), String>
{
    let repo = weak_sum_repo()?;
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
        !stub.contains("existing tests already discriminate it")
            && !stub.contains("not evidence of a missing discriminator"),
        "the stub must not call a change check reads as weak already discriminated:\n{stub}"
    );
    assert!(
        stub.contains("`ripr check` reads your changed code there as weak (weakly_exposed)")
            && stub.contains("this stub is for that gap"),
        "the stub must name check's reading:\n{stub}"
    );
    let json = repo.ripr(&[
        "agent",
        "stub",
        "--root",
        ".",
        "--at",
        "src/lib.rs:2",
        "--kind",
        "return_value",
        "--json",
    ])?;
    assert!(
        json.contains("\"grip_class\": \"strongly_gripped\"")
            && json.contains("\"classification\": \"weakly_exposed\""),
        "the JSON keeps the inventory class and carries check's class:\n{json}"
    );
    Ok(())
}

#[test]
fn pilot_never_says_no_test_to_add_while_check_reports_a_weak_gap() -> Result<(), String> {
    let repo = weak_sum_repo()?;
    let pilot = repo.ripr(&["pilot", "--root", "."])?;
    assert!(
        !pilot.contains("so there is no test to add"),
        "pilot must not say there is no test to add while check reports a gap:\n{pilot}"
    );
    assert!(
        pilot.contains("but `ripr check` reads your changed code there as a gap or cannot tell")
            && pilot.contains("Pilot ranked no gap, but `ripr check` reads your changed code")
            && pilot.contains("run `ripr check --root "),
        "pilot must point at check's reading of the change:\n{pilot}"
    );
    Ok(())
}

/// #16082: on a clean checkout of the comparison base, bare human `ripr
/// check` analyzes the last commit. Repair, stub and pilot scope to that same
/// range, so the committed weak change keeps the agreement the uncommitted
/// fixture above proves. Before #16082 all three read the empty default range
/// and kept the inventory's "already checked" reading.
#[test]
fn clean_default_branch_repair_stub_and_pilot_follow_checks_last_commit() -> Result<(), String> {
    let repo = Repo::create(SUM_OLD, SUM_NEW, SUM_TEST)?;
    let check = repo.ripr(&["check"])?;
    assert!(
        check.contains("analyzed the last commit (--base HEAD~1)")
            && check.contains("Static exposure: weak"),
        "precondition, bare check falls back to the last commit and reports the weak gap:\n{check}"
    );
    let structured = repo.ripr(&["check", "--format", "json"])?;
    assert!(
        !structured.contains("\"id\": \"probe:"),
        "structured formats keep their empty-range contract:\n{structured}"
    );
    let json = repo.ripr(&["check", "--format", "json", "--base", "HEAD~1"])?;
    let (_, tail) = json
        .split_once("\"id\": \"probe:")
        .ok_or_else(|| format!("check JSON names no probe finding:\n{json}"))?;
    let (id, _) = tail
        .split_once('"')
        .ok_or_else(|| format!("unterminated finding id:\n{json}"))?;
    let id = format!("probe:{id}");

    let (code, repair) = repo.ripr_status(&["repair", &id, "--root", "."])?;
    assert_eq!(code, Some(3), "repair must not start:\n{repair}");
    assert!(
        !repair.contains("already check it") && !repair.contains("no gap to repair"),
        "repair must not call check's weak finding already checked:\n{repair}"
    );
    assert!(
        repair.contains("`ripr check` reads this changed code as weak"),
        "repair must name check's reading of the last commit:\n{repair}"
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
        !stub.contains("existing tests already discriminate it")
            && stub.contains("`ripr check` reads your changed code there as weak (weakly_exposed)"),
        "the stub must name check's reading of the last commit:\n{stub}"
    );

    let pilot = repo.ripr(&["pilot", "--root", "."])?;
    assert!(
        !pilot.contains("so there is no test to add")
            && pilot.contains("Pilot ranked no gap, but `ripr check` reads your changed code"),
        "pilot must point at check's reading of the last commit:\n{pilot}"
    );
    Ok(())
}

/// #16082 control: an explicit `--base` names the range, so a clean
/// checkout compared with itself is not rescoped. Repair keeps the empty
/// range and says nothing differs from that base.
#[test]
fn explicit_base_on_clean_default_branch_is_not_rescoped() -> Result<(), String> {
    let repo = Repo::create(SUM_OLD, SUM_NEW, SUM_TEST)?;
    let (_, repair) = repo.ripr_status(&["repair", "--root", ".", "--base", "HEAD"])?;
    assert!(
        repair.contains("because nothing differs from `HEAD`"),
        "an explicit base must keep its own empty range:\n{repair}"
    );
    Ok(())
}

/// #16082 controls: check stays the authority on the rescope. A last commit
/// that check's own intake refuses (over the diff line limit) or does not
/// count (only ripr's own reports) leaves check on its empty default range,
/// and repair and pilot stay there too instead of scoping to the raw patch.
#[test]
fn last_commit_check_does_not_accept_keeps_repair_and_pilot_unscoped() -> Result<(), String> {
    let over = Repo::create(SUM_OLD, SUM_NEW, SUM_TEST)?;
    over.write("README.md", &"line\n".repeat(20))?;
    over.git(&["add", "README.md"])?;
    over.git(&["commit", "-q", "--no-gpg-sign", "--amend", "--no-edit"])?;
    let limit = [("RIPR_MAX_DIFF_CHANGED_LINES", "5")];
    let (_, check) = over.ripr_status_env(&["check"], &limit)?;
    assert!(
        !check.contains("analyzed the last commit"),
        "precondition, check's intake refuses the oversized last commit:\n{check}"
    );
    let (_, repair) = over.ripr_status_env(&["repair", "--root", "."], &limit)?;
    assert!(
        repair.contains("could not tell which lines you changed"),
        "repair must stay on check's empty range:\n{repair}"
    );

    let reports = Repo::create_uncommitted(SUM_NEW, SUM_NEW, SUM_TEST)?;
    std::fs::create_dir_all(reports.0.join("target/ripr/pilot")).map_err(|e| e.to_string())?;
    reports.write("target/ripr/pilot/pilot-summary.md", "# report\n")?;
    reports.git(&["add", "-f", "target/ripr/pilot/pilot-summary.md"])?;
    reports.git(&["commit", "-q", "--no-gpg-sign", "-m", "reports"])?;
    let check = reports.ripr(&["check"])?;
    assert!(
        !check.contains("analyzed the last commit"),
        "precondition, check does not count ripr's own reports:\n{check}"
    );
    let repair = reports.ripr(&["repair", "--root", "."])?;
    assert!(
        repair.contains("could not tell which lines you changed"),
        "repair must stay on check's empty range:\n{repair}"
    );
    let pilot = reports.ripr(&["pilot", "--root", "."])?;
    assert!(
        !pilot.contains("HEAD~1"),
        "pilot must not report the report-only commit as the change:\n{pilot}"
    );
    Ok(())
}

/// Discriminating negative: a change check reads as exposed keeps every
/// surface's "already checked" wording.
#[test]
fn an_exposed_change_keeps_the_already_checked_wording_on_every_surface() -> Result<(), String> {
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
    let id = only_finding_id(&repo)?;
    let (code, repair) = repo.ripr_status(&["repair", &id, "--root", "."])?;
    assert_eq!(code, Some(3), "repair must not start:\n{repair}");
    assert!(
        repair.contains("existing tests already check it")
            && !repair.contains("`ripr check` reads this changed code as"),
        "an exposed change stays already checked:\n{repair}"
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
        stub.contains("existing tests already discriminate it")
            && !stub.contains("`ripr check` reads your changed code"),
        "an exposed change keeps the strongly gripped disclosure:\n{stub}"
    );
    let pilot = repo.ripr(&["pilot", "--root", "."])?;
    assert!(
        pilot.contains("No gap was found, so there is no test to add")
            && !pilot.contains("but `ripr check` reads"),
        "an exposed change keeps pilot's no-gap closing line:\n{pilot}"
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

/// #13636 (A): with the production change still uncommitted, `ripr repair`
/// refuses to start, so `ripr check` names the commit step before the repair
/// start it prints. Once the change is committed, the commit step disappears.
#[test]
fn uncommitted_change_check_names_the_commit_step_before_the_repair_start() -> Result<(), String> {
    let dirty = Repo::create_uncommitted(CONST_OLD, CONST_NEW, CONST_TEST)?;
    let check = dirty.ripr(&["check", "--base", "HEAD"])?;
    let commit_at = check
        .find("Commit your production change first")
        .ok_or_else(|| format!("check must name the commit step:\n{check}"))?;
    let start_at = check
        .find("\n  ripr repair ")
        .ok_or_else(|| format!("check must still name the repair start:\n{check}"))?;
    assert!(
        commit_at < start_at,
        "the commit step must come before the repair start:\n{check}"
    );
    assert!(
        check.contains("--literal-pathspecs commit -m \"<message>\" -- src/lib.rs"),
        "the commit step must name the uncommitted production file:\n{check}"
    );
    // The commit step is the reason the start would refuse today.
    let (code, repair) = dirty.ripr_status(&["repair", "--root", "."])?;
    assert_ne!(code, Some(0), "repair must refuse while dirty:\n{repair}");
    assert!(
        repair.contains("ripr repair needs your source changes committed first")
            && repair.contains("Uncommitted: src/lib.rs"),
        "repair must refuse for the same file check names:\n{repair}"
    );

    let json = dirty.ripr(&["check", "--base", "HEAD", "--format", "json"])?;
    let repair = json_repair_route(&json)?;
    assert_eq!(
        repair["status"], "blocked_uncommitted",
        "the JSON repair route must say the start is blocked:\n{json}"
    );
    assert_eq!(
        repair["uncommitted_files"],
        serde_json::json!(["src/lib.rs"]),
        "the JSON repair route must name the uncommitted file:\n{json}"
    );
    assert!(
        repair.get("command").is_none(),
        "a blocked start must carry no runnable command:\n{json}"
    );

    let committed = Repo::create(CONST_OLD, CONST_NEW, CONST_TEST)?;
    let check = committed.ripr(&["check", "--base", "HEAD~1"])?;
    assert!(
        check.contains("\n  ripr repair ") && !check.contains("Commit your production change"),
        "a committed change needs no commit step:\n{check}"
    );
    Ok(())
}

/// The `finding_routes[0].repair` object of a `ripr check --format json`
/// run. `text` is stdout followed by stderr; only the first JSON value is read.
fn json_repair_route(text: &str) -> Result<serde_json::Value, String> {
    let start = text
        .find('{')
        .ok_or_else(|| format!("no JSON document:\n{text}"))?;
    let document: serde_json::Value = serde_json::Deserializer::from_str(&text[start..])
        .into_iter()
        .next()
        .ok_or_else(|| format!("no JSON document:\n{text}"))?
        .map_err(|error| format!("unparseable JSON ({error}):\n{text}"))?;
    let routes = document["finding_routes"]
        .as_array()
        .ok_or_else(|| format!("finding_routes must be an array:\n{text}"))?;
    let [route] = routes.as_slice() else {
        return Err(format!("expected one finding route:\n{text}"));
    };
    route
        .get("repair")
        .cloned()
        .ok_or_else(|| format!("the finding route must carry a repair entry:\n{text}"))
}

/// A repository whose analyzed root is the `crate` subdirectory, with a
/// repository-level `src/lib.rs` outside it. The `CONST_OLD` -> `CONST_NEW`
/// change is committed when `commit_change` is set and left dirty otherwise;
/// the outside file is dirty in both cases.
fn outer_repo_with_subdir_root(commit_change: bool) -> Result<Repo, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let outer = std::env::temp_dir().join(format!(
        "ripr-check-repair-agree-outer-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let repo = Repo(outer);
    for dir in ["src", "crate/src", "crate/tests"] {
        std::fs::create_dir_all(repo.0.join(dir)).map_err(|e| e.to_string())?;
    }
    repo.write(".gitignore", "target/\n")?;
    repo.write("src/lib.rs", "pub fn outer() -> u8 {\n    1\n}\n")?;
    repo.write("crate/Cargo.toml", CARGO_TOML)?;
    repo.write("crate/src/lib.rs", CONST_OLD)?;
    repo.write("crate/tests/t.rs", CONST_TEST)?;
    repo.git(&["-c", "init.templateDir=", "init", "-q"])?;
    repo.git(&["config", "user.email", "t@example.com"])?;
    repo.git(&["config", "user.name", "t"])?;
    repo.git(&["config", "commit.gpgSign", "false"])?;
    repo.git(&["add", "-A"])?;
    repo.git(&["commit", "-q", "--no-gpg-sign", "-m", "base"])?;
    repo.write("crate/src/lib.rs", CONST_NEW)?;
    if commit_change {
        repo.git(&["commit", "-q", "--no-gpg-sign", "-am", "change"])?;
    }
    repo.write("src/lib.rs", "pub fn outer() -> u8 {\n    2\n}\n")?;
    Ok(repo)
}

/// #15488: `ripr check --root <subdir>` withholds the seam start altogether
/// and names the Git top-level, because a repair attempt starts only there
/// (#15363) — the start (and its commit step) printed here before would
/// refuse at the before phase even after the commit. The dirty files inside
/// and outside the analyzed root no longer need a `../...` commit step on
/// this screen; the top-level check names them in its own route.
#[test]
fn subdir_check_withholds_the_seam_start_and_names_the_top_level() -> Result<(), String> {
    let repo = outer_repo_with_subdir_root(false)?;
    let check = repo.ripr(&["check", "--root", "crate", "--base", "HEAD"])?;
    assert!(
        !check.contains("\n  ripr repair ")
            && !check.contains("Commit your production change first")
            && check.contains("is not the exact Git top-level"),
        "a subdirectory check must offer no seam start and name the top-level:\n{check}"
    );
    // The start itself still refuses first, before writing anything.
    let (code, repair) = repo.ripr_status(&["repair", "--root", "crate"])?;
    assert!(
        code != Some(0) && repair.contains("is not the exact Git top-level"),
        "the start refuses the subdirectory root:\n{repair}"
    );
    assert!(
        !repo.0.join("crate/target/ripr/repair-attempts").exists(),
        "the refusal must take no attempt lock under the subdirectory root:\n{repair}"
    );
    let json = repo.ripr(&[
        "check", "--root", "crate", "--base", "HEAD", "--format", "json",
    ])?;
    // A withheld route offers no repair entry at all: no command, no step.
    // The error must be the specific missing-repair one, so malformed JSON
    // or a missing or multiple-route document fails here instead of passing.
    let error = match json_repair_route(&json) {
        Ok(_) => {
            return Err(format!(
                "a subdirectory route must carry no repair entry:\n{json}"
            ));
        }
        Err(error) => error,
    };
    assert!(
        error.starts_with("the finding route must carry a repair entry:"),
        "expected the missing-repair error, got: {error}"
    );
    Ok(())
}

/// #13636 (A): a committed-only run did not analyze the dirty file the start
/// refuses over, so the seam route prints no commit step vouching for it and
/// no runnable start; it is refused, as the gap route refuses the same state.
/// #15488 ordering: under a subdirectory root the finding's own refusal is
/// named first. The top-level gate applies only where a start would otherwise
/// be printed, so it does not rewrite this reason.
#[test]
fn committed_only_check_at_a_subdirectory_root_refuses_the_seam_route() -> Result<(), String> {
    let repo = outer_repo_with_subdir_root(true)?;
    let check = repo.ripr(&["check", "--root", "crate", "--base", "HEAD~1"])?;
    assert!(
        !check.contains("Commit your production change")
            && !check.contains("\n  ripr repair ")
            && check.contains("No strong assertion was detected"),
        "a committed-only run at a subdirectory root must refuse the seam route:\n{check}"
    );
    let json = repo.ripr(&[
        "check", "--root", "crate", "--base", "HEAD~1", "--format", "json",
    ])?;
    // A refused route offers no repair entry at all: no command, no commit step.
    let Err(error) = json_repair_route(&json) else {
        return Err(format!(
            "a refused seam route must carry no repair entry:\n{json}"
        ));
    };
    assert!(
        error.starts_with("the finding route must carry a repair entry"),
        "the JSON must still carry its one finding route: {error}"
    );
    Ok(())
}

const SHIPPING_OLD: &str = "pub fn shipping(weight: u32) -> u32 {\n    if weight > 20 {\n        15\n    } else {\n        5\n    }\n}\n";
const SHIPPING_NEW: &str = "pub fn shipping(weight: u32) -> u32 {\n    if weight > 25 {\n        15\n    } else {\n        5\n    }\n}\n";

/// The 16-hex spot id of the `ripr repair <id>` start a check screen offers.
fn offered_repair_id(check: &str) -> Option<String> {
    check.lines().find_map(|line| {
        let rest = line.trim_start().strip_prefix("ripr repair ")?;
        let id = rest.split_whitespace().next()?;
        (id.len() == 16 && id.bytes().all(|b| b.is_ascii_hexdigit())).then(|| id.to_string())
    })
}

/// #14084: a changed boundary whose only test calls the owner and asserts
/// nothing. The before phase withholds the seam (its propagation stage is
/// unknown), so check must not print a start for it and bare repair must not
/// list it as startable; both give the one plain reason, as a typed refusal.
#[test]
fn no_assertion_gap_is_not_offered_a_start_the_before_phase_refuses() -> Result<(), String> {
    let repo = Repo::create(
        SHIPPING_OLD,
        SHIPPING_NEW,
        "use ar::shipping;\n\n#[test]\nfn shipping_runs() {\n    let _ = shipping(30);\n}\n",
    )?;
    let reason = "it cannot tell from reading the code whether the changed value reaches a result a test can check";
    let check = repo.ripr(&["check", "--base", "HEAD~1"])?;
    assert!(
        check.contains("Changed behavior: weight > 25"),
        "the fixture must produce the boundary finding:\n{check}"
    );
    assert!(
        offered_repair_id(&check).is_none() && check.contains(reason),
        "check must name the reason instead of offering a start:\n{check}"
    );
    let repair = refused_repair(&repo)?;
    assert!(
        repair.contains(reason) && repair.contains("refusal code=no_eligible_seam"),
        "bare repair must refuse with the same reason:\n{repair}"
    );
    // The item check names for the full gate's answer: a typed refusal, not
    // the before phase's exit 2.
    let id = check
        .split("run `ripr repair ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .ok_or_else(|| format!("check must name the item to ask the gate about:\n{check}"))?
        .to_string();
    let (code, named) = repo.ripr_status(&["repair", &id, "--root", "."])?;
    assert_eq!(
        code,
        Some(3),
        "a named item must refuse as a decision:\n{named}"
    );
    assert!(
        named.contains(reason)
            && named.contains("refusal code=seam_not_repair_eligible")
            && !named.contains("no workflow starts")
            && !named.contains("static limitation"),
        "the named refusal must be the shared gate's plain one:\n{named}"
    );
    Ok(())
}

/// Control for #14084: the same change under a test that reads the result
/// with a loose assertion is offered a start by check, and that exact
/// command starts.
#[test]
fn weak_assertion_gap_start_offered_by_check_starts() -> Result<(), String> {
    let repo = Repo::create(
        SHIPPING_OLD,
        SHIPPING_NEW,
        "use ar::shipping;\n\n#[test]\nfn shipping_is_positive() {\n    assert!(shipping(30) > 0);\n}\n",
    )?;
    let check = repo.ripr(&["check", "--base", "HEAD~1"])?;
    let id = offered_repair_id(&check)
        .ok_or_else(|| format!("check must offer a repair start:\n{check}"))?;
    let (code, started) = repo.ripr_status(&["repair", &id, "--root", "."])?;
    assert_eq!(code, Some(0), "the offered start must start:\n{started}");
    assert!(
        started.contains("Repair prepared for seam"),
        "the start must print its before-phase summary:\n{started}"
    );
    Ok(())
}

/// #14084: when check already reads the finding as unclear, its limitation
/// guidance is the whole story; the withheld seam adds no second advisory.
/// The comparison sits in parentheses, an operand shape the boundary reader
/// does not bind, so the finding stays unclear. (A plain
/// `member && total >= 120` is read by the guarded boundary rule, #13037,
/// which credits only rows that hold `member = true`.)
#[test]
fn unclear_finding_on_a_withheld_seam_adds_no_repair_advisory() -> Result<(), String> {
    let repo = Repo::create(
        "pub fn discount(total: u32, member: bool) -> u32 {\n    if member && (total >= 100) {\n        total - 10\n    } else {\n        total\n    }\n}\n",
        "pub fn discount(total: u32, member: bool) -> u32 {\n    if member && (total >= 120) {\n        total - 10\n    } else {\n        total\n    }\n}\n",
        "use ar::discount;\n\n#[test]\nfn runs() {\n    let _ = discount(200, true);\n}\n",
    )?;
    let check = repo.ripr(&["check", "--base", "HEAD~1"])?;
    assert!(
        check.contains("Static exposure: unknown"),
        "the fixture must produce an unclear finding:\n{check}"
    );
    assert!(
        offered_repair_id(&check).is_none() && !check.contains("no repair start offered"),
        "an unclear finding must stay without a repair advisory:\n{check}"
    );
    let repair = refused_repair(&repo)?;
    assert!(
        repair.contains("whether the changed value reaches a result a test can check"),
        "bare repair still names why the spot cannot start:\n{repair}"
    );
    Ok(())
}

/// #14719: a compound predicate's gap id names its one comparison
/// (`s.len==1`), which the inner `s.len() > 1` seam on the same line also
/// mints. Repair by that id picks the most-specific seam instead of refusing
/// the id as naming several seams.
#[test]
fn compound_predicate_gap_id_names_one_seam() -> Result<(), String> {
    let repo = Repo::create(
        "pub fn f(s: &str) -> u32 {\n    if s.starts_with('-') {\n        1\n    } else {\n        0\n    }\n}\n",
        "pub fn f(s: &str) -> u32 {\n    if s.starts_with('-') && s.len() > 1 {\n        1\n    } else {\n        0\n    }\n}\n",
        "use ar::f;\n\n#[test]\nfn t() {\n    let _ = f(\"-v\");\n}\n",
    )?;
    let check = repo.ripr(&["check", "--base", "HEAD~1"])?;
    let id = "gap:rust:src/lib.rs:f:predicate_boundary:predicate:s.len==1";
    assert!(
        check.contains(id),
        "the predicate's gap id names the narrowed comparison:\n{check}"
    );
    let (_, repair) = repo.ripr_status(&["repair", id, "--root", "."])?;
    assert!(
        !repair.contains("item_names_several_seams") && repair.contains("names gap `"),
        "the gap id must resolve to one seam:\n{repair}"
    );
    let inner = seam_id_for_expression(&repo, "s.len() > 1")?;
    let outer = seam_id_for_expression(&repo, "s.starts_with('-') && s.len() > 1")?;
    assert_ne!(inner, outer, "the fixture must mint two distinct seams");
    assert!(
        repair.contains(&format!("names gap `{inner}`")) && !repair.contains(&format!("`{outer}`")),
        "repair must pick the inner comparison seam {inner}, not the enclosing {outer}:\n{repair}"
    );
    Ok(())
}

/// The seam id `ripr pilot` lists for the one seam whose changed expression is
/// exactly `expression`.
fn seam_id_for_expression(repo: &Repo, expression: &str) -> Result<String, String> {
    repo.ripr(&["pilot", "--root", "."])?;
    let text = std::fs::read_to_string(repo.0.join("target/ripr/pilot/agent-seam-packets.json"))
        .map_err(|e| e.to_string())?;
    let packets: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let ids: Vec<&str> = packets["packets"]
        .as_array()
        .ok_or("pilot packets must be an array")?
        .iter()
        .filter(|packet| packet["changed_expression"] == expression)
        .filter_map(|packet| packet["seam_id"].as_str())
        .collect();
    match ids.as_slice() {
        [id] => Ok((*id).to_string()),
        other => Err(format!(
            "expected one seam for `{expression}`, found {other:?}"
        )),
    }
}

/// #15278 item 4: a field change in a method no test calls. Pilot names the
/// method a test must call instead of describing oracles that never run it.
#[test]
fn pilot_names_the_method_a_test_must_call() -> Result<(), String> {
    const HEAD: &str = "pub struct Counter {\n    pub count: u32,\n}\n\nimpl Counter {\n    pub fn new() -> Self {\n        Counter { count: 0 }\n    }\n\n    pub fn bump(&mut self) {\n        self.count += STEP;\n    }\n}\n";
    let old = HEAD.replace("STEP", "1");
    let new = HEAD.replace("STEP", "2");
    let repo = Repo::create_uncommitted(
        &old,
        &new,
        "use ar::Counter;\n\n#[test]\nfn starts_at_zero() {\n    assert_eq!(Counter::new().count, 0);\n}\n",
    )?;
    let check = repo.ripr(&["check"])?;
    assert!(
        check.contains("Static exposure: no path"),
        "precondition, check finds no test path to the changed field:\n{check}"
    );
    let pilot = repo.ripr(&["pilot", "--root", "."])?;
    assert!(
        pilot.contains("why it matters: no test is seen calling `bump`")
            && pilot.contains("test to add: one that calls `bump`"),
        "pilot must say a test has to call bump:\n{pilot}"
    );
    assert!(
        !pilot.contains("Strongest oracle") && !pilot.contains("has no test file to use"),
        "pilot must not describe oracles or a missing test file here:\n{pilot}"
    );
    Ok(())
}

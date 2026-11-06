//! Public-API controls for dead control flow inside a `Display::fmt` body
//! (#8999, RIPR-SPEC-0297).
//!
//! A test that formats a value of the impl's type runs `fmt`, but only the
//! code of `fmt` that can run. A changed line in a literal `if false` block
//! or after a top-level `return`, and a changed helper whose only call in
//! `fmt` sits in such a region, are reached by no formatting test. An unknown
//! condition may run and stays reached. Reach never raises a class.

use ripr::domain::RelationReason;
use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

const TEST: &str = "    #[test]
    fn straight() {
        let m = Merge { n: 1 };
        assert_eq!(m.to_string(), \"merge-1\");
    }
";

/// A `Display` impl whose `fmt` body is `fmt_body`, plus the formatting test.
fn fmt_source(fmt_body: &str) -> String {
    format!(
        "use std::fmt;

pub struct Merge {{
    n: u32,
}}

impl fmt::Display for Merge {{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {{
{fmt_body}
    }}
}}

#[cfg(test)]
mod tests {{
    use super::*;

{TEST}}}
"
    )
}

/// A helper `bump` that `fmt` calls from `fmt_body`, plus the formatting test.
fn helper_source(fmt_body: &str) -> String {
    format!(
        "use std::fmt;

pub struct Merge {{
    n: u32,
}}

fn bump(n: u32) -> u32 {{
    n + 2
}}

impl fmt::Display for Merge {{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {{
{fmt_body}
    }}
}}

#[cfg(test)]
mod tests {{
    use super::*;

{TEST}}}
"
    )
}

/// The tests related to the changed line `new_line` (owner name ending in
/// `owner`) by `reason`, after diffing it against `old_line`.
fn related(
    lib: &str,
    old_line: &str,
    new_line: &str,
    owner: &str,
    reason: RelationReason,
) -> Result<Vec<String>, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "ripr-fmt-body-flow-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = related_in(&root, lib, old_line, new_line, owner, reason);
    let _ = std::fs::remove_dir_all(&root);
    result
}

fn related_in(
    root: &Path,
    lib: &str,
    old_line: &str,
    new_line: &str,
    owner: &str,
    reason: RelationReason,
) -> Result<Vec<String>, String> {
    std::fs::create_dir_all(root.join("src")).map_err(|error| format!("mkdir failed: {error}"))?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"merged\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )
    .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
    let line = lib
        .lines()
        .position(|line| line == new_line)
        .map(|index| index + 1)
        .ok_or("premise: the changed line is in the source")?;
    std::fs::write(root.join("src/lib.rs"), lib)
        .map_err(|error| format!("write source failed: {error}"))?;
    std::fs::write(
        root.join("diff.patch"),
        format!(
            "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -{line},1 +{line},1 @@
-{old_line}
+{new_line}
"
        ),
    )
    .map_err(|error| format!("write diff failed: {error}"))?;
    let output = check_workspace(CheckInput {
        root: root.to_path_buf(),
        base: None,
        diff_file: Some(root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let findings: Vec<Finding> = output
        .findings
        .into_iter()
        .filter(|finding| {
            finding.probe.location.line == line
                && finding
                    .probe
                    .owner
                    .as_ref()
                    .is_some_and(|found| found.0.ends_with(owner))
        })
        .collect();
    if findings.is_empty() {
        return Err("premise: the changed line yields a finding".to_string());
    }
    let mut names: Vec<String> = Vec::new();
    for finding in &findings {
        assert_ne!(
            finding.class,
            ExposureClass::Exposed,
            "reach never raises a class: {finding:?}"
        );
        names.extend(
            finding
                .related_tests
                .iter()
                .filter(|test| test.relation_reason == Some(reason))
                .map(|test| test.name.clone()),
        );
    }
    names.sort();
    names.dedup();
    Ok(names)
}

const DIRECT_OLD: &str = "            return write!(f, \"legacy-{}\", self.n + 1);";
const DIRECT_NEW: &str = "            return write!(f, \"legacy-{}\", self.n + 2);";

/// The changed line of `fmt` sits under `guard`; a plain write follows.
fn direct(guard: &str) -> Result<Vec<String>, String> {
    let body = |changed: &str| {
        format!(
            "        {guard} {{
{changed}
        }}
        write!(f, \"merge-{{}}\", self.n)"
        )
    };
    related(
        &fmt_source(&body(DIRECT_NEW)),
        DIRECT_OLD,
        DIRECT_NEW,
        "::fmt",
        RelationReason::DirectOwnerCall,
    )
}

#[test]
fn a_changed_line_in_a_literal_if_false_of_fmt_is_not_reached() -> Result<(), String> {
    let dead = direct("if false")?;
    assert!(
        dead.is_empty(),
        "formatting never runs a literal `if false` block of fmt: {dead:?}"
    );
    // Premise and positive control: the identical fixture under a condition
    // the analysis cannot decide is reached by the same formatting test.
    let live = direct("if self.n > 100")?;
    assert_eq!(
        live,
        vec!["straight".to_string()],
        "an unknown condition may run and stays reached"
    );
    Ok(())
}

#[test]
fn a_changed_line_after_a_top_level_return_of_fmt_is_not_reached() -> Result<(), String> {
    let source = |changed: &str| {
        fmt_source(&format!(
            "        return write!(f, \"merge-{{}}\", self.n);
{changed}"
        ))
    };
    let old = "        write!(f, \"legacy-{}\", self.n + 1)";
    let new = "        write!(f, \"legacy-{}\", self.n + 2)";
    let dead = related(
        &source(new),
        old,
        new,
        "::fmt",
        RelationReason::DirectOwnerCall,
    )?;
    assert!(
        dead.is_empty(),
        "nothing after a top-level return of fmt runs: {dead:?}"
    );
    Ok(())
}

#[test]
fn the_else_of_a_literal_if_false_in_fmt_stays_reached() -> Result<(), String> {
    let new = "            write!(f, \"merge-{}\", self.n + 2)";
    let old = "            write!(f, \"merge-{}\", self.n + 1)";
    let source = fmt_source(&format!(
        "        if false {{
            let _ = 0;
        }} else {{
{new}
        }}"
    ));
    let names = related(&source, old, new, "::fmt", RelationReason::DirectOwnerCall)?;
    assert_eq!(names, vec!["straight".to_string()], "the else branch runs");
    Ok(())
}

const HELPER_OLD: &str = "    n + 1";
const HELPER_NEW: &str = "    n + 2";

fn helper(fmt_body: &str) -> Result<Vec<String>, String> {
    related(
        &helper_source(fmt_body),
        HELPER_OLD,
        HELPER_NEW,
        "::bump",
        RelationReason::HelperOwnerCall,
    )
}

#[test]
fn a_helper_called_only_in_a_literal_if_false_of_fmt_is_not_reached() -> Result<(), String> {
    let dead = helper(
        "        if false {
            return write!(f, \"legacy-{}\", bump(self.n));
        }
        write!(f, \"merge-{}\", self.n)",
    )?;
    assert!(
        dead.is_empty(),
        "the helper is called only from code fmt never runs: {dead:?}"
    );
    Ok(())
}

#[test]
fn a_helper_called_under_an_unknown_condition_in_fmt_stays_reached() -> Result<(), String> {
    // The positive control for the dead-call test above: the identical fixture
    // with a condition the analysis cannot decide keeps the helper-entry
    // route, so the empty result there is the dead-code filter, not a missing
    // chain.
    let unknown = helper(
        "        if self.n > 100 {
            return write!(f, \"legacy-{}\", bump(self.n));
        }
        write!(f, \"merge-{}\", self.n)",
    )?;
    assert_eq!(
        unknown,
        vec!["straight".to_string()],
        "an unknown condition may run and stays reached"
    );
    Ok(())
}

#[test]
fn a_helper_called_after_a_top_level_return_of_fmt_is_not_reached() -> Result<(), String> {
    let dead = helper(
        "        return write!(f, \"merge-{}\", self.n);
        write!(f, \"legacy-{}\", bump(self.n))",
    )?;
    assert!(
        dead.is_empty(),
        "nothing after a top-level return of fmt runs: {dead:?}"
    );
    Ok(())
}

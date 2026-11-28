//! RIPR-SPEC-0157, #14132 (crossbeam-rs/crossbeam `4e59649`): two changed `let` bindings that
//! are operands of the same predicate each retarget to it (#3294). The
//! predicate is one changed behavior and must be reported once, not once per
//! changed binding.

use ripr::{CheckInput, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, diff: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-two-bindings-one-predicate-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"two-bindings-one-predicate\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), diff)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }

    fn findings(&self) -> Result<Vec<Finding>, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        Ok(output.findings)
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

const LIB: &str = "pub fn len(head: u64, tail: u64, mask: u64, cap: usize) -> usize {
    let hix = (head & mask) as usize;
    let tix = (tail & mask) as usize;

    if hix < tix {
        tix - hix
    } else if hix > tix {
        cap - (hix - tix)
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lens() {
        assert_eq!(len(1, 3, 7, 8), 2);
        assert_eq!(len(3, 1, 7, 8), 6);
    }
}
";

/// `(line, expression)` of every predicate finding, in report order.
fn predicates(findings: &[Finding]) -> Vec<(usize, String)> {
    findings
        .iter()
        .filter(|finding| finding.probe.family == ProbeFamily::Predicate)
        .map(|finding| {
            (
                finding.probe.location.line,
                finding.probe.expression.clone(),
            )
        })
        .collect()
}

fn expected() -> Vec<(usize, String)> {
    vec![
        (5, "if hix < tix {".to_string()),
        (7, "} else if hix > tix {".to_string()),
    ]
}

#[test]
fn two_changed_operands_report_each_predicate_once() -> Result<(), String> {
    let diff = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn len(head: u64, tail: u64, mask: u64, cap: usize) -> usize {
-    let hix = head & mask;
-    let tix = tail & mask;
+    let hix = (head & mask) as usize;
+    let tix = (tail & mask) as usize;
";
    let findings = TempRepo::create(LIB, diff)?.findings()?;
    let predicates = predicates(&findings);
    if predicates != expected() {
        return Err(format!(
            "each predicate is reported once, got {predicates:?}"
        ));
    }
    // The kept finding still names the changed binding that seeded it.
    for finding in &findings {
        if finding.probe.family == ProbeFamily::Predicate
            && !finding
                .evidence
                .iter()
                .any(|line| line.starts_with("binding_predicate_relation: changed binding `hix`"))
        {
            return Err(format!(
                "the first changed binding stays causal evidence: {:?}",
                finding.evidence
            ));
        }
    }
    Ok(())
}

/// Control: one changed operand already reports each predicate once, with
/// that binding as evidence. The guard must not change it.
#[test]
fn one_changed_operand_reports_the_same_predicates() -> Result<(), String> {
    let diff = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn len(head: u64, tail: u64, mask: u64, cap: usize) -> usize {
     let hix = (head & mask) as usize;
-    let tix = tail & mask;
+    let tix = (tail & mask) as usize;
";
    let findings = TempRepo::create(LIB, diff)?.findings()?;
    let predicates = predicates(&findings);
    if predicates != expected() {
        return Err(format!(
            "one changed operand keeps both predicates, got {predicates:?}"
        ));
    }
    for finding in &findings {
        if finding.probe.family == ProbeFamily::Predicate
            && !finding
                .evidence
                .iter()
                .any(|line| line.starts_with("binding_predicate_relation: changed binding `tix`"))
        {
            return Err(format!(
                "the changed binding is causal evidence: {:?}",
                finding.evidence
            ));
        }
    }
    Ok(())
}

/// Two changed bindings in different comparisons of one line are two changed
/// behaviors: each keeps its own predicate finding, as before the guard.
#[test]
fn changed_bindings_in_separate_comparisons_keep_their_own_findings() -> Result<(), String> {
    // The trailing comment names `a` next to the `c` comparison; comment
    // text is not an operand and must not merge the two.
    for comment in ["", " // a"] {
        let predicate = format!("if a < lo && c < hi {{{comment}");
        separate_comparisons_case(&predicate)?;
    }
    Ok(())
}

fn separate_comparisons_case(predicate: &str) -> Result<(), String> {
    let lib = format!(
        "pub fn both(x: u64, y: u64, lo: u64, hi: u64) -> bool {{
    let a = x & 7;
    let c = y & 7;
    {predicate}
        return true;
    }}
    false
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn boths() {{
        assert!(both(1, 1, 2, 2));
    }}
}}
"
    );
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,4 +1,4 @@
 pub fn both(x: u64, y: u64, lo: u64, hi: u64) -> bool {{
-    let a = x & 3;
-    let c = y & 3;
+    let a = x & 7;
+    let c = y & 7;
     {predicate}
"
    );
    let findings = TempRepo::create(&lib, &diff)?.findings()?;
    let predicates = predicates(&findings);
    let line = (4, predicate.to_string());
    if predicates != [line.clone(), line] {
        return Err(format!(
            "each comparison of `{predicate}` keeps its finding, got {predicates:?}"
        ));
    }
    for binding in ["a", "c"] {
        let evidence = format!("binding_predicate_relation: changed binding `{binding}`");
        if !findings.iter().any(|finding| {
            finding
                .evidence
                .iter()
                .any(|line| line.starts_with(&evidence))
        }) {
            return Err(format!("no finding names changed binding `{binding}`"));
        }
    }
    Ok(())
}

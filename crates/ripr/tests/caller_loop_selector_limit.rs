//! Public-API controls for bounded caller-loop selector domains (#5503,
//! RIPR-SPEC-0240).
//!
//! A match arm fed by a bounded `for` loop inside an `Iterator::next`, checked
//! by a test that collects the iterator and compares the whole collection,
//! reads as `static_unknown` under `rust_caller_loop_selector_unresolved`.
//! Each control below changes one thing about the shape and keeps the gap.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, diff: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-caller-loop-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"caller-loop\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
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

/// A one-line edit of `line` in `src/lib.rs`.
fn diff(line: usize, before: &str, after: &str) -> String {
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -{line},1 +{line},1 @@
-{before}
+{after}
"
    )
}

/// Every finding on `line`: one edit can carry several probes.
fn findings_on(lib: &str, diff: &str, line: usize) -> Result<Vec<Finding>, String> {
    let findings = TempRepo::create(lib, diff)?.findings()?;
    let on_line: Vec<Finding> = findings
        .into_iter()
        .filter(|finding| finding.probe.location.line == line)
        .collect();
    if on_line.is_empty() {
        return Err(format!("premise: a finding on line {line}"));
    }
    Ok(on_line)
}

const LIMIT: &str = "rust_caller_loop_selector_unresolved";

fn is_gap(finding: &Finding) -> bool {
    matches!(
        finding.class,
        ExposureClass::WeaklyExposed | ExposureClass::ReachableUnrevealed
    )
}

fn limited(finding: &Finding) -> bool {
    finding.static_limit_kind.map(|kind| kind.as_str()) == Some(LIMIT)
}

fn summary(findings: &[Finding]) -> Vec<(&'static str, &'static str, Option<&'static str>)> {
    findings
        .iter()
        .map(|f| {
            (
                f.probe.family.as_str(),
                f.class.as_str(),
                f.static_limit_kind.map(|kind| kind.as_str()),
            )
        })
        .collect()
}

/// The shape every control varies: an owner whose arm `ARM` is fed by a
/// loop over `RANGE` in `Seqs::next`, and a test over `Seqs`.
struct Shape {
    consts: &'static str,
    range: &'static str,
    loop_prefix: &'static str,
    call: &'static str,
    impl_header: &'static str,
    test_body: &'static str,
}

const SHAPE: Shape = Shape {
    consts: "const MAX_ARM: usize = 4;",
    range: "1..MAX_ARM",
    loop_prefix: "",
    call: "let max = limit(i);",
    impl_header: "impl Iterator for Seqs {",
    test_body: "let seqs = Seqs::new(2).collect::<Vec<_>>();
        assert_eq!(seqs, vec![60]);",
};

/// The source and the line of the changed arm `2 => 20,`.
fn source(shape: &Shape) -> (String, usize) {
    let lib = format!(
        "{consts}

pub struct Seqs {{
    done: bool,
}}

impl Seqs {{
    pub fn new(_n: usize) -> Self {{
        Seqs {{ done: false }}
    }}
}}

{impl_header}
    type Item = u32;
    fn next(&mut self) -> Option<u32> {{
        if self.done {{
            return None;
        }}
        self.done = true;
        let mut total = 0;
        for i in {range} {{
            {loop_prefix}
            {call}
            total += max;
        }}
        Some(total)
    }}
}}

fn limit(n: usize) -> u32 {{
    match n {{
        1 => 10,
        2 => 20,
        3 => 30,
        4 => 40,
        _ => unreachable!(),
    }}
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn collects_the_sequences() {{
        {test_body}
    }}
}}
",
        consts = shape.consts,
        range = shape.range,
        loop_prefix = shape.loop_prefix,
        call = shape.call,
        impl_header = shape.impl_header,
        test_body = shape.test_body,
    );
    let line = lib
        .lines()
        .position(|text| text.trim() == "2 => 20,")
        .map_or(0, |at| at + 1);
    (lib, line)
}

fn arm_edit(line: usize, arm: &str) -> String {
    diff(
        line,
        &format!("        {arm} => 99,"),
        &format!("        {arm} => {},", if arm == "2" { 20 } else { 40 }),
    )
}

fn findings_for(shape: &Shape, arm: &str) -> Result<Vec<Finding>, String> {
    let (lib, mut line) = source(shape);
    if arm == "4" {
        line += 2;
    }
    findings_on(&lib, &arm_edit(line, arm), line)
}

/// Some finding on the line carries the limit and none is still a gap.
fn withheld(shape: &Shape, arm: &str, label: &str) -> Result<(), String> {
    let findings = findings_for(shape, arm)?;
    if findings.iter().any(is_gap) || !findings.iter().any(limited) {
        return Err(format!(
            "{label}: expected the caller-loop limit, got {:?}",
            summary(&findings)
        ));
    }
    Ok(())
}

/// No finding carries the limit and some finding is still a gap.
fn keeps_gap(shape: &Shape, arm: &str, label: &str) -> Result<(), String> {
    let findings = findings_for(shape, arm)?;
    if findings.iter().any(limited) || !findings.iter().any(is_gap) {
        return Err(format!(
            "{label}: expected the gap kept, got {:?}",
            summary(&findings)
        ));
    }
    Ok(())
}

#[test]
fn an_arm_fed_by_a_bounded_caller_loop_is_withheld() -> Result<(), String> {
    withheld(&SHAPE, "2", "const-bounded loop")?;
    withheld(
        &Shape {
            range: "1..=3",
            ..SHAPE
        },
        "2",
        "inclusive literal range",
    )
}

#[test]
fn the_limit_names_the_loop_domain_and_its_provenance() -> Result<(), String> {
    let findings = findings_for(&SHAPE, "2")?;
    let finding = findings
        .iter()
        .find(|finding| limited(finding))
        .ok_or_else(|| format!("premise: {:?}", summary(&findings)))?;
    let text = format!("{:?}", finding.evidence);
    if !(text.contains("1..3") && text.contains("src/lib.rs:") && text.contains("Seqs")) {
        return Err(format!("expected domain and provenance in {text}"));
    }
    Ok(())
}

#[test]
fn the_domain_bound_is_exact() -> Result<(), String> {
    let at = Shape {
        consts: "",
        range: "1..65",
        ..SHAPE
    };
    withheld(&at, "2", "64 selectors")?;
    let over = Shape {
        consts: "",
        range: "1..66",
        ..SHAPE
    };
    keeps_gap(&over, "2", "65 selectors")
}

#[test]
fn an_arm_outside_the_domain_keeps_the_gap() -> Result<(), String> {
    // Arm 4 exists but the loop feeds only 1..=3.
    keeps_gap(&SHAPE, "4", "arm outside the loop domain")
}

#[test]
fn a_loop_shape_ripr_cannot_read_keeps_the_gap() -> Result<(), String> {
    keeps_gap(
        &Shape {
            consts: "",
            range: "1..bound()",
            ..SHAPE
        },
        "2",
        "unresolved bound",
    )?;
    keeps_gap(
        &Shape {
            loop_prefix: "if self.done { continue; }",
            ..SHAPE
        },
        "2",
        "an exit before the call",
    )?;
    keeps_gap(
        &Shape {
            call: "let max = limit(i + 1);",
            ..SHAPE
        },
        "2",
        "computed selector",
    )?;
    keeps_gap(
        &Shape {
            loop_prefix: "let i = 4;",
            ..SHAPE
        },
        "2",
        "rebound loop variable",
    )?;
    // A local named like the file-level bound may shadow it.
    keeps_gap(
        &Shape {
            consts: "const END: usize = 4;",
            range: "1..END",
            loop_prefix: "let END = 2;",
            ..SHAPE
        },
        "2",
        "shadowed named bound",
    )?;
    keeps_gap(
        &Shape {
            impl_header: "impl Seqs {\n    fn step(&mut self) -> Option<u32> { None }\n}\nimpl Other for Seqs {",
            ..SHAPE
        },
        "2",
        "caller is not an Iterator impl",
    )
}

#[test]
fn a_consumer_that_is_not_an_exhausting_exact_collect_keeps_the_gap() -> Result<(), String> {
    for (label, body) in [
        (
            "take",
            "let seqs = Seqs::new(2).take(1).collect::<Vec<_>>();\n        assert_eq!(seqs, vec![60]);",
        ),
        (
            "length only",
            "let seqs = Seqs::new(2).collect::<Vec<_>>();\n        assert_eq!(seqs.len(), 1);",
        ),
        (
            "mutated collection",
            "let mut seqs = Seqs::new(2).collect::<Vec<_>>();\n        seqs.pop();\n        assert_eq!(seqs, vec![]);",
        ),
        (
            "inequality only",
            "let seqs = Seqs::new(2).collect::<Vec<_>>();\n        assert_ne!(seqs, vec![]);\n        assert_eq!(1, 1);",
        ),
        ("no assertion", "let _ = Seqs::new(2).collect::<Vec<_>>();"),
    ] {
        keeps_gap(
            &Shape {
                test_body: body,
                ..SHAPE
            },
            "2",
            label,
        )?;
    }
    Ok(())
}

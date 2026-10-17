//! Exact one-hop primitive field observation, with independent sink controls.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const OWNER: &str = "#[derive(Debug, PartialEq)]
pub struct Stat { pub missing_duration_count: usize, pub sibling: usize }
pub fn stats(total: usize, known: usize) -> Stat {
    Stat {
        missing_duration_count: total - known,
        sibling: 19,
    }
}
";
const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -5 +5 @@
-        missing_duration_count: 0,
+        missing_duration_count: total - known,
";

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = std::env::temp_dir();
        std::fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
        let path = parent.join(format!(
            "ripr-one-hop-field-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        // Claim the root exclusively before writing or allowing Drop to remove it.
        std::fs::create_dir(&path).map_err(|error| error.to_string())?;
        let scratch = Self(path);
        std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
        Ok(scratch)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn source(wrapper: &str, assertions: &str) -> String {
    format!(
        "{OWNER}\npub fn projected(total: usize, known: usize) -> usize {{\n{wrapper}\n}}\n#[test]\nfn pin() {{\n{assertions}\n}}\n"
    )
}

fn finding(source: &str, diff: &str) -> Result<Finding, String> {
    let context_line = source
        .lines()
        .position(|line| line.trim() == "missing_duration_count: total - known,")
        .ok_or("missing changed-field source line")?
        + 1;
    let diff = diff.replace(
        "@@ -5 +5 @@",
        &format!("@@ -{context_line} +{context_line} @@"),
    );
    let scratch = Scratch::new()?;
    std::fs::write(
        scratch.0.join("Cargo.toml"),
        "[package]\nname = \"field-projection\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
    let diff_path = scratch.0.join("change.diff");
    std::fs::write(&diff_path, diff).map_err(|error| error.to_string())?;
    let report = check_workspace(CheckInput {
        root: scratch.0.clone(),
        diff_file: Some(diff_path),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let mut subjects = report.findings.into_iter().filter(|finding| {
        finding.probe.family == ProbeFamily::FieldConstruction
            && finding
                .probe
                .expression
                .starts_with("missing_duration_count:")
    });
    let first = subjects.next().ok_or("missing changed-field subject")?;
    if subjects.next().is_some() {
        return Err("ambiguous changed-field subjects".to_owned());
    }
    Ok(first)
}

const PROJECTS_FIELD: &str = "    stats(total, known).missing_duration_count";
const EXACT_PIN: &str = "    assert_eq!(projected(20, 1), 19);";

#[test]
fn projected_primitive_field_has_an_exact_observation() -> Result<(), String> {
    let finding = finding(&source(PROJECTS_FIELD, EXACT_PIN), DIFF)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert_eq!(finding.ripr.propagate.state.as_str(), "yes");
    assert_eq!(finding.ripr.reveal.discriminate.state.as_str(), "yes");
    Ok(())
}

#[test]
fn direct_field_observation_remains_exposed() -> Result<(), String> {
    let source = source(
        PROJECTS_FIELD,
        "    assert_eq!(stats(20, 1).missing_duration_count, 19);",
    );
    assert_eq!(finding(&source, DIFF)?.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn wrong_sink_discarded_result_and_sibling_field_cannot_borrow_a_strong_oracle()
-> Result<(), String> {
    let wrong_sink = format!(
        "{}\nfn sentinel() -> usize {{ 19 }}\n",
        source(
            PROJECTS_FIELD,
            "    let _ = projected(20, 1);\n    assert_eq!(sentinel(), 19);",
        )
    );
    for (name, source) in [
        ("wrong sink", wrong_sink),
        (
            "discarded owner result",
            source("    let _ = stats(total, known);\n    19", EXACT_PIN),
        ),
        (
            "sibling field",
            source("    stats(total, known).sibling", EXACT_PIN),
        ),
    ] {
        let finding = finding(&source, DIFF)?;
        assert_ne!(finding.class, ExposureClass::Exposed, "{name}: {finding:?}");
        assert!(
            finding
                .ripr
                .propagate
                .summary
                .contains("helper_result_not_forwarded"),
            "{name}: {finding:?}"
        );
    }
    Ok(())
}

#[test]
fn transformed_and_extra_hop_paths_keep_the_existing_limit() -> Result<(), String> {
    let cases = [
        source(
            "    stats(total, known).missing_duration_count + 1",
            "    assert_eq!(projected(20, 1), 20);",
        ),
        source(
            "    let result = stats(total, known);\n    result.missing_duration_count",
            EXACT_PIN,
        ),
        source(PROJECTS_FIELD, "    assert_eq!(projected(10 + 10, 1), 19);"),
        format!(
            "{}\nfn outer(total: usize, known: usize) -> usize {{ projected(total, known) }}\n",
            source(PROJECTS_FIELD, "    assert_eq!(outer(20, 1), 19);")
        ),
        source(PROJECTS_FIELD, EXACT_PIN).replace(
            "missing_duration_count: usize",
            "missing_duration_count: Count",
        ) + "\ntype Count = usize;\n",
    ];
    for source in cases {
        assert_ne!(
            finding(&source, DIFF)?.class,
            ExposureClass::Exposed,
            "{source}"
        );
    }
    Ok(())
}

#[test]
fn execution_refusals_and_shadowed_wrapper_cannot_supply_the_projection_pin() -> Result<(), String>
{
    for source in [
        source(
            PROJECTS_FIELD,
            "    if false { assert_eq!(projected(20, 1), 19); }",
        ),
        source(
            PROJECTS_FIELD,
            "    let check = || { assert_eq!(projected(20, 1), 19); };\n    let _ = check;",
        ),
        source(
            PROJECTS_FIELD,
            "    fn projected(_: usize, _: usize) -> usize { 19 }\n    assert_eq!(projected(20, 1), 19);",
        ),
        format!(
            "macro_rules! assert_eq {{ ($($t:tt)*) => {{}} }}\n{}",
            source(PROJECTS_FIELD, EXACT_PIN)
        ),
    ] {
        assert_ne!(
            finding(&source, DIFF)?.class,
            ExposureClass::Exposed,
            "{source}"
        );
    }
    Ok(())
}

#[test]
fn reversed_and_mixed_oracles_keep_the_exact_projection_identity() -> Result<(), String> {
    for assertions in [
        "    assert_eq!(19, projected(20, 1));",
        "    assert_eq!(sentinel(), 19);\n    assert_eq!(projected(20, 1), 19);",
    ] {
        let source = source(PROJECTS_FIELD, assertions) + "\nfn sentinel() -> usize { 19 }\n";
        assert_eq!(
            finding(&source, DIFF)?.class,
            ExposureClass::Exposed,
            "{source}"
        );
    }
    Ok(())
}

#[test]
fn single_field_record_keeps_the_retained_count_projection() -> Result<(), String> {
    let source = source(PROJECTS_FIELD, EXACT_PIN)
        .replace(", pub sibling: usize", "")
        .replace("        sibling: 19,\n", "");
    assert_eq!(finding(&source, DIFF)?.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn panic_outcome_and_custom_drop_are_outside_the_projection_slice() -> Result<(), String> {
    for source in [
        source(PROJECTS_FIELD, "    assert_eq!(projected(20, 1), 999);")
            .replace("#[test]", "#[test]\n#[should_panic]"),
        source(PROJECTS_FIELD, EXACT_PIN) + "\nimpl Drop for Stat { fn drop(&mut self) {} }\n",
    ] {
        assert_ne!(
            finding(&source, DIFF)?.class,
            ExposureClass::Exposed,
            "{source}"
        );
    }
    Ok(())
}

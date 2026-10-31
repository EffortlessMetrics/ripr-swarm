//! Public-API controls for a test module that carries the owner's `cfg` gate
//! (#9643, RIPR-SPEC-0240).
//!
//! When the changed owner and the test that pins it sit under the same
//! unevaluated `cfg`, the test is compiled whenever the changed code is. ripr
//! cannot know which feature set a run enables, so the refused `assert_eq!`
//! reads `static_unknown` with `rust_assertion_context_unresolved`, never as a
//! gap and never as credit. A gate only the test carries, a different gate, a
//! provably disabled gate and an `#[ignore]` test keep the gap.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// `stamp` on line 3 with the changed tail on line 4, inside `owner_gate`'s
/// module (line 1 attribute, line 2 `pub mod v4 {`); the test module carries
/// `test_gate`.
fn source(owner_gate: &str, test_gate: &str, test_attrs: &str) -> String {
    // Keep `stamp`'s changed line at a fixed number: a missing gate line is
    // replaced by a comment.
    let first = if owner_gate.is_empty() {
        "// ungated"
    } else {
        owner_gate
    };
    let open = format!("{first}\npub mod v4 {{");
    format!(
        "{open}
    pub fn stamp(raw: u16) -> u16 {{
        0x4000 | (raw & 0x0fff)
    }}

    {test_gate}
    mod tests {{
        use super::stamp;

        #[test]
        {test_attrs}
        fn stamp_sets_the_version_nibble() {{
            assert_eq!(stamp(0xffff), 0x4fff);
            assert_eq!(stamp(0x0000), 0x4000);
        }}
    }}
}}
"
    )
}

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,6 +1,6 @@
 // ungated
 pub mod v4 {
     pub fn stamp(raw: u16) -> u16 {
-        (raw & 0x0fff) | 0x4000
+        0x4000 | (raw & 0x0fff)
     }
 }
";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-shared-cfg-gate-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"gates\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[features]\nv4 = []\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), DIFF)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The only `return_value` finding on the changed line 4.
fn finding(owner_gate: &str, test_gate: &str, test_attrs: &str) -> Result<Finding, String> {
    let repo = TempRepo::create(&source(owner_gate, test_gate, test_attrs))?;
    let output = check_workspace(CheckInput {
        root: repo.root.clone(),
        base: None,
        diff_file: Some(repo.root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let mut on_line = output.findings.iter().filter(|finding| {
        finding.probe.location.line == 4 && finding.probe.family == ripr::ProbeFamily::ReturnValue
    });
    match (on_line.next(), on_line.next()) {
        (Some(finding), None) => Ok(finding.clone()),
        other => Err(format!(
            "premise: exactly one return_value finding on line 4, got {other:?} in {:?}",
            output.findings
        )),
    }
}

fn assert_withheld(finding: &Finding, what: &str) {
    assert_eq!(finding.class, ExposureClass::StaticUnknown, "{what}");
    assert_eq!(
        finding.static_limit_kind.map(|kind| kind.as_str()),
        Some("rust_assertion_context_unresolved"),
        "{what}"
    );
}

fn assert_gap(finding: &Finding, what: &str) {
    assert!(
        matches!(
            finding.class,
            ExposureClass::WeaklyExposed | ExposureClass::ReachableUnrevealed
        ),
        "{what}: {finding:?}"
    );
    assert_eq!(finding.static_limit_kind, None, "{what}");
}

#[test]
fn a_test_module_under_the_owners_gate_is_unknown_not_a_gap() -> Result<(), String> {
    let feature = "#[cfg(feature = \"v4\")]";
    // The #9643 shape: the test module sits inside the gated owner module and
    // carries only `cfg(test)`, so it inherits the owner's gate.
    let found = finding(feature, "#[cfg(test)]", "")?;
    assert_withheld(&found, "test module nested in the gated owner module");

    let found = finding(feature, &format!("{feature}\n    #[cfg(test)]"), "")?;
    assert_withheld(&found, "shared feature gate plus cfg(test)");
    // The refusal is disclosed, not hidden.
    assert!(
        found
            .evidence
            .iter()
            .any(|line| line.starts_with("assertion not credited: ")),
        "{:?}",
        found.evidence
    );

    // Whitespace in the attribute does not make the gates differ.
    let found = finding(
        "#[cfg( feature = \"v4\" )]",
        "#[cfg(feature=\"v4\")]\n    #[cfg(test)]",
        "",
    )?;
    assert_withheld(&found, "shared gate spelled with different whitespace");
    Ok(())
}

#[test]
fn a_gate_the_owner_does_not_share_keeps_the_gap() -> Result<(), String> {
    let feature = "#[cfg(feature = \"v4\")]";
    let test_gate = format!("{feature}\n    #[cfg(test)]");
    let found = finding("", &test_gate, "")?;
    assert_gap(
        &found,
        "owner ungated: the default run never compiles the test",
    );

    let found = finding("#[cfg(feature = \"v7\")]", &test_gate, "")?;
    assert_gap(&found, "owner gated by another feature");

    let found = finding("#[cfg(any())]", "#[cfg(any())]\n    #[cfg(test)]", "")?;
    assert_gap(&found, "a shared gate that is provably off");

    let found = finding(feature, &test_gate, "#[ignore]")?;
    assert_gap(&found, "an ignored test under the shared gate");
    Ok(())
}

/// `lib` with its changed expression on `line`, diffed without context.
fn finding_in(lib: &str, line: usize) -> Result<Finding, String> {
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\nindex 1111111..2222222 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{line},1 +{line},1 @@\n-        (raw & 0x0fff) | 0x4000\n+        0x4000 | (raw & 0x0fff)\n"
    );
    let repo = TempRepo::create(lib)?;
    std::fs::write(repo.root.join("diff.patch"), diff).map_err(|error| error.to_string())?;
    let output = check_workspace(CheckInput {
        root: repo.root.clone(),
        base: None,
        diff_file: Some(repo.root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let mut on_line = output.findings.iter().filter(|finding| {
        finding.probe.location.line == line
            && finding.probe.family == ripr::ProbeFamily::ReturnValue
    });
    match (on_line.next(), on_line.next()) {
        (Some(finding), None) => Ok(finding.clone()),
        other => Err(format!("premise: one return_value finding, got {other:?}")),
    }
}

/// Two nested gated modules around the owner and the test; `inner_test_gate`
/// is an extra attribute on the test module.
fn nested(inner_test_gate: &str) -> String {
    format!(
        "#[cfg(feature = \"a\")]
pub mod a {{
    #[cfg(feature = \"b\")]
    pub mod b {{
        pub fn stamp(raw: u16) -> u16 {{
        0x4000 | (raw & 0x0fff)
        }}

        {inner_test_gate}
        #[cfg(test)]
        mod tests {{
            use super::stamp;

            #[test]
            fn stamp_sets_the_version_nibble() {{
                assert_eq!(stamp(0xffff), 0x4fff);
            }}
        }}
    }}
}}
"
    )
}

#[test]
fn every_test_gate_must_be_carried_by_the_owner() -> Result<(), String> {
    // Two nesting levels, both shared with the owner: withheld.
    let found = finding_in(&nested(""), 6)?;
    assert_withheld(&found, "two shared gate levels");
    // A shared gate beside an extra independent one on the test: the extra
    // gate can keep the test out of a run the owner is in, so the gap stays.
    let found = finding_in(&nested("#[cfg(feature = \"c\")]"), 6)?;
    assert_gap(&found, "shared gates plus an extra independent test gate");
    // The extra gate is also refused when it is only a negation of a shared one.
    let found = finding_in(&nested("#[cfg(not(feature = \"b\"))]"), 6)?;
    assert_gap(&found, "shared gates plus a negated shared feature");
    Ok(())
}

#[test]
fn the_ungated_owner_control_refuses_for_the_gate_not_for_a_comment() -> Result<(), String> {
    // The helper writes `// ungated` where the owner's attribute would be;
    // the test still carries a real gate, so the refusal exists and is a gap.
    let feature = "#[cfg(feature = \"v4\")]";
    let lib = source("", &format!("{feature}\n    #[cfg(test)]"), "");
    assert!(lib.starts_with("// ungated\npub mod v4"), "{lib}");
    let found = finding("", &format!("{feature}\n    #[cfg(test)]"), "")?;
    assert_gap(&found, "owner ungated");
    assert!(
        found
            .evidence
            .iter()
            .any(|line| line.contains("cannot evaluate for test builds")),
        "the test's gate must be the named refusal: {:?}",
        found.evidence
    );
    Ok(())
}

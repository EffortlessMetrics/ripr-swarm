//! Public-path controls for a slice-length boundary whose tests pass literal
//! `&[..]` arguments directly in assertions (#8061 lane, abstention).
//!
//! The empty literal reaches `xs.len() == 0` and an exact pin on that same call
//! credits it; literals that never reach the boundary keep the missing
//! discriminator. A computed, bound or conditional argument, an extra caller,
//! or a non-pinning assertion keeps the abstention or gap.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const PREFIX: &str = "pub fn average(xs: &[u32]) -> u32 {
    if xs.len() == 0 {
        return 0;
    }
    xs.iter().sum::<u32>() / xs.len() as u32
}
";

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,5 +1,5 @@
 pub fn average(xs: &[u32]) -> u32 {
-    if xs.is_empty() {
+    if xs.len() == 0 {
         return 0;
     }
     xs.iter().sum::<u32>() / xs.len() as u32
";

fn finding(extra: &str, body: &str) -> Result<Finding, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let root: PathBuf = std::env::temp_dir().join(format!(
        "ripr-slice-direct-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
    let result = (|| {
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"direct\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[workspace]\n",
        )
        .map_err(|e| e.to_string())?;
        let source = format!(
            "{PREFIX}{extra}
#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn average_cases() {{
{body}
    }}
}}
"
        );
        std::fs::write(root.join("src/lib.rs"), source).map_err(|e| e.to_string())?;
        std::fs::write(root.join("diff.patch"), DIFF).map_err(|e| e.to_string())?;
        let output = check_workspace(CheckInput {
            root: root.clone(),
            base: None,
            diff_file: Some(root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let mut found: Vec<_> = output
            .findings
            .into_iter()
            .filter(|f| f.probe.expression == "xs.len() == 0")
            .collect();
        if found.len() != 1 {
            return Err(format!(
                "expected one anchored finding, got {}",
                found.len()
            ));
        }
        found.pop().ok_or_else(|| "missing finding".to_string())
    })();
    let _ = std::fs::remove_dir_all(&root);
    result
}

fn mentions_empty_length(finding: &Finding) -> bool {
    finding
        .activation
        .observed_values
        .iter()
        .map(|fact| fact.value.as_str())
        .chain(
            finding
                .activation
                .missing_discriminators
                .iter()
                .map(|fact| fact.value.as_str()),
        )
        .any(|value| value == "xs.len() == 0")
}

#[test]
fn an_empty_literal_pinned_exactly_credits_the_boundary() -> Result<(), String> {
    let finding = finding(
        "",
        "        assert_eq!(average(&[]), 0);\n        assert_eq!(average(&[2, 4]), 3);",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert!(
        finding
            .activation
            .observed_values
            .iter()
            .any(|fact| fact.value == "xs.len() == 0" && fact.text.contains("average(&[])"))
    );
    Ok(())
}

#[test]
fn nonempty_literals_keep_the_missing_empty_discriminator() -> Result<(), String> {
    let finding = finding(
        "",
        "        assert_eq!(average(&[2, 4]), 3);\n        assert_eq!(average(&[6]), 6);",
    )?;
    assert_ne!(finding.class, ExposureClass::Exposed);
    assert!(
        finding
            .activation
            .missing_discriminators
            .iter()
            .any(|fact| fact.value == "xs.len() == 0"),
        "{:?}",
        finding.activation
    );
    assert!(
        finding
            .activation
            .observed_values
            .iter()
            .all(|fact| fact.value != "xs.len() == 0")
    );
    Ok(())
}

#[test]
fn an_empty_literal_without_an_exact_pin_is_not_credited() -> Result<(), String> {
    let finding = finding(
        "",
        "        assert!(average(&[]) < 100);\n        assert!(average(&[2, 4]) < 100);",
    )?;
    assert_ne!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn hidden_or_conditional_arguments_cannot_supply_a_length() -> Result<(), String> {
    for body in [
        // A bound slice is not a literal argument.
        "        let none: &[u32] = &[];\n        assert_eq!(average(none), 0);",
        // The call result is bound, so the assertion no longer names the call.
        "        let got = average(&[2, 4]);\n        assert_eq!(got, 3);",
        // A computed element list is not a literal table.
        "        assert_eq!(average(&[1 + 1, 4]), 3);",
        // A call that never runs cannot witness or exclude an input.
        "        if false {\n            assert_eq!(average(&[]), 0);\n        }\n        assert_eq!(average(&[2, 4]), 3);",
        // An earlier return skips the empty call.
        "        assert_eq!(average(&[2, 4]), 3);\n        return;\n        #[allow(unreachable_code)]\n        assert_eq!(average(&[]), 0);",
    ] {
        let finding = finding("", body)?;
        assert!(
            !mentions_empty_length(&finding),
            "{body}: {:?}",
            finding.activation
        );
        assert_ne!(finding.class, ExposureClass::Exposed, "{body}");
    }
    Ok(())
}

#[test]
fn another_production_use_keeps_the_input_set_open() -> Result<(), String> {
    let finding = finding(
        "\npub fn caller(values: &[u32]) -> u32 {\n    average(values)\n}\n",
        "        assert_eq!(average(&[]), 0);\n        assert_eq!(average(&[2, 4]), 3);",
    )?;
    assert!(!mentions_empty_length(&finding), "{:?}", finding.activation);
    assert_ne!(finding.class, ExposureClass::Exposed);
    Ok(())
}

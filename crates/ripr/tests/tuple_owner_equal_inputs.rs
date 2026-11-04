//! External tests must distinguish both versions of a tuple-struct owner (#12808).
use ripr::domain::ExposureClass;
use ripr::{CheckInput, Mode, OutputFormat, check_workspace};

fn finding(body: &str) -> Result<ripr::Finding, Box<dyn std::error::Error>> {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "ripr-tuple-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root)?;
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::create_dir_all(root.join("tests"))?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n",
    )?;
    std::fs::write(
        root.join("src/lib.rs"),
        "use std::ops::Add;\n#[derive(Debug, Clone, Copy, PartialEq)]\npub struct Money(pub i64);\nimpl Add for Money {\n    type Output = Money;\n    fn add(self, other: Money) -> Money {\n        Self(other.0 + self.0)\n    }\n}\n",
    )?;
    std::fs::write(
        root.join("tests/t.rs"),
        format!("use demo::Money;\nuse std::ops::Add;\n#[test]\nfn adds() {{\n{body}\n}}\n"),
    )?;
    std::fs::write(
        root.join("change.diff"),
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -7,1 +7,1 @@\n-        Self(other.0 - self.0)\n+        Self(other.0 + self.0)\n",
    )?;
    let result = check_workspace(CheckInput {
        root: root.to_path_buf(),
        diff_file: Some(root.join("change.diff")),
        mode: Mode::Draft,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })
    .map_err(std::io::Error::other)?;
    let findings: Vec<_> = result
        .findings
        .iter()
        .filter(|f| f.probe.location.line == 7)
        .collect();
    assert_eq!(findings.len(), 1, "expected one changed return finding");
    let result = findings[0].clone();
    std::fs::remove_dir_all(root)?;
    Ok(result)
}

fn class(body: &str) -> Result<ExposureClass, Box<dyn std::error::Error>> {
    Ok(finding(body)?.class)
}

#[test]
fn zero_projection() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        class("let r = Money(0).add(Money(0));\nassert_eq!(r.0, 0);")?,
        ExposureClass::WeaklyExposed
    );
    Ok(())
}
#[test]
fn zero_whole() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        class("let r = Money(0).add(Money(0));\nassert_eq!(r, Money(0));")?,
        ExposureClass::WeaklyExposed
    );
    Ok(())
}
#[test]
fn separating_projection() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        class("let r = Money(2).add(Money(3));\nassert_eq!(r.0, 5);")?,
        ExposureClass::Exposed
    );
    Ok(())
}
#[test]
fn mixed_inputs_keep_the_separating_test() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        class(
            "let r = Money(0).add(Money(0));\nassert_eq!(r.0, 0);\nlet s = Money(2).add(Money(3));\nassert_eq!(s.0, 5);"
        )?,
        ExposureClass::Exposed
    );
    Ok(())
}
#[test]
fn unknown_operand_does_not_claim_invisibility() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        class("let n = 0;\nlet r = Money(n).add(Money(0));\nassert_eq!(r.0, 0);")?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn separate_related_test_can_discriminate() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        class(
            "let r = Money(0).add(Money(0));\nassert_eq!(r.0, 0);\n}\n#[test]\nfn separates() {\nlet s = Money(2).add(Money(3));\nassert_eq!(s.0, 5);"
        )?,
        ExposureClass::Exposed
    );
    Ok(())
}
#[test]
fn overflow_and_shadowed_constructor_decline_the_witness() -> Result<(), Box<dyn std::error::Error>>
{
    for body in [
        "let r = Money(9223372036854775807).add(Money(1));\nassert_eq!(r.0, 0);",
        "#[allow(non_snake_case)]\nfn Money(n: i64) -> demo::Money { demo::Money(n) }\nlet r = Money(0).add(Money(0));\nassert_eq!(r.0, 0);",
    ] {
        let finding = finding(body)?;
        assert!(
            !finding.related_tests.is_empty(),
            "premise: test is related"
        );
        assert!(
            !finding
                .ripr
                .infect
                .summary
                .contains("Every related test calls the owner with literal inputs"),
            "{}",
            finding.ripr.infect.summary
        );
    }
    Ok(())
}

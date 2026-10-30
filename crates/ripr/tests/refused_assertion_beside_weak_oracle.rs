//! A refused `assert_eq!` on the owner beside an admitted weaker assertion
//! must not turn into a `weakly_exposed` repair request when the refusal is a
//! limit of ripr's own reading: the refused assertion may be the exact oracle
//! the gap asks for (#5431, RIPR-SPEC-0240).
use ripr::{CheckInput, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;

#[test]
fn an_analyzer_limit_refusal_beside_a_weak_oracle_is_not_a_gap() -> Result<(), String> {
    let root_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("refused_beside_weak");
    // (name, extra test-module line, test body, expected class)
    let cases = [
        // The glob may rebind `assert_eq!`: a limit.
        (
            "glob_refused_eq_plus_weak",
            "use mystery::*;",
            "assert_eq!(tier(100), 2);\n        assert!(tier(100) > 1);",
            "static_unknown",
        ),
        // Control: the weak assertion alone is a real weak oracle.
        ("weak_only", "", "assert!(tier(100) > 1);", "weakly_exposed"),
        // The refused assertion does not call the owner, so it could not
        // have been the oracle.
        (
            "glob_refused_unrelated_eq_plus_weak",
            "use mystery::*;",
            "assert_eq!(1 + 1, 2);\n        assert!(tier(100) > 1);",
            "weakly_exposed",
        ),
        // A branch can keep the assertion from running: a real gap that a
        // runtime control pins, not a limit.
        (
            "branch_refused_eq_plus_weak",
            "fn flag() -> bool { false }",
            "if flag() {\n            assert_eq!(tier(100), 2);\n        }\n        assert!(tier(100) > 1);",
            "weakly_exposed",
        ),
        // The boundary input is never supplied: that gap stands whatever the
        // refused assertion compares.
        (
            "glob_refused_eq_off_boundary_plus_weak",
            "use mystery::*;",
            "assert_eq!(tier(250), 2);\n        assert!(tier(250) > 1);",
            "weakly_exposed",
        ),
    ];
    let mut wrong = Vec::new();
    for (name, extra, body, expected) in cases {
        let source = format!(
            "pub fn tier(n: u8) -> u8 {{\n    if n > 100 {{ 2 }} else {{ 1 }}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    {extra}\n\n    #[test]\n    fn tiers() {{\n        {body}\n    }}\n}}\n"
        );
        let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -2,1 +2,1 @@\n-    if n >= 100 { 2 } else { 1 }\n+    if n > 100 { 2 } else { 1 }\n";
        let root = root_dir.join(name);
        std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"tier_subject\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n",
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(root.join("src/lib.rs"), &source).map_err(|e| e.to_string())?;
        std::fs::write(root.join("diff.patch"), diff).map_err(|e| e.to_string())?;
        let report = check_workspace(CheckInput {
            root: root.clone(),
            diff_file: Some(root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let classes: Vec<&str> = report
            .findings
            .iter()
            .filter(|f| f.probe.family.as_str() == "predicate")
            .map(|f| f.class.as_str())
            .collect();
        if classes != [expected] {
            wrong.push(format!("{name}: expected {expected}, got {classes:?}"));
        }
        if expected == "static_unknown" {
            let withheld = report
                .findings
                .iter()
                .any(|f| f.static_limit_kind.is_some());
            if !withheld {
                wrong.push(format!("{name}: no named limit"));
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    Ok(())
}

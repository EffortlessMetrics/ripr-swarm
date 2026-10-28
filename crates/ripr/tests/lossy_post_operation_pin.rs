//! A literal pin whose expected value the owner's lossy final operation
//! (a mask, a shift, a clamp, a saturating subtraction, a divisor) produces
//! for the old expression as well cannot notice the change, so it must not
//! credit `exposed` (#10737).
use ripr::{CheckInput, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;

#[test]
fn a_pin_the_lossy_operation_hides_the_change_from_never_credits_exposed() -> Result<(), String> {
    let root_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("lossy_post_operation_pin");
    // (name, owner expression over `x` containing the changed `x * 3`,
    //  test body, credited)
    let cases = [
        ("control_plain", "x * 3", "assert_eq!(scale(4), 12);", true),
        (
            "shift_hides_it",
            "(x * 3) >> 4",
            "assert_eq!(scale(4), 0);",
            false,
        ),
        (
            "shift_shows_it",
            "(x * 3) >> 4",
            "assert_eq!(scale(6), 1);",
            true,
        ),
        (
            "mask_hides_it",
            "(x * 3) & 1",
            "assert_eq!(scale(4), 0);",
            false,
        ),
        (
            "mask_shows_it",
            "(x * 3) & 1",
            "assert_eq!(scale(1), 1);",
            true,
        ),
        (
            "high_mask_hides_it",
            "(x * 3) & 0xF0",
            "assert_eq!(scale(4), 0);",
            false,
        ),
        (
            "clamp_hides_it",
            "(x * 3).clamp(0, 5)",
            "assert_eq!(scale(4), 5);",
            false,
        ),
        (
            "clamp_shows_it",
            "(x * 3).clamp(0, 5)",
            "assert_eq!(scale(1), 3);",
            true,
        ),
        (
            "narrow_literal_saturation_shows_it",
            "(x * 3).saturating_sub(if 255u8.saturating_add(1) == 255 { 0 } else { 5 })",
            "assert_eq!(scale(1), 3);",
            true,
        ),
        (
            "saturation_hides_it",
            "(x * 3).saturating_sub(100)",
            "assert_eq!(scale(4), 0);",
            false,
        ),
        (
            "saturation_shows_it",
            "(x * 3).saturating_sub(100)",
            "assert_eq!(scale(40), 20);",
            true,
        ),
        (
            "divisor_hides_it",
            "(x * 3) / 100",
            "assert_eq!(scale(4), 0);",
            false,
        ),
        (
            "divisor_shows_it",
            "(x * 3) / 100",
            "assert_eq!(scale(40), 1);",
            true,
        ),
    ];
    let mut wrong = Vec::new();
    for (name, expression, body, credited) in cases {
        let source = format!(
            "pub fn scale(x: u32) -> u32 {{\n    {expression}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn scales() {{\n        {body}\n    }}\n}}\n"
        );
        let new_line = format!("    {expression}");
        let old_line = new_line.replace("x * 3", "x * 2");
        let diff = format!(
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -2,1 +2,1 @@\n-{old_line}\n+{new_line}\n"
        );
        let root = root_dir.join(name);
        std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"scale_subject\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n",
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(root.join("src/lib.rs"), &source).map_err(|e| e.to_string())?;
        std::fs::write(root.join("diff.patch"), &diff).map_err(|e| e.to_string())?;
        let report = check_workspace(CheckInput {
            root: root.clone(),
            diff_file: Some(root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let classes: Vec<String> = report
            .findings
            .iter()
            .map(|f| format!("{}={}", f.probe.family.as_str(), f.class.as_str()))
            .collect();
        let exposed = report
            .findings
            .iter()
            .any(|f| f.probe.family.as_str() == "return_value" && f.class.as_str() == "exposed");
        if report.findings.is_empty() || exposed != credited {
            wrong.push(format!("{name}: {classes:?}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    Ok(())
}

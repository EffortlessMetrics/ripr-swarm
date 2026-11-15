//! #14387: an exact pin credits a changed early `return <value>;` inside a
//! guard when the closed integer subset shows that the pinned call's literal
//! arguments make the two versions return different values, and the pinned
//! value is one of them. Every other shape keeps its earlier reading.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

fn lib(function: &str, assertions: &str) -> String {
    format!(
        "{function}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn it_works() {{
        {assertions}
    }}
}}
"
    )
}

/// A unified diff covering the whole function, with correct hunk line counts,
/// so every changed line pairs at its real source line.
fn fn_diff(before_fn: &str, after_fn: &str) -> String {
    let before_lines: Vec<&str> = before_fn.lines().collect();
    let after_lines: Vec<&str> = after_fn.lines().collect();
    assert_eq!(
        before_lines.len(),
        after_lines.len(),
        "fixture functions must align line for line"
    );
    assert!(
        before_lines
            .iter()
            .zip(after_lines.iter())
            .any(|(before, after)| before != after),
        "fixture functions must differ somewhere"
    );
    let count = before_lines.len();
    let mut hunk = String::new();
    for index in 0..count {
        match before_lines[index] == after_lines[index] {
            true => {
                hunk.push(' ');
                hunk.push_str(before_lines[index]);
                hunk.push('\n');
            }
            false => {
                hunk.push('-');
                hunk.push_str(before_lines[index]);
                hunk.push('\n');
                hunk.push('+');
                hunk.push_str(after_lines[index]);
                hunk.push('\n');
            }
        }
    }
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,{count} +1,{count} @@
{hunk}"
    )
}

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, patch: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-guard-return-closed-pin-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"guard-return\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), patch)
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

fn finding_on_line<'a>(
    findings: &'a [Finding],
    line: usize,
    expression: &str,
) -> Result<&'a Finding, String> {
    let mut on_line = findings.iter().filter(|finding| {
        finding.probe.family == ProbeFamily::ReturnValue
            && finding.probe.location.line == line
            && finding.probe.expression.contains(expression)
    });
    match (on_line.next(), on_line.next()) {
        (Some(finding), None) => Ok(finding),
        other => Err(format!(
            "premise: exactly one return_value finding on line {line} for {expression}, got {other:?} in {findings:?}"
        )),
    }
}

const AFTER: &str = "pub fn discount(total: u32) -> u32 {
    if total > 500 {
        return 1;
    }
    if total >= 100 {
        return total / 5;
    }
    0
}";
const BEFORE: &str = "pub fn discount(total: u32) -> u32 {
    if total > 500 {
        return 1;
    }
    if total >= 100 {
        return total / 10;
    }
    0
}";

fn guard_return_class(before: &str, assertions: &str) -> Result<ExposureClass, String> {
    let findings =
        TempRepo::create(&lib(AFTER, assertions), &fn_diff(before, AFTER))?.findings()?;
    Ok(finding_on_line(&findings, 6, "total / 5")?.class.clone())
}

#[test]
fn an_exact_pin_whose_literal_takes_the_changed_guard_is_exposed() -> Result<(), String> {
    for assertions in [
        "assert_eq!(discount(200), 40);",
        // The old value pinned: the test fails on the new code instead.
        "assert_eq!(discount(200), 20);",
        "assert_eq!(40, discount(200));",
    ] {
        assert_eq!(
            guard_return_class(BEFORE, assertions)?,
            ExposureClass::Exposed,
            "{assertions}"
        );
    }
    Ok(())
}

#[test]
fn a_pin_that_cannot_tell_the_versions_apart_stays_below_exposed() -> Result<(), String> {
    for assertions in [
        // The literal skips the changed guard: an earlier guard or the tail
        // answers on both versions.
        "assert_eq!(discount(50), 0);",
        "assert_eq!(discount(600), 1);",
        // The pinned value is neither version's result.
        "assert_eq!(discount(200), 99);",
        // The argument is not a literal the subset reads.
        "let n = 200;\n        assert_eq!(discount(n), 20);",
        "assert_eq!(discount(100 + 100), 20);",
        // Not an exact pin.
        "assert!(discount(200) > 0);",
        "assert_ne!(discount(200), 0);",
    ] {
        assert_ne!(
            guard_return_class(BEFORE, assertions)?,
            ExposureClass::Exposed,
            "{assertions}"
        );
    }
    // A change both versions agree on at the pinned input: `100 / 5` and
    // `100 - 80` are both 20.
    let agreeing = BEFORE.replace("return total / 10;", "return total - 80;");
    assert_ne!(
        guard_return_class(&agreeing, "assert_eq!(discount(100), 20);")?,
        ExposureClass::Exposed
    );
    Ok(())
}

/// The diff also moves the guard condition, so the old owner is not the new
/// one with only the return line swapped back: `rate(300)` is 60 on both real
/// versions (the old tail, the new guard), and the pin cannot notice the
/// change.
#[test]
fn a_diff_that_also_changes_the_guard_keeps_the_pin_unproven() -> Result<(), String> {
    let after = "pub fn rate(t: u32) -> u32 {
    if t > 200 {
        return t / 5;
    }
    t * 2 / 10
}";
    let before = "pub fn rate(t: u32) -> u32 {
    if t > 500 {
        return t / 10;
    }
    t * 2 / 10
}";
    let findings = TempRepo::create(
        &lib(after, "assert_eq!(rate(300), 60);"),
        &fn_diff(before, after),
    )?
    .findings()?;
    let changed = finding_on_line(&findings, 3, "t / 5")?;
    assert_ne!(changed.class, ExposureClass::Exposed, "{changed:?}");
    // The same pin with only the return line changed tells the versions
    // apart (30 before, 60 after), so it is credited.
    let only_return = before.replace("t > 500", "t > 200");
    let findings = TempRepo::create(
        &lib(after, "assert_eq!(rate(300), 60);"),
        &fn_diff(&only_return, after),
    )?
    .findings()?;
    let changed = finding_on_line(&findings, 3, "t / 5")?;
    assert_eq!(changed.class, ExposureClass::Exposed, "{changed:?}");
    Ok(())
}

/// The subset reads `Some(..)` and `None` by spelling, so a file that binds
/// one of those names keeps the pin unproven (#14786 review). The same owner
/// without the rival binding is the control.
#[test]
fn a_file_that_rebinds_a_prelude_variant_keeps_the_pin_unproven() -> Result<(), String> {
    let owner = |rival: &str, divisor: &str| {
        format!(
            "pub fn pick(total: u32) -> Option<u32> {{
    if total >= 100 {{
        return Some(total / {divisor});
    }}
    None
}}
{rival}"
        )
    };
    for (rival, expected_exposed) in [
        ("", true),
        (
            "#[allow(non_upper_case_globals)]\nconst None: Option<u32> = Some(7);",
            false,
        ),
    ] {
        let after = owner(rival, "5");
        let before = owner(rival, "10");
        let findings = TempRepo::create(
            &lib(&after, "assert_eq!(pick(200), Some(40));"),
            &fn_diff(&before, &after),
        )?
        .findings()?;
        let changed = finding_on_line(&findings, 3, "total / 5")?;
        assert_eq!(
            changed.class == ExposureClass::Exposed,
            expected_exposed,
            "rival {rival:?}: {changed:?}"
        );
    }
    Ok(())
}

/// The subset checks arithmetic in each integer type's built-in range, so a
/// file that may rebind an integer type name keeps the pin unproven (#14786
/// review). Under `type u32 = u8;` both real versions overflow at `n * 4`, so
/// the pin passes on neither. The same owner without the alias is the control.
#[test]
fn a_file_that_may_rebind_an_integer_type_keeps_the_pin_unproven() -> Result<(), String> {
    let owner = |rival: &str, factor: &str| {
        format!(
            "pub fn f(n: u32) -> u32 {{
    if n >= 100 {{
        return n * {factor} / 10;
    }}
    0
}}
{rival}"
        )
    };
    for (rival, expected_exposed) in [
        ("", true),
        ("#[allow(non_camel_case_types)]\ntype u32 = u8;", false),
        ("pub use std::num::*;", false),
    ] {
        let after = owner(rival, "4");
        let before = owner(rival, "3");
        let findings = TempRepo::create(
            &lib(&after, "assert_eq!(f(100), 40);"),
            &fn_diff(&before, &after),
        )?
        .findings()?;
        let changed = finding_on_line(&findings, 3, "n * 4 / 10")?;
        assert_eq!(
            changed.class == ExposureClass::Exposed,
            expected_exposed,
            "rival {rival:?}: {changed:?}"
        );
    }
    Ok(())
}

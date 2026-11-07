//! Production-path red witnesses for #13037 tier values.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
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
            "ripr-early-return-scalar-credit-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"early-return-scalar\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
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

fn require_exposed(findings: &[Finding], line: usize, literal: &str) -> Result<(), String> {
    let found: Vec<_> = findings
        .iter()
        .filter(|finding| {
            finding.probe.location.line == line && finding.probe.expression.trim() == literal
        })
        .collect();
    assert!(
        !found.is_empty(),
        "the changed tier literal must produce a finding: {findings:?}"
    );
    assert!(
        found
            .iter()
            .any(|finding| finding.class == ExposureClass::Exposed),
        "an exact pin for the selected current tier should observe the change: {found:?}"
    );
    Ok(())
}

#[test]
fn current_pin_distinguishes_a_changed_tier_returned_as_a_binding() -> Result<(), String> {
    let before = "pub fn tier(weight: u32) -> u32 {\n    let base = if weight <= 10 {\n        9\n    } else {\n        20\n    };\n    base\n}";
    let after = before.replace("        9\n", "        6\n");
    let findings = TempRepo::create(
        &lib(&after, "assert_eq!(tier(4), 6);"),
        &fn_diff(before, &after),
    )?
    .findings()?;
    require_exposed(&findings, 3, "6")
}

#[test]
fn a_new_shipping_shaped_function_exposes_a_selected_tier() -> Result<(), String> {
    let function = "pub fn tier(weight: u32, order: u32, express: bool) -> u32 {\n    if order >= 100 && !express {\n        return 0;\n    }\n    let base = if weight <= 10 {\n        6\n    } else {\n        20\n    };\n    if express {\n        base * 2\n    } else {\n        base\n    }\n}";
    let count = function.lines().count();
    let added = function
        .lines()
        .map(|line| format!("+{line}\n"))
        .collect::<String>();
    let patch = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -0,0 +1,{count} @@\n{added}"
    );
    let findings = TempRepo::create(&lib(function, "assert_eq!(tier(4, 30, false), 6);"), &patch)?
        .findings()?;
    require_exposed(&findings, 6, "6")
}

#[test]
fn a_new_shipping_shape_with_a_named_limit_exposes_the_selected_tier() -> Result<(), String> {
    let constant = "pub const FREE_OVER: u32 = 100;";
    let function = "pub fn tier(weight: u32, order: u32, express: bool) -> u32 {
    if order >= FREE_OVER && !express {
        return 0;
    }
    let base = if weight <= 10 {
        6
    } else {
        20
    };
    if express {
        base * 2
    } else {
        base
    }
}";
    let count = function.lines().count() + 1;
    let added = function
        .lines()
        .map(|line| format!("+{line}\n"))
        .collect::<String>();
    let patch = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1,{count} @@\n {constant}\n{added}"
    );
    let source = format!("{constant}\n{function}");
    let findings = TempRepo::create(&lib(&source, "assert_eq!(tier(4, 30, false), 6);"), &patch)?
        .findings()?;
    require_exposed(&findings, 7, "6")
}

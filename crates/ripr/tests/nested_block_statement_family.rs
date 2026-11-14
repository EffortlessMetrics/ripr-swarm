//! #14192: a statement line inside a block nested in the returned
//! expression (a closure passed to the tail call, `Box::pin(async move {
//! .. })`) selects the families it would in a function body, not the outer
//! `return_value` / `error_path` shape. The nested block's own tail and a
//! method-chain link stay part of the returned value.

use ripr::{CheckInput, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
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
            "ripr-nested-block-statement-family-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"nested-block\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
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

fn families_on_line(findings: &[Finding], line: usize) -> Vec<ProbeFamily> {
    let mut families: Vec<ProbeFamily> = findings
        .iter()
        .filter(|finding| finding.probe.location.line == line)
        .map(|finding| finding.probe.family.clone())
        .collect();
    families.dedup();
    families
}

fn families(before: &str, after: &str, line: usize) -> Result<Vec<ProbeFamily>, String> {
    let helper = "\n\nfn with(f: impl FnOnce() -> u32) -> u32 {\n    f()\n}";
    let (before, after) = (format!("{before}{helper}"), format!("{after}{helper}"));
    let findings = TempRepo::create(&lib(&after, "assert_eq!(1, 1);"), &fn_diff(&before, &after))?
        .findings()?;
    let families = families_on_line(&findings, line);
    if families.is_empty() {
        return Err(format!("premise: a finding on line {line}: {findings:?}"));
    }
    Ok(families)
}

const CLOSURE: &str = "pub fn total(items: Vec<u32>) -> u32 {
    with(|| {
        let mut sum = 0;
        for item in &items {
            sum += item;
        }
        sum
    })
}";

const PINNED: &str =
    "pub fn total(items: Vec<u32>) -> std::pin::Pin<Box<dyn std::future::Future<Output = u32>>> {
    Box::pin(async move {
        let mut sum = 0;
        for item in items {
            sum += item;
        }
        sum
    })
}";

const IN_IF: &str = "pub fn total(items: Vec<u32>, flag: bool) -> u32 {
    if flag {
        let mut sum = 0;
        for item in items {
            sum += item;
        }
        sum
    } else {
        0
    }
}";

#[test]
fn a_statement_in_a_block_nested_in_the_tail_is_not_the_returned_value() -> Result<(), String> {
    for owner in [CLOSURE, PINNED] {
        let before = owner.replace("sum += item;", "sum += item * 2;");
        let nested = families(&before, owner, 5)?;
        assert!(
            !nested
                .iter()
                .any(|family| matches!(family, ProbeFamily::ReturnValue | ProbeFamily::ErrorPath)),
            "{owner}: {nested:?}"
        );
    }
    // The same statement in a plain `if` tail block reads the same families.
    let before = IN_IF.replace("sum += item;", "sum += item * 2;");
    let plain = families(&before, IN_IF, 5)?;
    let before = CLOSURE.replace("sum += item;", "sum += item * 2;");
    assert_eq!(families(&before, CLOSURE, 5)?, plain);
    Ok(())
}

#[test]
fn the_nested_tail_and_a_chain_link_stay_the_returned_value() -> Result<(), String> {
    // The closure block's own tail is the value the owner returns.
    let before = CLOSURE.replace("        sum\n    })", "        sum + 1\n    })");
    assert!(
        families(&before, CLOSURE, 7)?.contains(&ProbeFamily::ReturnValue),
        "nested tail"
    );
    let chain = "pub fn total(items: Vec<u32>) -> u32 {
    items
        .iter()
        .map(|item| item * 2)
        .sum()
}";
    let before = chain.replace("item * 2", "item * 3");
    assert!(
        families(&before, chain, 4)?.contains(&ProbeFamily::ReturnValue),
        "chain link"
    );
    Ok(())
}

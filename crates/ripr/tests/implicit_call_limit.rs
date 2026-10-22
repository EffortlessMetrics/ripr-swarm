//! Public-API controls for trait-impl methods the language calls implicitly
//! (RIPR-SPEC-0240, #9441).
//!
//! A test that reaches an operator impl, a `Drop` impl, an `Iterator` impl or
//! a `dyn Trait` method without spelling the method name reads as
//! `static_unknown` under `rust_implicit_call_unresolved`. Each control below
//! changes one thing about the shape and keeps the gap.

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
            "ripr-implicit-call-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"implicit-call\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
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

const LIMIT: &str = "rust_implicit_call_unresolved";

fn is_gap(finding: &Finding) -> bool {
    matches!(
        finding.class,
        ExposureClass::WeaklyExposed | ExposureClass::ReachableUnrevealed
    )
}

fn limited(finding: &Finding) -> bool {
    finding.static_limit_kind.map(|kind| kind.as_str()) == Some(LIMIT)
}

/// Some finding on the line carries the limit and none is still a gap.
fn withheld(lib: &str, diff: &str, line: usize, label: &str) -> Result<(), String> {
    let findings = findings_on(lib, diff, line)?;
    if findings.iter().any(is_gap) || !findings.iter().any(limited) {
        return Err(format!(
            "{label}: expected the implicit-call limit, got {:?}",
            findings
                .iter()
                .map(|f| (
                    f.probe.family.as_str(),
                    f.class.as_str(),
                    f.static_limit_kind
                ))
                .collect::<Vec<_>>()
        ));
    }
    Ok(())
}

/// No finding carries the limit and some finding is still a gap.
fn kept(lib: &str, diff: &str, line: usize, label: &str) -> Result<(), String> {
    let findings = findings_on(lib, diff, line)?;
    if findings.iter().any(limited) || !findings.iter().any(is_gap) {
        return Err(format!(
            "{label}: expected the gap kept, got {:?}",
            findings
                .iter()
                .map(|f| (f.class.as_str(), f.static_limit_kind))
                .collect::<Vec<_>>()
        ));
    }
    Ok(())
}

/// 1-based line of the first line of `lib` containing `needle`.
fn line_of(lib: &str, needle: &str) -> Result<usize, String> {
    lib.lines()
        .position(|line| line.contains(needle))
        .map(|at| at + 1)
        .ok_or_else(|| format!("premise: `{needle}` in the source"))
}

fn add_source(test_body: &str) -> String {
    format!(
        "use std::ops::Add;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Money(pub i64);

impl Add for Money {{
    type Output = Money;
    fn add(self, other: Money) -> Money {{
        Money(other.0 + self.0)
    }}
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn money_adds_through_the_operator() {{
        {test_body}
    }}
}}
"
    )
}

#[test]
fn an_operator_impl_reached_through_the_operator_is_withheld() -> Result<(), String> {
    let lib = add_source("assert_eq!(Money(2) + Money(3), Money(5));");
    let line = line_of(&lib, "Money(other.0 + self.0)")?;
    let edit = diff(
        line,
        "        Money(self.0 + other.0)",
        "        Money(other.0 + self.0)",
    );
    withheld(&lib, &edit, line, "Add through +")
}

#[test]
fn an_operator_impl_controls_keep_the_gap() -> Result<(), String> {
    let edit = |lib: &str| -> Result<(usize, String), String> {
        let line = line_of(lib, "Money(other.0 + self.0)")?;
        let edit = diff(
            line,
            "        Money(self.0 + other.0)",
            "        Money(other.0 + self.0)",
        );
        Ok((line, edit))
    };
    let controls = [
        // No test names `Money`, so nothing suggests the operator runs the impl.
        ("no test names Money", add_source("assert_eq!(1 + 2, 3);")),
        // A smoke test has no exact assertion.
        ("smoke test", add_source("let _ = Money(2) + Money(3);")),
        // The test spells `-`, which is not `Add`'s syntax.
        (
            "wrong operator",
            add_source("assert_eq!(Money(2).0 - Money(3).0, -1);"),
        ),
        // An ignored test never runs.
        (
            "ignored test",
            add_source("assert_eq!(Money(2) + Money(3), Money(5));")
                .replace("    #[test]\n", "    #[test]\n    #[ignore]\n"),
        ),
    ];
    for (label, lib) in controls {
        let (line, edit) = edit(&lib)?;
        kept(&lib, &edit, line, label)?;
    }
    Ok(())
}

#[test]
fn a_drop_impl_reached_at_scope_exit_is_withheld() -> Result<(), String> {
    let lib = "use std::cell::Cell;
use std::rc::Rc;

pub struct Guard(pub Rc<Cell<u32>>);

impl Drop for Guard {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 2 - 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropping_the_guard_counts_once() {
        let count = Rc::new(Cell::new(0));
        {
            let _guard = Guard(count.clone());
        }
        assert_eq!(count.get(), 1);
    }
}
";
    let line = line_of(lib, "self.0.set(")?;
    let edit = diff(
        line,
        "        self.0.set(self.0.get() + 1);",
        "        self.0.set(self.0.get() + 2 - 1);",
    );
    withheld(lib, &edit, line, "Drop at scope exit")
}

#[test]
fn an_iterator_impl_reached_through_collect_is_withheld() -> Result<(), String> {
    let lib = "pub struct Countdown(pub u32);

impl Iterator for Countdown {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        if self.0 == 0 {
            return None;
        }
        self.0 -= 1;
        Some(self.0 + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn countdown_collects_in_order() {
        assert_eq!(Countdown(3).collect::<Vec<_>>(), vec![3, 2, 1]);
    }
}
";
    let line = line_of(lib, "Some(self.0 + 1)")?;
    let edit = diff(line, "        Some(self.0)", "        Some(self.0 + 1)");
    withheld(lib, &edit, line, "Iterator through collect")
}

fn dispatch_source(test_body: &str) -> String {
    format!(
        "pub trait Shape {{
    fn area(&self) -> u32;
}}

pub struct Square(pub u32);

impl Shape for Square {{
    fn area(&self) -> u32 {{
        self.0 * self.0
    }}
}}

pub struct Tile(pub u32);

impl Shape for Tile {{
    fn area(&self) -> u32 {{
        self.0 * self.0
    }}
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn square_area_through_dyn() {{
        {test_body}
    }}
}}
"
    )
}

const DYN_SQUARE: &str = "let shapes: Vec<Box<dyn Shape>> = vec![Box::new(Square(3))];
        assert_eq!(shapes[0].area(), 9);";

#[test]
fn a_dyn_trait_method_called_on_an_untyped_receiver_is_withheld() -> Result<(), String> {
    let lib = dispatch_source(DYN_SQUARE);
    let line = line_of(&lib, "self.0 * self.0")?;
    let edit = diff(line, "        self.0 + self.0", "        self.0 * self.0");
    withheld(&lib, &edit, line, "dyn Shape::area")
}

#[test]
fn a_dispatch_shape_that_differs_keeps_its_class() -> Result<(), String> {
    let edit = |lib: &str| -> Result<(usize, String), String> {
        let line = line_of(lib, "self.0 * self.0")?;
        Ok((
            line,
            diff(line, "        self.0 + self.0", "        self.0 * self.0"),
        ))
    };
    // The test names the type but never calls a method named `area`.
    let lib = dispatch_source("let _square = Square(3);\n        assert_eq!(9, 9);");
    let (line, patch) = edit(&lib)?;
    kept(&lib, &patch, line, "never calls area")?;
    // The test calls `area` but never names `Square`.
    let lib = dispatch_source("let tile = Tile(3);\n        assert_eq!(tile.area(), 9);");
    let (line, patch) = edit(&lib)?;
    kept(&lib, &patch, line, "never names Square")
}

#[test]
fn a_test_local_shadow_trait_keeps_the_gap() -> Result<(), String> {
    // `Sample.render()` runs the test module's own `Render`, so the changed
    // production method never runs: a real gap, not an implicit call.
    let lib = "pub trait Render {
    fn render(&self) -> String;
}

impl Render for Sample {
    fn render(&self) -> String {
        String::from(\"\")
    }
}

pub struct Sample;

#[cfg(test)]
mod tests {
    use super::Sample;

    trait Render {
        fn render(&self) -> String;
    }

    impl Render for Sample {
        fn render(&self) -> String {
            String::new()
        }
    }

    #[test]
    fn render_is_empty() {
        assert_eq!(Sample.render(), String::from(\"\"));
    }
}
";
    let line = line_of(lib, "String::from(\"\")")?;
    let edit = diff(
        line,
        "        String::from(\"wrong\")",
        "        String::from(\"\")",
    );
    kept(lib, &edit, line, "shadow trait")
}

#[test]
fn a_workspace_trait_named_like_an_operator_trait_is_not_the_operator() -> Result<(), String> {
    // A local `trait Add` is not `std::ops::Add`, so `+` cannot call `sum`.
    let lib = "pub trait Add {
    fn sum(&self) -> i64;
}

pub struct Money(pub i64);

impl Add for Money {
    fn sum(&self) -> i64 {
        self.0 + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_and_arithmetic() {
        let _money = Money(2);
        assert_eq!(1 + 2, 3);
    }
}
";
    let line = line_of(lib, "self.0 + 1")?;
    let edit = diff(line, "        self.0 + 2", "        self.0 + 1");
    kept(lib, &edit, line, "local trait named Add")
}

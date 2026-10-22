//! Public-API controls for trait-impl methods no test names
//! (RIPR-SPEC-0240).

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
            "ripr-implicit-trait-call-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        let repo = Self { root };
        std::fs::write(
            repo.root.join("Cargo.toml"),
            "[package]\nname = \"implicit-trait-call\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(repo.root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(repo.root.join("diff.patch"), diff)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(repo)
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

const LIMIT: &str = "rust_implicit_trait_call_unresolved";

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
            "{label}: expected the framework limit, got {:?}",
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
fn keeps_gap(lib: &str, diff: &str, line: usize, label: &str) -> Result<(), String> {
    let findings = findings_on(lib, diff, line)?;
    if findings.iter().any(limited) || !findings.iter().any(is_gap) {
        return Err(format!(
            "{label}: expected the gap kept, got {:?}",
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

const ADD_OWNER: &str = "pub struct Money(pub i64);

impl std::ops::Add for Money {
    type Output = Money;
    fn add(self, other: Money) -> Money {
        Money(self.0 + other.0)
    }
}
";

fn add_source(test_body: &str) -> String {
    format!(
        "{ADD_OWNER}
#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn money_adds() {{
        {test_body}
    }}
}}
"
    )
}

const ADD_LINE: usize = 6;
const ADD_BEFORE: &str = "        Money(self.0 + other.0)";
const ADD_AFTER: &str = "        Money(other.0 + self.0)";

#[test]
fn an_operator_impl_reached_by_its_operator_is_withheld() -> Result<(), String> {
    let lib = add_source("assert_eq!((Money(2) + Money(3)).0, 5);");
    withheld(
        &lib,
        &diff(ADD_LINE, ADD_BEFORE, ADD_AFTER),
        ADD_LINE,
        "operator",
    )
}

#[test]
fn a_drop_impl_reached_by_scope_end_is_withheld() -> Result<(), String> {
    let lib = "use std::cell::Cell;
use std::rc::Rc;

pub struct Guard(pub Rc<Cell<u32>>);

impl Drop for Guard {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_counts_its_drop() {
        let counter = Rc::new(Cell::new(0));
        {
            let _guard = Guard(counter.clone());
        }
        assert_eq!(counter.get(), 1);
    }
}
";
    withheld(
        lib,
        &diff(
            8,
            "        self.0.set(self.0.get() + 1);",
            "        self.0.set(1 + self.0.get());",
        ),
        8,
        "drop",
    )
}

#[test]
fn a_trait_object_call_with_a_second_impl_is_withheld() -> Result<(), String> {
    let lib = "pub trait Shape {
    fn area(&self) -> u32;
}

pub struct Square(pub u32);
pub struct Tile(pub u32);

impl Shape for Square {
    fn area(&self) -> u32 {
        self.0 * self.0
    }
}

impl Shape for Tile {
    fn area(&self) -> u32 {
        self.0 * self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_area_through_dyn() {
        let shapes: Vec<Box<dyn Shape>> = vec![Box::new(Square(3))];
        assert_eq!(shapes[0].area(), 9);
    }
}
";
    withheld(
        lib,
        &diff(10, "        self.0 * self.0", "        self.0 * self.0 + 0"),
        10,
        "dyn",
    )
}

/// Control: a test that names the method is a direct call, so reach is
/// established and the weak oracle still reads as a gap.
#[test]
fn a_trait_impl_method_called_by_name_keeps_the_gap() -> Result<(), String> {
    let lib = add_source("assert!(Money(2).add(Money(3)).0 > 0);");
    keeps_gap(
        &lib,
        &diff(ADD_LINE, ADD_BEFORE, ADD_AFTER),
        ADD_LINE,
        "named call",
    )
}

/// Control: an inherent method has no implicit caller, so the limit does not
/// apply and the finding is never rewritten under it.
#[test]
fn an_inherent_method_is_never_withheld_by_this_limit() -> Result<(), String> {
    let lib = "pub struct Money(pub i64);

impl Money {
    pub fn plus(self, other: Money) -> Money {
        Money(self.0 + other.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_adds() {
        assert_eq!((Money(2).plus(Money(3))).0, 5);
    }
}
";
    let findings = findings_on(
        lib,
        &diff(
            5,
            "        Money(self.0 + other.0)",
            "        Money(other.0 + self.0)",
        ),
        5,
    )?;
    if findings.iter().any(limited) {
        return Err("inherent method carried the trait-impl limit".to_string());
    }
    Ok(())
}

/// Control: when no name-tied test pins an exact value, the weak oracle is the
/// finding and the gap stands.
#[test]
fn an_operator_impl_with_only_a_relational_check_keeps_the_gap() -> Result<(), String> {
    let lib = add_source("assert!((Money(2) + Money(3)).0 > 0);");
    keeps_gap(
        &lib,
        &diff(ADD_LINE, ADD_BEFORE, ADD_AFTER),
        ADD_LINE,
        "relational",
    )
}

/// Control: a turbofish call is still a call by name.
#[test]
fn a_turbofish_call_by_name_keeps_the_gap() -> Result<(), String> {
    let lib = "pub struct P;

pub trait Parse {
    fn parse_as<T: std::str::FromStr>(&self, text: &str) -> Option<T>;
}

impl Parse for P {
    fn parse_as<T: std::str::FromStr>(&self, text: &str) -> Option<T> {
        text.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_as_a_number() {
        assert_eq!(P.parse_as::<i32>(\"1\"), Some(1));
    }
}
";
    let diff = diff(
        9,
        "        text.parse().ok()",
        "        text.parse().ok().or(None)",
    );
    let findings = findings_on(lib, &diff, 9)?;
    if findings.iter().any(limited) {
        return Err("a named turbofish call was withheld".to_string());
    }
    Ok(())
}

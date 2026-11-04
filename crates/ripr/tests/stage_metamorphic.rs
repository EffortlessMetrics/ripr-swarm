//! Metamorphic properties of stage derivation feeding classification (#12917).
//!
//! `classify` is proved total and fail-closed over already-derived stage
//! values; the code that derives the reach, infect, propagate and reveal
//! stages from a diff and the test syntax is covered by example tests only.
//! A wrong stage promotes to a wrong `exposed`, which the classifier proofs
//! cannot see. This suite generates a changed comparison boundary plus a
//! sibling test file and drives the public library API
//! ([`ripr::check_workspace`]) over pairs of related inputs, asserting four
//! relations on the finding for the changed line:
//!
//! 1. adding an assertion that discriminates the changed value never lowers
//!    the class;
//! 2. removing the only discriminator never raises it (and drops it below
//!    `exposed`);
//! 3. adding, renaming or reformatting unrelated code is invariant;
//! 4. a test whose input the analysis cannot see yields an `*_unknown` class
//!    for the changed predicate, never `exposed`.
//!
//! Every case first asserts its fixture: the generated diff and test file
//! produce exactly one finding per probe family at the intended line and
//! expression. Only then are classes compared.
//!
//! The default case count is small so the required test run stays cheap. The
//! scheduled and manual `fuzz.yml` workflow raises it with `PROPTEST_CASES`
//! (400). Run longer locally with `PROPTEST_CASES=2000 cargo test -p ripr
//! --test stage_metamorphic`.

use proptest::prelude::*;
use proptest::test_runner::Config;
use ripr::domain::StageState;
use ripr::{
    CheckInput, CheckOutput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily,
    check_workspace,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const DEFAULT_CASES: u32 = 16;
const CRATE: &str = "stage_metamorphic_subject";

fn config() -> Config {
    Config {
        cases: std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_CASES),
        failure_persistence: Some(Box::new(proptest::test_runner::FileFailurePersistence::Off)),
        ..Config::default()
    }
}

/// Position on the class ladder. `no_static_path` and the `*_unknown`
/// classes are lumped at the bottom: they are all "nothing credited" and the
/// vocabulary does not order them against each other.
fn rank(class: &ExposureClass) -> u8 {
    match class {
        ExposureClass::Exposed => 4,
        ExposureClass::WeaklyExposed => 3,
        ExposureClass::ReachableUnrevealed => 2,
        ExposureClass::NoStaticPath
        | ExposureClass::InfectionUnknown
        | ExposureClass::PropagationUnknown
        | ExposureClass::StaticUnknown => 1,
    }
}

fn is_unknown(class: &ExposureClass) -> bool {
    matches!(
        class,
        ExposureClass::InfectionUnknown
            | ExposureClass::PropagationUnknown
            | ExposureClass::StaticUnknown
    )
}

/// One generated changed boundary. `flipped` selects `> T` becoming `>= T`
/// (false) or `>= T` becoming `> T` (true); either way `gate(T)` is the one
/// input whose answer the edit changes.
#[derive(Clone, Debug)]
struct Subject {
    threshold: u64,
    flipped: bool,
    /// Non-discriminating asserted values (never the threshold).
    others: Vec<(u64, u8)>,
    /// Assert style of the discriminator and where it sits among the others.
    disc_style: u8,
    disc_at: usize,
    /// Unrelated source lines inserted before the changed function.
    prelude: Vec<u8>,
}

impl Subject {
    fn old_op(&self) -> &'static str {
        if self.flipped { ">=" } else { ">" }
    }

    fn new_op(&self) -> &'static str {
        if self.flipped { ">" } else { ">=" }
    }

    fn holds(&self, value: u64) -> bool {
        if self.flipped {
            value > self.threshold
        } else {
            value >= self.threshold
        }
    }

    fn assertion(&self, value: u64, style: u8, spaced: bool) -> String {
        let expected = self.holds(value);
        let call = if spaced {
            format!("gate( {value} )")
        } else {
            format!("gate({value})")
        };
        match (style % 3, expected) {
            (0, _) => {
                if spaced {
                    format!("assert_eq!( {call} , {expected} );")
                } else {
                    format!("assert_eq!({call}, {expected});")
                }
            }
            (1, true) => format!("assert!({call});"),
            (1, false) => format!("assert!(!{call});"),
            _ => format!("assert_eq!({expected}, {call});"),
        }
    }

    fn expression(&self) -> String {
        format!("amount {} {}", self.new_op(), self.threshold)
    }

    /// Source with the changed function after `prelude` unrelated lines.
    /// Returns the source and the 1-based line of the changed expression.
    fn source(&self, changed: bool, unrelated_tail: bool) -> (String, usize) {
        let op = if changed {
            self.new_op()
        } else {
            self.old_op()
        };
        let mut lines: Vec<String> = Vec::new();
        for pick in &self.prelude {
            match pick % 4 {
                0 => lines.push("// unrelated note".to_string()),
                1 => lines.push(String::new()),
                2 => lines.push("pub const UNRELATED_LIMIT: u8 = 3;".to_string()),
                _ => lines.push("pub fn untouched(n: u8) -> u8 { n }".to_string()),
            }
        }
        lines.push("pub fn gate(amount: u64) -> bool {".to_string());
        let line = lines.len() + 1;
        lines.push(format!("    amount {op} {}", self.threshold));
        lines.push("}".to_string());
        if unrelated_tail {
            lines.push(String::new());
            lines.push("pub fn spare(n: u8) -> u8 {".to_string());
            lines.push("    n + 1".to_string());
            lines.push("}".to_string());
        }
        (lines.join("\n") + "\n", line)
    }

    /// Whole-file diff: the changed line replaced, everything else context.
    fn diff(&self, unrelated_tail: bool) -> (String, usize) {
        let (old, _) = self.source(false, unrelated_tail);
        let (new, line) = self.source(true, unrelated_tail);
        let old_lines: Vec<&str> = old.lines().collect();
        let new_lines: Vec<&str> = new.lines().collect();
        let mut body = String::new();
        for (index, (before, after)) in old_lines.iter().zip(&new_lines).enumerate() {
            if index + 1 == line {
                body.push_str(&format!("-{before}\n+{after}\n"));
            } else {
                body.push_str(&format!(" {before}\n"));
            }
        }
        let count = old_lines.len();
        (
            format!(
                "diff --git a/src/lib.rs b/src/lib.rs\nindex 1111111..2222222 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,{count} +1,{count} @@\n{body}"
            ),
            line,
        )
    }

    /// The test file, with or without the discriminator, plain or reformatted
    /// with unrelated additions.
    fn test_file(&self, with_discriminator: bool, noisy: bool) -> String {
        let mut asserts: Vec<String> = self
            .others
            .iter()
            .map(|(value, style)| self.assertion(*value, *style, noisy))
            .collect();
        if with_discriminator {
            let at = self.disc_at.min(asserts.len());
            asserts.insert(at, self.assertion(self.threshold, self.disc_style, noisy));
        }
        render_test(&asserts, noisy)
    }
}

fn render_test(asserts: &[String], noisy: bool) -> String {
    let body: String = asserts.iter().map(|line| format!("    {line}\n")).collect();
    if noisy {
        format!(
            "// Reformatted and padded with unrelated items.\nuse {CRATE}::*;\n\nfn unrelated_helper() -> u8 {{\n    3\n}}\n\n\n#[test]\nfn renamed_case() {{\n\n{body}\n}}\n"
        )
    } else {
        format!("use {CRATE}::*;\n\n#[test]\nfn case() {{\n{body}}}\n")
    }
}

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(source: &str, test_source: &str, diff: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-stage-metamorphic-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let repo = Self { root };
        for (name, content) in [
            (
                "Cargo.toml",
                "[package]\nname = \"stage-metamorphic-subject\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
            ),
            ("src/lib.rs", source),
            ("tests/case.rs", test_source),
            ("diff.patch", diff),
        ] {
            let path = repo.root.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("create {} failed: {error}", parent.display()))?;
            }
            std::fs::write(&path, content)
                .map_err(|error| format!("write {} failed: {error}", path.display()))?;
        }
        Ok(repo)
    }

    fn check(&self) -> Result<CheckOutput, String> {
        check_workspace(CheckInput {
            root: self.root.clone(),
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// What a relation compares for one probe family.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Judgment {
    class: ExposureClass,
    reach: StageState,
    infect: StageState,
    propagate: StageState,
}

struct Observed {
    predicate: Judgment,
    return_value: Judgment,
}

fn judgment(finding: &Finding) -> Judgment {
    Judgment {
        class: finding.class.clone(),
        reach: finding.ripr.reach.state.clone(),
        infect: finding.ripr.infect.state.clone(),
        propagate: finding.ripr.propagate.state.clone(),
    }
}

/// Runs the analysis and asserts the fixture produced exactly one finding per
/// probe family at the changed line, over the changed expression.
fn observe(
    subject: &Subject,
    test_source: &str,
    unrelated_tail: bool,
) -> Result<Observed, TestCaseError> {
    observe_text(subject, test_source, unrelated_tail).map_err(TestCaseError::fail)
}

fn observe_text(
    subject: &Subject,
    test_source: &str,
    unrelated_tail: bool,
) -> Result<Observed, String> {
    let (source, _) = subject.source(true, unrelated_tail);
    let (diff, line) = subject.diff(unrelated_tail);
    let repo = TempRepo::create(&source, test_source, &diff)?;
    let output = repo.check()?;
    let expression = subject.expression();
    let pick = |family: ProbeFamily| -> Result<Judgment, String> {
        let mut found = output.findings.iter().filter(|finding| {
            finding.probe.family == family && finding.probe.location.line == line
        });
        let finding = found
            .next()
            .ok_or_else(|| format!("fixture produced no {family:?} finding at line {line}"))?;
        if found.next().is_some() {
            return Err(format!("fixture produced duplicate {family:?} findings"));
        }
        if finding.probe.expression.trim() != expression {
            return Err(format!(
                "{family:?} finding is `{}`, not the changed `{expression}`",
                finding.probe.expression
            ));
        }
        Ok(judgment(finding))
    };
    Ok(Observed {
        predicate: pick(ProbeFamily::Predicate)?,
        return_value: pick(ProbeFamily::ReturnValue)?,
    })
}

fn subject() -> impl Strategy<Value = Subject> {
    (
        2u64..500,
        any::<bool>(),
        prop::collection::vec((0u64..1_000, 0u8..3), 0..4),
        0u8..3,
        0usize..4,
        prop::collection::vec(0u8..4, 0..4),
    )
        .prop_map(
            |(threshold, flipped, others, disc_style, disc_at, prelude)| Subject {
                threshold,
                flipped,
                others: others
                    .into_iter()
                    .map(|(value, style)| {
                        // The threshold itself is the discriminator.
                        (if value == threshold { value + 1 } else { value }, style)
                    })
                    .collect(),
                disc_style,
                disc_at,
                prelude,
            },
        )
}

fn lowered(
    relation: &str,
    before: &Judgment,
    after: &Judgment,
    subject: &Subject,
    family: ProbeFamily,
) -> Result<(), TestCaseError> {
    if rank(&after.class) < rank(&before.class) {
        return Err(TestCaseError::fail(format!(
            "{relation}: {family:?} fell from {:?} to {:?} for {subject:?}",
            before.class, after.class
        )));
    }
    Ok(())
}

proptest! {
    #![proptest_config(config())]

    /// Relation 1: adding the discriminating assertion never lowers the class,
    /// and for a boundary change it is what makes the finding `exposed`.
    #[test]
    fn adding_a_discriminator_never_lowers_the_class(subject in subject()) {
        let without = observe(&subject, &subject.test_file(false, false), false)?;
        let with = observe(&subject, &subject.test_file(true, false), false)?;
        lowered("add discriminator", &without.predicate, &with.predicate, &subject, ProbeFamily::Predicate)?;
        lowered("add discriminator", &without.return_value, &with.return_value, &subject, ProbeFamily::ReturnValue)?;
        prop_assert_eq!(&with.predicate.class, &ExposureClass::Exposed, "{:?}", subject);
        prop_assert_eq!(&with.return_value.class, &ExposureClass::Exposed, "{:?}", subject);
    }

    /// Relation 2: removing the only discriminator never raises the class, and
    /// what remains is not `exposed`.
    #[test]
    fn removing_the_only_discriminator_never_raises_the_class(subject in subject()) {
        let with = observe(&subject, &subject.test_file(true, false), false)?;
        let without = observe(&subject, &subject.test_file(false, false), false)?;
        lowered("remove discriminator", &without.predicate, &with.predicate, &subject, ProbeFamily::Predicate)?;
        lowered("remove discriminator", &without.return_value, &with.return_value, &subject, ProbeFamily::ReturnValue)?;
        prop_assert_ne!(&without.predicate.class, &ExposureClass::Exposed, "{:?}", subject);
        prop_assert_ne!(&without.return_value.class, &ExposureClass::Exposed, "{:?}", subject);
    }

    /// Relation 3: reformatting the tests and adding, renaming or padding
    /// unrelated items changes no judgment, with or without a discriminator.
    #[test]
    fn unrelated_edits_are_invariant(subject in subject(), with_discriminator in any::<bool>()) {
        let plain = observe(&subject, &subject.test_file(with_discriminator, false), false)?;
        let noisy = observe(&subject, &subject.test_file(with_discriminator, true), true)?;
        prop_assert_eq!(&plain.predicate, &noisy.predicate, "{:?}", subject);
        prop_assert_eq!(&plain.return_value, &noisy.return_value, "{:?}", subject);
    }

    /// Relation 4: when the only checks feed `gate` a value the analysis
    /// cannot see, the changed predicate is `*_unknown`, never `exposed`. With
    /// no test at all nothing is exposed either.
    #[test]
    fn unseen_inputs_are_unknown_never_exposed(subject in subject(), form in 0u8..3, expected in any::<bool>()) {
        let setup = match form {
            0 => "let v = std::env::var(\"STAGE_INPUT\").unwrap().len() as u64;",
            1 => "let v = read_input();",
            _ => "let v = std::env::args().count() as u64;",
        };
        let helper = if form == 1 { "fn read_input() -> u64 {\n    std::env::args().count() as u64\n}\n\n" } else { "" };
        let test = format!(
            "use {CRATE}::*;\n\n{helper}#[test]\nfn unseen() {{\n    {setup}\n    assert_eq!(gate(v), {expected});\n}}\n"
        );
        let unseen = observe(&subject, &test, false)?;
        prop_assert!(is_unknown(&unseen.predicate.class), "{:?}: {:?}", subject, unseen.predicate);

        let none = observe(&subject, &format!("use {CRATE}::*;\n"), false)?;
        prop_assert_ne!(&none.predicate.class, &ExposureClass::Exposed, "{:?}", subject);
        prop_assert_ne!(&none.return_value.class, &ExposureClass::Exposed, "{:?}", subject);
    }
}

proptest! {
    #![proptest_config(config())]

    /// Relation 4 for the ReturnValue family. The return-value probe reads
    /// `exposed` for an input the analysis cannot see while the predicate probe
    /// on the same line reads `infection_unknown` (#13040). Kept as an ignored
    /// witness so the gap stays visible; run with `--ignored` to reproduce.
    #[test]
    #[ignore = "known gap #13040: return_value probe reads exposed for an unseen input"]
    fn unseen_inputs_are_never_exposed_for_return_value(subject in subject(), expected in any::<bool>()) {
        let test = format!(
            "use {CRATE}::*;\n\n#[test]\nfn unseen() {{\n    let v = std::env::args().count() as u64;\n    assert_eq!(gate(v), {expected});\n}}\n"
        );
        let unseen = observe(&subject, &test, false)?;
        prop_assert_ne!(&unseen.return_value.class, &ExposureClass::Exposed, "{:?}", subject);
    }
}

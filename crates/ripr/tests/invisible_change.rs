//! End-to-end witness for a changed return expression that no related test
//! input can tell apart from its baseline (#8659): an exact assertion on the
//! owner is not a discriminator for a change its literal inputs never reach.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// `lib` is the changed `src/lib.rs` whose line 2 replaced `old`; `test` is
/// the integration test.
fn findings(lib: &str, old: &str, test: &str) -> Result<Vec<Finding>, String> {
    findings_at(lib, 2, old, test)
}

/// [`findings`] for a change on 1-based `line` of `lib`.
fn findings_at(lib: &str, line: usize, old: &str, test: &str) -> Result<Vec<Finding>, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let repo = TempRepo {
        root: std::env::temp_dir().join(format!(
            "ripr-invisible-change-{}-{sequence}",
            std::process::id()
        )),
    };
    let root = &repo.root;
    std::fs::create_dir_all(root.join("src")).map_err(|error| format!("src dir: {error}"))?;
    std::fs::create_dir_all(root.join("tests")).map_err(|error| format!("tests dir: {error}"))?;
    let write = |path: &str, text: &str| {
        std::fs::write(root.join(path), text).map_err(|error| format!("write {path}: {error}"))
    };
    write(
        "Cargo.toml",
        "[package]\nname = \"invisible\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )?;
    write("src/lib.rs", lib)?;
    write("tests/it.rs", test)?;
    let lines = lib.lines().collect::<Vec<_>>();
    let new = line
        .checked_sub(1)
        .and_then(|index| lines.get(index))
        .ok_or_else(|| format!("lib has a line {line}"))?;
    let context = |range: std::ops::Range<usize>| {
        range
            .filter_map(|index| lines.get(index))
            .map(|line| format!(" {line}\n"))
            .collect::<String>()
    };
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\nindex 0000000..1111111 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,{count} +1,{count} @@\n{}-{old}\n+{new}\n{}",
        context(0..line - 1),
        context(line..lines.len()),
        count = lines.len()
    );
    write("diff.patch", &diff)?;
    let output = check_workspace(CheckInput {
        root: root.clone(),
        base: None,
        diff_file: Some(root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })
    .map_err(|error| format!("check failed: {error}"))?;
    Ok(output
        .findings
        .into_iter()
        .filter(|finding| finding.probe.location.line == line)
        .collect())
}

fn return_value(findings: &[Finding]) -> Result<&Finding, String> {
    of_family(findings, "return_value")
}

fn of_family<'a>(findings: &'a [Finding], family: &str) -> Result<&'a Finding, String> {
    findings
        .iter()
        .find(|finding| finding.probe.family.as_str() == family)
        .ok_or_else(|| format!("no {family} finding in {findings:?}"))
}

/// Whether the infection stage says the related tests' inputs agree on both
/// versions, in either wording (#8659, #15272).
fn reads_same_value(finding: &Finding) -> bool {
    finding
        .ripr
        .infect
        .summary
        .contains("evaluate to the same value")
}

const POW: &str = "pub fn backoff_ms(attempt: u32, base: u64) -> u64 {\n    base * 3u64.pow(attempt)\n}\n\npub fn tail() {}\n";
const POW_OLD: &str = "    base * 2u64.pow(attempt)";
const MIN: &str =
    "pub fn capped(attempt: u32) -> u32 {\n    attempt.min(6)\n}\n\npub fn tail() {}\n";
const MIN_OLD: &str = "    attempt.min(5)";

#[test]
fn an_input_where_pow_bases_agree_does_not_expose_the_change() -> Result<(), String> {
    // `3^0 == 2^0`: the only asserted input cannot tell the versions apart.
    let findings = findings(
        POW,
        POW_OLD,
        "use invisible::backoff_ms;\n\n#[test]\nfn first() {\n    assert_eq!(backoff_ms(0, 100), 100);\n}\n",
    )?;
    let finding = return_value(&findings)?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert!(
        finding
            .ripr
            .infect
            .summary
            .contains("evaluate to the same value"),
        "{finding:?}"
    );
    Ok(())
}

#[test]
fn an_input_below_the_cap_does_not_expose_a_raised_cap() -> Result<(), String> {
    let findings = findings(
        MIN,
        MIN_OLD,
        "use invisible::capped;\n\n#[test]\nfn low() {\n    assert_eq!(capped(3), 3);\n}\n",
    )?;
    assert_ne!(return_value(&findings)?.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_second_input_that_separates_the_versions_keeps_the_credit() -> Result<(), String> {
    // Control: `backoff_ms(1, 100)` is 300 changed and 200 before.
    let findings = findings(
        POW,
        POW_OLD,
        "use invisible::backoff_ms;\n\n#[test]\nfn first() {\n    assert_eq!(backoff_ms(0, 100), 100);\n}\n\n#[test]\nfn second() {\n    assert_eq!(backoff_ms(1, 100), 300);\n}\n",
    )?;
    let finding = return_value(&findings)?;
    assert!(
        !finding
            .ripr
            .infect
            .summary
            .contains("evaluate to the same value"),
        "{finding:?}"
    );
    Ok(())
}

#[test]
fn an_input_that_hits_the_cap_keeps_the_credit() -> Result<(), String> {
    // Control: `capped(9)` is 6 changed and 5 before.
    let findings = findings(
        MIN,
        MIN_OLD,
        "use invisible::capped;\n\n#[test]\nfn high() {\n    assert_eq!(capped(9), 6);\n}\n",
    )?;
    assert!(
        !return_value(&findings)?
            .ripr
            .infect
            .summary
            .contains("evaluate to the same value")
    );
    Ok(())
}

#[test]
fn an_owner_call_in_the_expected_operand_keeps_the_credit() -> Result<(), String> {
    // Control: `capped(3)` agrees, but `capped(9) - 3` is 3 changed and 2 before.
    let findings = findings(
        MIN,
        MIN_OLD,
        "use invisible::capped;\n\n#[test]\nfn wide() {\n    assert_eq!(capped(3), capped(9) - 3);\n}\n",
    )?;
    assert!(
        !return_value(&findings)?
            .ripr
            .infect
            .summary
            .contains("evaluate to the same value")
    );
    Ok(())
}

#[test]
fn a_computed_argument_keeps_the_credit() -> Result<(), String> {
    // Control: `next()` is not a literal, so nothing is folded or claimed.
    let findings = findings(
        MIN,
        MIN_OLD,
        "use invisible::capped;\n\nfn next() -> u32 {\n    3\n}\n\n#[test]\nfn computed() {\n    assert_eq!(capped(next()), 3);\n}\n",
    )?;
    assert!(
        !return_value(&findings)?
            .ripr
            .infect
            .summary
            .contains("evaluate to the same value")
    );
    Ok(())
}

#[test]
fn a_relational_sibling_that_holds_on_both_versions_does_not_expose_the_change()
-> Result<(), String> {
    // `backoff_ms(3, 100) > backoff_ms(1, 100)` is true before (800 > 200) and after (2700 > 300).
    let findings = findings(
        POW,
        POW_OLD,
        "use invisible::backoff_ms;\n\n#[test]\nfn first() {\n    assert_eq!(backoff_ms(0, 100), 100);\n}\n\n#[test]\nfn grows() {\n    assert!(backoff_ms(3, 100) > backoff_ms(1, 100));\n}\n",
    )?;
    let finding = return_value(&findings)?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    // The asserted inputs do evaluate differently (800 vs 2700), so the
    // summary names the unchanged comparison truth, not a missing input
    // difference (#10916).
    assert!(
        finding.ripr.infect.summary.contains("same truth value"),
        "{finding:?}"
    );
    assert!(
        !finding
            .ripr
            .infect
            .summary
            .contains("evaluate to the same value"),
        "{finding:?}"
    );
    Ok(())
}

#[test]
fn a_rebound_assert_macro_keeps_the_credit() -> Result<(), String> {
    // Control: the written relational tokens fold the same on both versions,
    // but the file rebinds `assert!` to an exact-value check the baseline
    // fails (`2700` after vs `800` before). Ripr reads macro names, not
    // expansions, so both invisible readings must decline and today's
    // non-relational reading keeps the finding (#10920 review).
    let test = "use invisible::backoff_ms;\n\nmacro_rules! assert {\n    ($f:ident($($a:tt)*) > $g:ident($($b:tt)*)) => {\n        std::assert_eq!($f($($a)*), 2700)\n    };\n}\n\n#[test]\nfn first() {\n    assert_eq!(backoff_ms(0, 100), 100);\n}\n\n#[test]\nfn grows() {\n    assert!(backoff_ms(3, 100) > backoff_ms(1, 100));\n}\n";
    let findings = findings(POW, POW_OLD, test)?;
    let finding = return_value(&findings)?;
    assert!(
        !finding
            .ripr
            .infect
            .summary
            .contains("evaluate to the same value"),
        "{finding:?}"
    );
    assert!(
        !finding.ripr.infect.summary.contains("same truth value"),
        "{finding:?}"
    );
    Ok(())
}

#[test]
fn a_relational_assertion_that_flips_between_versions_keeps_the_credit() -> Result<(), String> {
    // Control: `backoff_ms(3, 100) > 1000` is true after (2700) and false before (800).
    let findings = findings(
        POW,
        POW_OLD,
        "use invisible::backoff_ms;\n\n#[test]\nfn first() {\n    assert_eq!(backoff_ms(0, 100), 100);\n}\n\n#[test]\nfn big() {\n    assert!(backoff_ms(3, 100) > 1000);\n}\n",
    )?;
    let finding = return_value(&findings)?;
    assert!(
        !finding
            .ripr
            .infect
            .summary
            .contains("evaluate to the same value"),
        "{finding:?}"
    );
    // The flip is a discriminator for the relational reading too, so the
    // truth-value summary must not appear either (#10920 review).
    assert!(
        !finding.ripr.infect.summary.contains("same truth value"),
        "{finding:?}"
    );
    Ok(())
}

const LITERAL: &str = "impl Builder {\n    pub fn build(self) -> Req { Req { url: self.url, timeout_ms: self.timeout_ms.unwrap_or(3_000) } }\n\n    pub fn new(url: &str) -> Self { Builder { url: url.to_string(), timeout_ms: None } }\n\n    pub fn timeout_ms(mut self, t: u64) -> Self { self.timeout_ms = Some(t); self }\n}\n\npub struct Builder { url: String, timeout_ms: Option<u64> }\n\n#[derive(Debug)]\npub struct Req { pub url: String, pub timeout_ms: u64 }\n";
const LITERAL_OLD: &str = "    pub fn build(self) -> Req { Req { url: self.url, timeout_ms: self.timeout_ms.unwrap_or(30_000) } }";

#[test]
fn an_assertion_on_an_untouched_field_does_not_expose_a_changed_default() -> Result<(), String> {
    // #10916 case 2: only `timeout_ms` moved. The test reads `url`, and reads
    // `timeout_ms` only after setting it, so nothing the diff changed reaches
    // an assertion.
    let findings = findings(
        LITERAL,
        LITERAL_OLD,
        "use invisible::Builder;\n\n#[test]\nfn url_kept() {\n    assert_eq!(Builder::new(\"http://x\").build().url, \"http://x\");\n}\n\n#[test]\nfn explicit() {\n    let r = Builder::new(\"u\").timeout_ms(500).build();\n    assert_eq!(r.timeout_ms, 500);\n}\n",
    )?;
    let finding = return_value(&findings)?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn an_assertion_that_reads_the_changed_default_keeps_the_credit() -> Result<(), String> {
    // Control: the default is read and pinned to the new value.
    let findings = findings(
        LITERAL,
        LITERAL_OLD,
        "use invisible::Builder;\n\n#[test]\nfn url_kept() {\n    assert_eq!(Builder::new(\"http://x\").build().url, \"http://x\");\n}\n\n#[test]\nfn default() {\n    assert_eq!(Builder::new(\"u\").build().timeout_ms, 3_000);\n}\n",
    )?;
    let finding = return_value(&findings)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn a_path_qualified_call_at_an_agreeing_input_does_not_expose_the_change() -> Result<(), String> {
    // #15414: `invisible::capped(3)` is the same call as `capped(3)`; both
    // versions return 3, so the qualified spelling must not earn credit.
    let findings = findings(
        MIN,
        MIN_OLD,
        "#[test]\nfn low() {\n    assert_eq!(invisible::capped(3), 3);\n}\n",
    )?;
    let finding = return_value(&findings)?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert!(
        finding
            .ripr
            .infect
            .summary
            .contains("evaluate to the same value"),
        "{finding:?}"
    );
    Ok(())
}

#[test]
fn a_crate_rooted_qualified_call_at_an_agreeing_input_does_not_expose_the_change()
-> Result<(), String> {
    let findings = findings(
        POW,
        POW_OLD,
        "#[test]\nfn first() {\n    assert_eq!(::invisible::backoff_ms(0, 100), 100);\n}\n",
    )?;
    let finding = return_value(&findings)?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert!(
        finding
            .ripr
            .infect
            .summary
            .contains("evaluate to the same value"),
        "{finding:?}"
    );
    Ok(())
}

#[test]
fn a_path_qualified_call_at_a_separating_input_keeps_the_credit() -> Result<(), String> {
    // Control: `invisible::capped(9)` is 6 changed and 5 before.
    let findings = findings(
        MIN,
        MIN_OLD,
        "#[test]\nfn high() {\n    assert_eq!(invisible::capped(9), 6);\n}\n",
    )?;
    let finding = return_value(&findings)?;
    assert!(
        !finding
            .ripr
            .infect
            .summary
            .contains("evaluate to the same value"),
        "{finding:?}"
    );
    Ok(())
}

#[test]
fn a_method_call_through_a_path_is_not_read_as_the_owner() -> Result<(), String> {
    // Control: `invisible::other().capped(3)` is not a plain path to the
    // owner, so the qualified reading declines and nothing is claimed.
    let lib = "pub fn capped(attempt: u32) -> u32 {\n    attempt.min(6)\n}\n\npub struct S;\npub fn other() -> S { S }\nimpl S { pub fn capped(&self, a: u32) -> u32 { a + 100 } }\n";
    let findings = findings(
        lib,
        MIN_OLD,
        "#[test]\nfn low() {\n    assert_eq!(invisible::other().capped(3), 103);\n}\n",
    )?;
    assert!(!findings.is_empty(), "the changed owner should be analyzed");
    for finding in &findings {
        assert!(
            !finding
                .ripr
                .infect
                .summary
                .contains("evaluate to the same value"),
            "{finding:?}"
        );
    }
    Ok(())
}

// #15272: inputs that cannot discriminate (a zero receiver field, equal
// arguments, a tuple both versions build alike) must not read `exposed`.

const DOUBLE: &str = "pub struct Acc {\n    pub base: u64,\n}\n\nimpl Acc {\n    pub fn double(&self) -> u64 {\n        self.base * 3\n    }\n}\n";
const DOUBLE_LINE: usize = 7;
const DOUBLE_OLD: &str = "        self.base * 2";
const DOUBLE_ONE_LINE: &str = "impl Acc {\n    pub fn double(&self) -> u64 { self.base * 3 }\n}\n\npub struct Acc { pub base: u64 }\n";
const DOUBLE_ONE_LINE_OLD: &str = "    pub fn double(&self) -> u64 { self.base * 2 }";

fn double_test(body: &str) -> String {
    format!("use invisible::Acc;\n\n#[test]\nfn pinned() {{\n{body}\n}}\n")
}

#[test]
fn a_zero_receiver_field_does_not_expose_a_changed_multiplier() -> Result<(), String> {
    // `0 * 3 == 0 * 2`: the literal receiver cannot tell the versions apart.
    let findings = findings_at(
        DOUBLE,
        DOUBLE_LINE,
        DOUBLE_OLD,
        &double_test("    assert_eq!(Acc { base: 0 }.double(), 0);"),
    )?;
    let finding = return_value(&findings)?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert!(reads_same_value(finding), "{finding:?}");
    Ok(())
}

#[test]
fn a_zero_receiver_field_does_not_expose_a_one_line_method() -> Result<(), String> {
    let findings = findings(
        DOUBLE_ONE_LINE,
        DOUBLE_ONE_LINE_OLD,
        &double_test("    assert_eq!(Acc { base: 0 }.double(), 0);"),
    )?;
    let finding = return_value(&findings)?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert!(reads_same_value(finding), "{finding:?}");
    Ok(())
}

#[test]
fn a_nonzero_receiver_field_keeps_the_credit() -> Result<(), String> {
    // Control: `2 * 3` is 6 changed and 4 before, on both method layouts.
    let test = double_test("    assert_eq!(Acc { base: 2 }.double(), 6);");
    for findings in [
        findings_at(DOUBLE, DOUBLE_LINE, DOUBLE_OLD, &test)?,
        findings(DOUBLE_ONE_LINE, DOUBLE_ONE_LINE_OLD, &test)?,
    ] {
        let finding = return_value(&findings)?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
        assert!(!reads_same_value(finding), "{finding:?}");
    }
    Ok(())
}

#[test]
fn a_receiver_the_assertion_does_not_spell_keeps_the_credit() -> Result<(), String> {
    // Controls: the receiver's fields are read only from a record literal of
    // the owner's self type. A binding, a constructor, a functional update
    // and a field shorthand each hold a value the assertion does not show.
    for body in [
        "    let acc = Acc { base: 0 };\n    assert_eq!(acc.double(), 0);",
        "    assert_eq!(zero().double(), 0);\n}\n\nfn zero() -> Acc {\n    Acc { base: 0 }",
        "    assert_eq!(Acc { ..Acc { base: 0 } }.double(), 0);",
        "    let base = 0;\n    assert_eq!(Acc { base }.double(), 0);",
    ] {
        for findings in [
            findings_at(DOUBLE, DOUBLE_LINE, DOUBLE_OLD, &double_test(body))?,
            findings(DOUBLE_ONE_LINE, DOUBLE_ONE_LINE_OLD, &double_test(body))?,
        ] {
            let finding = return_value(&findings)?;
            assert!(!reads_same_value(finding), "{body}: {finding:?}");
        }
    }
    Ok(())
}

#[test]
fn a_record_literal_of_another_type_is_not_read_as_the_receiver() -> Result<(), String> {
    // Control: `Twin { base: 0 }.double()` calls another method. Reading it
    // against `Acc::double` would claim an input the owner never saw.
    let lib = "pub struct Acc {\n    pub base: u64,\n}\n\nimpl Acc {\n    pub fn double(&self) -> u64 {\n        self.base * 3\n    }\n}\n\npub struct Twin {\n    pub base: u64,\n}\n\nimpl Twin {\n    pub fn double(&self) -> u64 {\n        self.base + 7\n    }\n}\n";
    let findings = findings_at(
        lib,
        DOUBLE_LINE,
        DOUBLE_OLD,
        "use invisible::Twin;\n\n#[test]\nfn twin() {\n    assert_eq!(Twin { base: 0 }.double(), 7);\n}\n",
    )?;
    for finding in &findings {
        assert!(!reads_same_value(finding), "{finding:?}");
    }
    Ok(())
}

const LARGEST: &str = "pub fn largest<T: PartialOrd>(a: T, b: T) -> T {\n    if a > b { b } else { a }\n}\n\npub fn tail() {}\n";
const LARGEST_OLD: &str = "    if a > b { a } else { b }";

#[test]
fn equal_arguments_do_not_expose_swapped_arms() -> Result<(), String> {
    // `largest(2, 2)` returns 2 from either arm on either version, though
    // the inputs sit exactly on the `a > b` boundary.
    let findings = findings(
        LARGEST,
        LARGEST_OLD,
        "use invisible::largest;\n\n#[test]\nfn equal() {\n    assert_eq!(largest(2, 2), 2);\n}\n",
    )?;
    assert!(!findings.is_empty(), "the changed owner should be analyzed");
    for finding in &findings {
        assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
        assert!(reads_same_value(finding), "{finding:?}");
    }
    Ok(())
}

#[test]
fn unequal_arguments_are_not_read_as_agreeing() -> Result<(), String> {
    // Control: `largest(1, 2)` is 1 changed and 2 before.
    let findings = findings(
        LARGEST,
        LARGEST_OLD,
        "use invisible::largest;\n\n#[test]\nfn equal() {\n    assert_eq!(largest(2, 2), 2);\n}\n\n#[test]\nfn unequal() {\n    assert_eq!(largest(1, 2), 1);\n}\n",
    )?;
    assert!(!findings.is_empty(), "the changed owner should be analyzed");
    for finding in &findings {
        assert!(!reads_same_value(finding), "{finding:?}");
    }
    Ok(())
}

#[test]
fn an_owner_another_function_calls_is_not_read_as_agreeing() -> Result<(), String> {
    // Control: `widest` may feed `largest` inputs no test names.
    let lib = "pub fn largest<T: PartialOrd>(a: T, b: T) -> T {\n    if a > b { b } else { a }\n}\n\npub fn widest(a: u32, b: u32) -> u32 {\n    largest(a + 1, b)\n}\n";
    let findings = findings(
        lib,
        LARGEST_OLD,
        "use invisible::{largest, widest};\n\n#[test]\nfn equal() {\n    assert_eq!(largest(2, 2), 2);\n}\n\n#[test]\nfn wide() {\n    assert_eq!(widest(1, 2), 2);\n}\n",
    )?;
    assert!(!findings.is_empty(), "the changed owner should be analyzed");
    for finding in &findings {
        assert!(!reads_same_value(finding), "{finding:?}");
    }
    Ok(())
}

#[test]
fn a_rebound_assert_eq_is_not_read_as_agreeing() -> Result<(), String> {
    // Control: the file rebinds `assert_eq!`, so its written operands are
    // not what runs.
    let test = "use invisible::largest;\n\nmacro_rules! assert_eq {\n    ($f:ident($a:expr, $b:expr), $e:expr) => {\n        std::assert_eq!($f($a - 1, $b), 1)\n    };\n}\n\n#[test]\nfn equal() {\n    assert_eq!(largest(2, 2), 2);\n}\n";
    let findings = findings(LARGEST, LARGEST_OLD, test)?;
    assert!(!findings.is_empty(), "the changed owner should be analyzed");
    for finding in &findings {
        assert!(!reads_same_value(finding), "{finding:?}");
    }
    Ok(())
}

const PAIR: &str = "pub fn pair(a: i32) -> (i32, bool) {\n    (a, a >= 0)\n}\n\npub fn tail() {}\n";
const PAIR_OLD: &str = "    (a, a > 0)";

#[test]
fn a_tuple_both_versions_build_alike_does_not_expose_a_changed_component() -> Result<(), String> {
    // `pair(5)` is `(5, true)` under `a > 0` and under `a >= 0`.
    let findings = findings(
        PAIR,
        PAIR_OLD,
        "use invisible::pair;\n\n#[test]\nfn five() {\n    assert_eq!(pair(5), (5, true));\n}\n",
    )?;
    let finding = return_value(&findings)?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert!(reads_same_value(finding), "{finding:?}");
    // The predicate lens has its own reading of the same line; it must not
    // credit the input either.
    let predicate = of_family(&findings, "predicate")?;
    assert_ne!(predicate.class, ExposureClass::Exposed, "{predicate:?}");
    Ok(())
}

#[test]
fn a_tuple_the_versions_build_differently_keeps_the_credit() -> Result<(), String> {
    // Control: `pair(0)` is `(0, true)` changed and `(0, false)` before.
    let findings = findings(
        PAIR,
        PAIR_OLD,
        "use invisible::pair;\n\n#[test]\nfn five() {\n    assert_eq!(pair(5), (5, true));\n}\n\n#[test]\nfn zero() {\n    assert_eq!(pair(0), (0, true));\n}\n",
    )?;
    let finding = return_value(&findings)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert!(!reads_same_value(finding), "{finding:?}");
    Ok(())
}

#[test]
fn an_equivalent_boundary_rewrite_is_not_sent_after_a_missing_input() -> Result<(), String> {
    // `if x > 50 { 50 } else { x }` and `if x >= 50 { 50 } else { x }` agree
    // at every input, so no test could separate them. Saying "no test input
    // reaches a difference" would ask for a test that cannot exist.
    let findings = findings(
        "pub fn clamp_max(x: u32) -> u32 {\n    if x >= 50 { 50 } else { x }\n}\n\npub fn tail() {}\n",
        "    if x > 50 { 50 } else { x }",
        "use invisible::clamp_max;\n\n#[test]\nfn at() {\n    assert_eq!(clamp_max(50), 50);\n}\n",
    )?;
    assert!(!findings.is_empty(), "the changed owner should be analyzed");
    for finding in &findings {
        assert!(!reads_same_value(finding), "{finding:?}");
    }
    Ok(())
}

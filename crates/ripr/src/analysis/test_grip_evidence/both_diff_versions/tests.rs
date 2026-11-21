//! #11738: reading the candidate rows first answers exactly what the
//! row-by-row reading of both versions answered, and a test that passes on
//! the candidate never reads the baseline body.

use super::super::value_contradiction::FOLD_PLAN_READS;
// The fold-only reading is the subject here; the entry point adds the
// predicate projection on top of it.
use super::fold_fails_on_both_diff_versions as test_fails_on_both_diff_versions;
use crate::analysis::facts::{FunctionSummary, RustIndex, TestSummary};
use crate::analysis::rust_index::summarize_file;
use crate::analysis::syntax::run_memo;
use crate::analysis::test_grip_evidence::{ExactValueVerdict, assertion_verdict_for_test};
use std::path::{Path, PathBuf};

/// The implementation this module replaced, verbatim, kept as the oracle.
fn reference_test_fails_on_both_diff_versions(
    test: &TestSummary,
    owner: &FunctionSummary,
    before: Option<&str>,
    after: Option<&str>,
    index: &RustIndex,
) -> bool {
    let (Some(before), Some(after)) = (before, after) else {
        return false;
    };
    if before == after || owner.body.matches(after).count() != 1 {
        return false;
    }
    let mut baseline_owner = owner.clone();
    baseline_owner.body =
        crate::analysis::facts::SourceText::from(owner.body.as_str().replacen(after, before, 1));
    let mut fails_on_candidate = false;
    let mut fails_on_base = false;
    for oracle in &test.assertions {
        if !fails_on_candidate
            && matches!(
                assertion_verdict_for_test(Some(owner), &oracle.text, test, index),
                ExactValueVerdict::Contradicted { .. }
            )
        {
            fails_on_candidate = true;
        }
        if !fails_on_base
            && matches!(
                assertion_verdict_for_test(Some(&baseline_owner), &oracle.text, test, index),
                ExactValueVerdict::Contradicted { .. }
            )
        {
            fails_on_base = true;
        }
        if fails_on_candidate && fails_on_base {
            return true;
        }
    }
    false
}

const LIB: &str = "src/lib.rs";

fn index(text: &str) -> RustIndex {
    let mut index = RustIndex::default();
    index.package_names.insert("demo".to_string());
    let facts = summarize_file(PathBuf::from(LIB), text.to_string());
    index.extend_functions(facts.functions.iter().cloned());
    index.extend_tests(facts.tests.iter().cloned());
    index.insert_file_only(PathBuf::from(LIB), facts);
    crate::analysis::facts::compose_fixture_source_roles(&mut index, Path::new(""));
    index
}

struct Lcg(u64);

impl Lcg {
    fn below(&mut self, bound: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % bound
    }
}

/// One owner with a changed `then` tail, and a test whose rows assert the
/// candidate value, the baseline value or a value neither returns.
struct Case {
    source: String,
    before: String,
    after: String,
}

fn case(rng: &mut Lcg) -> Case {
    let threshold = rng.below(6);
    let after_add = rng.below(4) + 1;
    let before_add = after_add + rng.below(3) + 1;
    let else_add = rng.below(5);
    let after = format!("x + {after_add}");
    let before = format!("x + {before_add}");
    let mut rows = String::new();
    for _ in 0..rng.below(3) + 1 {
        let input = rng.below(10);
        let candidate = if input > threshold {
            input + after_add
        } else {
            input + else_add
        };
        let baseline = if input > threshold {
            input + before_add
        } else {
            input + else_add
        };
        let expected = match rng.below(3) {
            0 => candidate,
            1 => baseline,
            _ => candidate + baseline + 100,
        };
        rows.push_str(&format!("        assert_eq!(pick({input}), {expected});\n"));
    }
    let source = format!(
        "pub fn pick(x: u64) -> u64 {{\n    if x > {threshold} {{\n        {after}\n    }} else {{\n        x + {else_add}\n    }}\n}}\n\n\
         #[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn picks() {{\n{rows}    }}\n}}\n"
    );
    Case {
        source,
        before,
        after,
    }
}

fn parts(index: &RustIndex) -> Option<(&FunctionSummary, &TestSummary)> {
    let owner = index
        .functions()
        .iter()
        .find(|owner| owner.name == "pick")?;
    let test = index.tests().iter().find(|test| test.name == "picks")?;
    Some((owner, test))
}

#[test]
fn candidate_first_answer_equals_the_row_by_row_reading_of_both_versions() -> Result<(), String> {
    let mut rng = Lcg(11_738);
    let (mut withheld, mut kept) = (0, 0);
    for _ in 0..150 {
        let case = case(&mut rng);
        let index = index(&case.source);
        let (owner, test) = parts(&index)
            .ok_or_else(|| format!("fixture lost its owner or test:\n{}", case.source))?;
        assert!(!test.assertions.is_empty(), "fixture lost its rows");
        for (before, after) in [
            (Some(case.before.as_str()), Some(case.after.as_str())),
            (None, Some(case.after.as_str())),
            (Some(case.before.as_str()), None),
            (Some(case.after.as_str()), Some(case.after.as_str())),
            (Some(case.before.as_str()), Some("x +")),
        ] {
            let expected =
                reference_test_fails_on_both_diff_versions(test, owner, before, after, &index);
            assert_eq!(
                test_fails_on_both_diff_versions(test, owner, before, after, &index),
                expected,
                "{before:?} -> {after:?} in\n{}",
                case.source
            );
            if expected {
                withheld += 1;
            } else {
                kept += 1;
            }
        }
    }
    // Both answers must occur often, or the corpus proves nothing.
    assert!(withheld >= 20, "only {withheld} withheld");
    assert!(kept >= 100, "only {kept} kept");
    Ok(())
}

#[test]
fn a_test_that_passes_on_the_candidate_never_reads_the_baseline_body() -> Result<(), String> {
    let source = "pub fn pick(x: u64) -> u64 {\n    if x > 2 {\n        x + 1\n    } else {\n        x\n    }\n}\n\n\
         #[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn picks() {\n        assert_eq!(pick(5), 6);\n        assert_eq!(pick(1), 1);\n    }\n}\n";
    let index = index(source);
    let (owner, test) = parts(&index).ok_or("fixture lost its owner or test")?;
    let reads = || FOLD_PLAN_READS.with(std::cell::Cell::get);
    let _scope = run_memo::enter();
    let start = reads();
    assert!(!test_fails_on_both_diff_versions(
        test,
        owner,
        Some("x + 2"),
        Some("x + 1"),
        &index
    ));
    assert_eq!(reads() - start, 1, "only the candidate body is read");
    // The replaced reading also read the baseline body.
    let start = reads();
    assert!(!reference_test_fails_on_both_diff_versions(
        test,
        owner,
        Some("x + 3"),
        Some("x + 1"),
        &index
    ));
    assert_eq!(
        reads() - start,
        1,
        "the candidate is memoized; the new baseline is read"
    );
    Ok(())
}

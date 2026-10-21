//! Bounded projection of a finding's observed values for machine output.
//!
//! A finding collects the values from every related test, and the related-test
//! heuristic is permissive: on a real workspace one finding can relate to
//! thousands of tests and carry more than ten thousand observed values. The
//! check JSON renders that list twice (`activation.observed_values` and the
//! promoted `observed_values`) plus one `assertion_texts` entry per line, so a
//! 500-line diff produced a 187 MB report. The renderers therefore project a
//! bounded subset and disclose the pre-cap count.
//!
//! The cap only affects rendering. Classification, missing discriminators and
//! every count computed from `finding.activation.observed_values` still use the
//! full vector.

use crate::domain::{ValueContext, ValueFact};

/// Describe a retained source value without claiming assertion execution or
/// admitted observation. ValueFact also represents refused assertion source
/// and bounded static value transfer; its presence is not an oracle witness.
pub(crate) fn source_value_evidence_line(fact: &ValueFact) -> String {
    format!(
        "source {} value {} at line {}",
        fact.context.as_str().replace('_', " "),
        fact.value,
        fact.line
    )
}

/// The human-output spelling of [`source_value_evidence_line`]: the same
/// sentence, with the test file named (`path:line`) when the producer recorded
/// it. The value is read from a related test, which is usually not in the diff,
/// so a bare line number cannot be found. Check JSON keeps the bare spelling.
pub(crate) fn source_value_evidence_line_with_file(fact: &ValueFact) -> String {
    // A multi-line string value would otherwise break the one-row layout of
    // the human evidence list (#9165); JSON keeps the raw value.
    let value = crate::output::related_test_miss::single_line_evidence_text(&fact.value);
    let context = fact.context.as_str().replace('_', " ");
    let Some(file) = &fact.file else {
        return format!("source {context} value {value} at line {}", fact.line);
    };
    format!(
        "source {context} value {value} at {}:{}",
        crate::output::path::display_path(file),
        fact.line
    )
}

/// Cap on observed values rendered per finding in check JSON and SARIF.
/// The pre-cap count is disclosed as `observed_values_total` whenever the cap
/// drops a value. Mirrors `MAX_RELATED_TESTS_PER_FINDING_JSON`.
pub(crate) const MAX_OBSERVED_VALUES_PER_FINDING: usize = 32;

/// The observed values a renderer should emit, in their original order.
///
/// Under the cap this is every value, unchanged. Over the cap it keeps the
/// values that say most about the inputs the tests use (call arguments, table
/// rows, builder calls, enum variants, returns) ahead of bare assertion
/// arguments, then restores the original order so the output stays stable
/// and reads like the uncapped list.
pub(crate) fn bounded_observed_values(facts: &[ValueFact]) -> Vec<&ValueFact> {
    if facts.len() <= MAX_OBSERVED_VALUES_PER_FINDING {
        return facts.iter().collect();
    }
    let mut ranked = facts.iter().enumerate().collect::<Vec<_>>();
    ranked.sort_by_key(|(index, fact)| (context_rank(&fact.context), *index));
    ranked.truncate(MAX_OBSERVED_VALUES_PER_FINDING);
    ranked.sort_by_key(|(index, _)| *index);
    ranked.into_iter().map(|(_, fact)| fact).collect()
}

/// The pre-cap count to disclose, or `None` when nothing was dropped.
pub(crate) fn elided_observed_values_total(facts: &[ValueFact]) -> Option<usize> {
    (facts.len() > MAX_OBSERVED_VALUES_PER_FINDING).then_some(facts.len())
}

fn context_rank(context: &ValueContext) -> u8 {
    match context {
        ValueContext::FunctionArgument => 0,
        ValueContext::TableRow => 1,
        ValueContext::BuilderMethod => 2,
        // Constants were labelled enum variants before #5357; they keep
        // that tier so the cap retains the same values.
        ValueContext::EnumVariant | ValueContext::Constant => 3,
        ValueContext::ReturnValue => 4,
        ValueContext::AssertionArgument => 5,
        ValueContext::Unknown => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fact(line: usize, context: ValueContext) -> ValueFact {
        ValueFact {
            file: None,
            line,
            text: format!("assert_eq!(f({line}), {line});"),
            value: line.to_string(),
            context,
        }
    }

    #[test]
    fn human_value_line_keeps_a_multi_line_string_value_on_one_row() {
        // #9165: a snapshot-style string literal spans lines; the human row
        // collapses it, and the JSON spelling still carries the raw value.
        let mut multi = fact(44, ValueContext::AssertionArgument);
        multi.file = Some(std::path::PathBuf::from("tests/test_expr.rs"));
        multi.value = "\"\n    Expr::Range {\n        limits: HalfOpen,\n    }\n    \"".to_string();

        let human = source_value_evidence_line_with_file(&multi);
        assert!(!human.contains('\n'), "{human:?}");
        assert_eq!(
            human,
            "source assertion argument value \" Expr::Range { limits: HalfOpen, } \" at tests/test_expr.rs:44"
        );
        assert!(source_value_evidence_line(&multi).contains('\n'));

        // Without a recorded file the human row is still one line.
        multi.file = None;
        let bare = source_value_evidence_line_with_file(&multi);
        assert!(!bare.contains('\n'), "{bare:?}");
        assert!(bare.ends_with(" at line 44"), "{bare:?}");

        // Negative control: a single-line value is unchanged.
        let mut single = fact(7, ValueContext::AssertionArgument);
        single.file = Some(std::path::PathBuf::from("tests/pricing.rs"));
        single.value = "\"a  b\"".to_string();
        assert_eq!(
            source_value_evidence_line_with_file(&single),
            "source assertion argument value \"a  b\" at tests/pricing.rs:7"
        );
    }

    #[test]
    fn under_the_cap_every_value_is_kept_in_order() {
        let facts = (1..=MAX_OBSERVED_VALUES_PER_FINDING)
            .map(|line| fact(line, ValueContext::AssertionArgument))
            .collect::<Vec<_>>();

        let bounded = bounded_observed_values(&facts);

        assert_eq!(bounded, facts.iter().collect::<Vec<_>>());
        assert_eq!(elided_observed_values_total(&facts), None);
    }

    #[test]
    fn over_the_cap_input_values_outrank_assertion_arguments_and_keep_order() {
        // 40 assertion arguments first, then 3 call arguments at later lines.
        let mut facts = (1..=40)
            .map(|line| fact(line, ValueContext::AssertionArgument))
            .collect::<Vec<_>>();
        facts.extend([
            fact(50, ValueContext::FunctionArgument),
            fact(51, ValueContext::TableRow),
            fact(52, ValueContext::FunctionArgument),
        ]);

        let bounded = bounded_observed_values(&facts);

        assert_eq!(bounded.len(), MAX_OBSERVED_VALUES_PER_FINDING);
        assert_eq!(elided_observed_values_total(&facts), Some(43));
        // The three input values survive although they come last.
        let lines = bounded.iter().map(|fact| fact.line).collect::<Vec<_>>();
        assert_eq!(&lines[lines.len() - 3..], &[50, 51, 52]);
        // The rest are the earliest assertion arguments, still in source order.
        assert_eq!(
            &lines[..MAX_OBSERVED_VALUES_PER_FINDING - 3],
            (1..=29).collect::<Vec<_>>().as_slice()
        );
    }
}

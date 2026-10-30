//! #9226: an integer-literal edit on a line that calls an imported symbol no
//! longer abstains as `missing_import_graph`, and is not credited as exposed
//! either: the callee is not evaluated, so the finding is `weakly_exposed`
//! with no nameable discriminator and no repair card.

use super::owners_tests::{extract_owners, extract_tests};
use super::*;
use std::path::Path;

const TEST_SOURCE: &str = "from src.seqs import take_nth\n\n\ndef test_take_nth():\n    assert list(take_nth(2, 'ABCDE')) == list('ACE')\n";

fn finding(old: &str, new: &str, tests_source: &str) -> Result<Finding, String> {
    let source = format!("import itertools\n\n\ndef take_nth(n, seq):\n{new}\n");
    let owners = extract_owners(Path::new("src/seqs.py"), &source);
    let tests = extract_tests(Path::new("tests/test_seqs.py"), tests_source);
    classify_change_with_old(Path::new("src/seqs.py"), 5, new, Some(old), &owners, &tests)
        .ok_or_else(|| "expected a finding".to_string())
}

fn abstains_on_the_import(finding: &Finding) -> bool {
    finding.class == ExposureClass::StaticUnknown
        && finding
            .evidence
            .iter()
            .any(|line| line.contains("missing_import_graph"))
}

fn assert_no_repair_signal(finding: &Finding) {
    assert_eq!(finding.recommended_next_step, None);
    assert!(finding.activation.missing_discriminators.is_empty());
    assert!(
        finding.canonical_gap.is_none(),
        "{:?}",
        finding.canonical_gap
    );
    assert!(finding.flow_sinks.is_empty());
    assert!(
        !finding
            .evidence
            .iter()
            .any(|line| line.starts_with("suggested_")),
        "{:?}",
        finding.evidence
    );
}

const OLD: &str = "    return itertools.islice(seq, 0, None, n)";

#[test]
fn an_integer_literal_swap_reads_as_weakly_exposed_with_no_repair_card() -> Result<(), String> {
    let finding = finding(
        OLD,
        "    return itertools.islice(seq, 1, None, n)",
        TEST_SOURCE,
    )?;
    assert_eq!(
        finding.class,
        ExposureClass::WeaklyExposed,
        "{:?}",
        finding.evidence
    );
    assert!(!abstains_on_the_import(&finding));
    assert_eq!(finding.recommended_next_step, None);
    assert!(finding.activation.missing_discriminators.is_empty());
    assert!(
        !finding
            .evidence
            .iter()
            .any(|line| line.starts_with("suggested_")),
        "{:?}",
        finding.evidence
    );
    assert!(
        finding
            .missing
            .iter()
            .any(|line| line.contains("does not evaluate"))
    );
    Ok(())
}

#[test]
fn a_negative_literal_and_a_subscript_swap_read_the_same_way() -> Result<(), String> {
    let finding = finding(
        "    return itertools.chain(seq)[-1]",
        "    return itertools.chain(seq)[-2]",
        TEST_SOURCE,
    )?;
    assert_eq!(
        finding.class,
        ExposureClass::WeaklyExposed,
        "{:?}",
        finding.evidence
    );
    Ok(())
}

#[test]
fn edits_that_move_anything_but_integer_values_keep_the_abstention() -> Result<(), String> {
    let cases = [
        // the callee moved
        (
            "    return itertools.islice(seq, 0, None, n)",
            "    return itertools.cycle(seq, 0, None, n)",
        ),
        // an operator moved
        (
            "    return itertools.islice(seq, n + 1, None, n)",
            "    return itertools.islice(seq, n - 1, None, n)",
        ),
        // an operand was dropped
        (
            "    return itertools.islice(seq, n + 1, None, n)",
            "    return itertools.islice(seq, n, None, n)",
        ),
        // a keyword moved
        (
            "    return itertools.islice(seq, 0, None, step=n)",
            "    return itertools.islice(seq, 0, None, stop=n)",
        ),
        // a float literal
        (
            "    return itertools.islice(seq, 0.5, None, n)",
            "    return itertools.islice(seq, 1.5, None, n)",
        ),
        // a string literal
        (
            "    return itertools.groupby(seq, 'a')",
            "    return itertools.groupby(seq, 'b')",
        ),
        // a name replaced by a literal
        (
            "    return itertools.islice(seq, n, None, n)",
            "    return itertools.islice(seq, 1, None, n)",
        ),
    ];
    for (old, new) in cases {
        let finding = finding(old, new, TEST_SOURCE)?;
        assert!(
            abstains_on_the_import(&finding),
            "{new}: {:?}",
            finding.evidence
        );
    }
    Ok(())
}

#[test]
fn without_the_old_line_the_abstention_stays() -> Result<(), String> {
    let new = "    return itertools.islice(seq, 1, None, n)";
    let source = format!("import itertools\n\n\ndef take_nth(n, seq):\n{new}\n");
    let owners = extract_owners(Path::new("src/seqs.py"), &source);
    let tests = extract_tests(Path::new("tests/test_seqs.py"), TEST_SOURCE);
    let finding = classify_change_with_old(Path::new("src/seqs.py"), 5, new, None, &owners, &tests)
        .ok_or_else(|| "expected a finding".to_string())?;
    assert!(abstains_on_the_import(&finding), "{:?}", finding.evidence);
    Ok(())
}

#[test]
fn a_predicate_line_gets_no_boundary_repair_either() -> Result<(), String> {
    let finding = finding(
        "    return itertools.count(n) > 3",
        "    return itertools.count(n) > 4",
        TEST_SOURCE,
    )?;
    assert_eq!(
        finding.class,
        ExposureClass::WeaklyExposed,
        "{:?}",
        finding.evidence
    );
    assert_no_repair_signal(&finding);
    Ok(())
}

#[test]
fn the_subscript_swap_has_no_repair_signal() -> Result<(), String> {
    let finding = finding(
        "    return itertools.chain(seq)[-1]",
        "    return itertools.chain(seq)[-2]",
        TEST_SOURCE,
    )?;
    assert_no_repair_signal(&finding);
    Ok(())
}

#[test]
fn only_a_strong_observing_oracle_lifts_the_abstention() -> Result<(), String> {
    let new = "    return itertools.islice(seq, 1, None, n)";
    // no test at all
    let none = finding(OLD, new, "")?;
    assert!(abstains_on_the_import(&none), "{:?}", none.evidence);
    // a smoke assertion that observes nothing exact
    let smoke = finding(
        OLD,
        new,
        "from src.seqs import take_nth\n\n\ndef test_take_nth():\n    assert take_nth(2, 'ABCDE')\n",
    )?;
    assert!(abstains_on_the_import(&smoke), "{:?}", smoke.evidence);
    // a strong assertion on a value the owner does not return
    let unrelated = finding(
        OLD,
        new,
        "from src.seqs import take_nth\n\n\ndef test_take_nth():\n    take_nth(2, 'ABCDE')\n    assert 1 + 1 == 2\n",
    )?;
    assert!(
        abstains_on_the_import(&unrelated),
        "{:?}",
        unrelated.evidence
    );
    Ok(())
}

#[test]
fn an_equal_valued_integer_respelling_is_not_an_edit() -> Result<(), String> {
    let finding = finding(
        "    return itertools.islice(seq, 0x1, None, n)",
        "    return itertools.islice(seq, 1, None, n)",
        TEST_SOURCE,
    )?;
    assert!(abstains_on_the_import(&finding), "{:?}", finding.evidence);
    Ok(())
}

#[test]
fn re_indenting_the_line_is_not_an_integer_only_edit() -> Result<(), String> {
    let finding = finding(
        "        return itertools.islice(seq, 0, None, n)",
        "    return itertools.islice(seq, 1, None, n)",
        TEST_SOURCE,
    )?;
    assert!(abstains_on_the_import(&finding), "{:?}", finding.evidence);
    Ok(())
}

#[test]
fn lambda_syntax_on_the_line_keeps_its_limit() -> Result<(), String> {
    let finding = finding(
        "    return itertools.islice(seq, 0 if (lambda: True) else 2, None, n)",
        "    return itertools.islice(seq, 1 if (lambda: True) else 2, None, n)",
        TEST_SOURCE,
    )?;
    assert_ne!(
        finding.class,
        ExposureClass::WeaklyExposed,
        "{:?}",
        finding.evidence
    );
    assert!(finding.canonical_gap.is_none());
    Ok(())
}

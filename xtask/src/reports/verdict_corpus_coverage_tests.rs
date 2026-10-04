use super::*;
use crate::reports::verdict_corpus::{CORPUS_DIR, load_corpus};

fn repo_path(relative: &str) -> std::path::PathBuf {
    crate::dogfood::repo_rooted_fixture_path(relative)
}

/// The committed corpus with every citation removed, so each test states
/// exactly the citations it scores.
fn bare_corpus() -> Result<Corpus, String> {
    let mut corpus = load_corpus(&repo_path(CORPUS_DIR))?;
    for case in &mut corpus.cases {
        case.spec_examples.clear();
    }
    Ok(corpus)
}

fn cite(corpus: &mut Corpus, index: usize, ids: &[&str]) {
    if let Some(case) = corpus.cases.get_mut(index) {
        case.spec_examples = ids.iter().map(|id| id.to_string()).collect();
    }
}

fn specs(rows: &[(&str, &[usize])]) -> SpecExamples {
    rows.iter()
        .map(|(id, numbers)| (id.to_string(), numbers.iter().copied().collect()))
        .collect()
}

fn ledger(text: &str) -> Result<Ledger, String> {
    toml::from_str(text).map_err(|err| err.to_string())
}

const LEDGER: &str = r#"
schema_version = "ripr_verdict_corpus_spec_coverage.v1"
floor = 2

[[spec]]
id = "RIPR-SPEC-0900"
scope = "in"

[[spec.waived]]
example = 3
reason = "every mutant is equivalent"

[[spec]]
id = "RIPR-SPEC-0901"
scope = "out"
reason = "CLI surface"

[[unmeasured]]
id = "RIPR-SPEC-0902"
reason = "prose examples"
"#;

fn fixture_specs() -> SpecExamples {
    specs(&[
        ("RIPR-SPEC-0900", &[1, 2, 3, 4]),
        ("RIPR-SPEC-0901", &[1, 2]),
        ("RIPR-SPEC-0902", &[]),
    ])
}

#[test]
fn numbered_examples_read_only_top_level_items_under_acceptance_examples() {
    let spec = "# RIPR-SPEC-0900: x\n\n## Behavior\n\n1. not an example\n\n## Acceptance Examples\n\nSource for 1 to 2:\n\n1. **First**: an item continued\n   over several lines\n   2. an indented line is a continuation, not an item\n2. Second.\n\n```text\n3. inside a fence\n```\n\n### Sub heading stays inside\n\n4. Fourth after a subheading.\n10. Tenth.\n\n## Test Mapping\n\n5. not an example\n";
    assert_eq!(
        numbered_examples(spec),
        [1, 2, 4, 10].into_iter().collect::<BTreeSet<_>>()
    );
    let prose = "## Acceptance Examples\n\n- serde `format_u8` scores false_actionable.\n- 2026 numbers stay prose.\n\n## Test Mapping\n";
    assert!(numbered_examples(prose).is_empty());
    assert!(numbered_examples("# no examples heading\n\n1. item\n").is_empty());
}

#[test]
fn spec_ids_come_from_the_spec_file_name() {
    assert_eq!(
        spec_id_of("RIPR-SPEC-0228-rust-field-write-observation.md").as_deref(),
        Some("RIPR-SPEC-0228")
    );
    assert_eq!(
        spec_id_of("RIPR-SPEC-0228.md").as_deref(),
        Some("RIPR-SPEC-0228")
    );
    assert_eq!(spec_id_of("README.md"), None);
    assert_eq!(spec_id_of("RIPR-SPEC-02x8-a.md"), None);
    assert_eq!(spec_id_of("RIPR-SPEC-02280-a.md"), None);
}

#[test]
fn example_ids_accept_only_the_canonical_spelling() {
    assert_eq!(
        parse_example_id("RIPR-SPEC-0227#10"),
        Some(("RIPR-SPEC-0227".to_string(), 10))
    );
    for bad in [
        "RIPR-SPEC-0227",
        "RIPR-SPEC-0227#",
        "RIPR-SPEC-0227#0",
        "RIPR-SPEC-0227#01",
        "RIPR-SPEC-227#1",
        "ripr-spec-0227#1",
        "RIPR-SPEC-0227 example 1",
        "RIPR-SPEC-0227#1a",
    ] {
        assert_eq!(parse_example_id(bad), None, "{bad}");
    }
}

#[test]
fn citations_of_malformed_unknown_or_unnumbered_examples_are_rejected() -> Result<(), String> {
    let ledger = ledger(LEDGER)?;
    let mut corpus = bare_corpus()?;
    cite(
        &mut corpus,
        0,
        &[
            "RIPR-SPEC-0900 example 1",
            "RIPR-SPEC-0999#1",
            "RIPR-SPEC-0900#7",
        ],
    );
    let violations = coverage_violations(&corpus, &ledger, &fixture_specs());
    let has = |needle: &str| violations.iter().any(|v| v.contains(needle));
    assert!(has("is not `RIPR-SPEC-NNNN#K`"), "{violations:#?}");
    assert!(
        has("no docs/specs file carries `RIPR-SPEC-0999`"),
        "{violations:#?}"
    );
    assert!(
        has("has no numbered acceptance example 7"),
        "{violations:#?}"
    );
    assert_eq!(violations.len(), 3, "{violations:#?}");
    Ok(())
}

#[test]
fn citing_an_out_of_scope_spec_is_rejected() -> Result<(), String> {
    let ledger = ledger(LEDGER)?;
    let mut corpus = bare_corpus()?;
    cite(&mut corpus, 0, &["RIPR-SPEC-0901#1"]);
    let violations = coverage_violations(&corpus, &ledger, &fixture_specs());
    assert!(
        violations.iter().any(|v| v.contains("out of scope")),
        "{violations:#?}"
    );
    Ok(())
}

#[test]
fn an_example_both_waived_and_covered_is_rejected() -> Result<(), String> {
    let ledger = ledger(LEDGER)?;
    let mut corpus = bare_corpus()?;
    cite(&mut corpus, 0, &["RIPR-SPEC-0900#3"]);
    let violations = coverage_violations(&corpus, &ledger, &fixture_specs());
    assert!(
        violations
            .iter()
            .any(|v| v.contains("`RIPR-SPEC-0900#3` is both waived")),
        "{violations:#?}"
    );
    cite(&mut corpus, 0, &["RIPR-SPEC-0900#1", "RIPR-SPEC-0900#2"]);
    let clean = coverage_violations(&corpus, &ledger, &fixture_specs());
    assert!(clean.is_empty(), "{clean:#?}");
    Ok(())
}

#[test]
fn ledger_must_name_every_spec_with_numbered_examples_and_only_real_ones() -> Result<(), String> {
    let ledger = ledger(LEDGER)?;
    let corpus = bare_corpus()?;
    let mut with_new = fixture_specs();
    with_new.insert("RIPR-SPEC-0903".to_string(), [1].into_iter().collect());
    let violations = coverage_violations(&corpus, &ledger, &with_new);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("`RIPR-SPEC-0903` has numbered acceptance examples but is missing")),
        "{violations:#?}"
    );
    // A spec without numbered examples needs no entry.
    let mut with_prose = fixture_specs();
    with_prose.insert("RIPR-SPEC-0904".to_string(), BTreeSet::new());
    assert!(coverage_violations(&corpus, &ledger, &with_prose).is_empty());

    let mut gone = fixture_specs();
    gone.remove("RIPR-SPEC-0901");
    let violations = coverage_violations(&corpus, &ledger, &gone);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("names `RIPR-SPEC-0901`, but no docs/specs file")),
        "{violations:#?}"
    );

    let mut numbered_now = fixture_specs();
    numbered_now.insert("RIPR-SPEC-0902".to_string(), [1].into_iter().collect());
    let violations = coverage_violations(&corpus, &ledger, &numbered_now);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("lists `RIPR-SPEC-0902` as unmeasured, but it now has")),
        "{violations:#?}"
    );

    let bad_waiver = LEDGER.replace("example = 3", "example = 9");
    let violations = coverage_violations(&corpus, &self::ledger(&bad_waiver)?, &fixture_specs());
    assert!(
        violations
            .iter()
            .any(|v| v.contains("waives `RIPR-SPEC-0900#9`, which is not")),
        "{violations:#?}"
    );

    let no_reason = LEDGER.replace("reason = \"CLI surface\"\n", "");
    let violations = coverage_violations(&corpus, &self::ledger(&no_reason)?, &fixture_specs());
    assert!(
        violations
            .iter()
            .any(|v| v.contains("out of scope without a reason")),
        "{violations:#?}"
    );
    Ok(())
}

#[test]
fn coverage_counts_cited_in_scope_examples_over_the_unwaived_ones() -> Result<(), String> {
    let ledger = ledger(LEDGER)?;
    let mut corpus = bare_corpus()?;
    cite(&mut corpus, 0, &["RIPR-SPEC-0900#1"]);
    // A second case on the same example does not count twice.
    cite(&mut corpus, 1, &["RIPR-SPEC-0900#1", "RIPR-SPEC-0900#4"]);
    let coverage = spec_example_coverage(&corpus, &ledger, &fixture_specs());
    assert_eq!(coverage.in_scope_examples, 4);
    assert_eq!(coverage.covered_examples, 2);
    assert_eq!(coverage.waived_examples, 1);
    assert_eq!(
        (coverage.coverage.numerator, coverage.coverage.denominator),
        (2, 3)
    );
    assert_eq!(
        (coverage.accounted.numerator, coverage.accounted.denominator),
        (3, 4)
    );
    assert_eq!(coverage.in_scope_specs, 1);
    assert_eq!(coverage.out_of_scope_specs, 1);
    assert_eq!(
        coverage.unmeasured_specs,
        vec!["RIPR-SPEC-0902".to_string()]
    );
    let row = coverage.by_spec.first().ok_or("no row")?;
    assert_eq!(row.spec, "RIPR-SPEC-0900");
    assert_eq!((row.covered, row.waived), (2, 1));
    assert_eq!(row.uncovered, vec![2]);
    Ok(())
}

#[test]
fn floor_gate_fails_below_passes_at_and_invites_a_raise_above() -> Result<(), String> {
    let ledger = ledger(LEDGER)?;
    let mut corpus = bare_corpus()?;
    cite(&mut corpus, 0, &["RIPR-SPEC-0900#1"]);
    let below = spec_example_coverage(&corpus, &ledger, &fixture_specs());
    let err = floor_gate(&below)
        .err()
        .ok_or("1 covered under floor 2 passed")?;
    assert!(err.contains("fell to 1/3 below the floor 2"), "{err}");
    assert!(err.contains("spec_examples"), "{err}");

    cite(&mut corpus, 1, &["RIPR-SPEC-0900#2"]);
    let equal = spec_example_coverage(&corpus, &ledger, &fixture_specs());
    assert_eq!(floor_gate(&equal)?, None);

    cite(&mut corpus, 2, &["RIPR-SPEC-0900#4"]);
    let above = spec_example_coverage(&corpus, &ledger, &fixture_specs());
    let note = floor_gate(&above)?.ok_or("no raise note above the floor")?;
    assert!(note.contains("raise `floor`"), "{note}");
    assert!(note.contains("to 3"), "{note}");

    let too_high = LEDGER.replace("floor = 2", "floor = 4");
    let violations = coverage_violations(&corpus, &self::ledger(&too_high)?, &fixture_specs());
    assert!(
        violations
            .iter()
            .any(|v| v.contains("floor 4 exceeds the 3 coverable examples")),
        "{violations:#?}"
    );
    Ok(())
}

#[test]
fn committed_ledger_is_valid_and_meets_its_floor() -> Result<(), String> {
    let corpus = load_corpus(&repo_path(CORPUS_DIR))?;
    let ledger = load_ledger(&repo_path(CORPUS_DIR))?;
    let specs = scan_specs(&repo_path(SPECS_DIR))?;
    let violations = coverage_violations(&corpus, &ledger, &specs);
    assert!(violations.is_empty(), "{violations:#?}");
    let coverage = spec_example_coverage(&corpus, &ledger, &specs);
    assert_eq!(
        floor_gate(&coverage)?,
        None,
        "floor is not the covered count"
    );
    // The backfilled spec cases: RIPR-SPEC-0227 examples 10 and 11 come from
    // the older checkout and accounts cases.
    for (case, id) in [
        ("checkout-withdraw-sibling-variant", "RIPR-SPEC-0227#10"),
        ("accounts-parse-too-long-variant", "RIPR-SPEC-0227#11"),
    ] {
        assert!(
            corpus
                .cases
                .iter()
                .any(|c| c.case_id == case && c.spec_examples.iter().any(|e| e == id)),
            "{case} does not cite {id}"
        );
    }
    Ok(())
}

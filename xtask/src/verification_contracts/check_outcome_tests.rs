//! Schema-only decisions: no typed deserializer may reject a mutant first.
//! `complete_no_findings` below is a contract example, not a runtime fixture.
use super::{read_json, repo_root, validate_value_against_schema};
use serde_json::{Value, json};

const KINDS: [(&str, bool, [u64; 5]); 8] = [
    ("no_scope", true, [0, 0, 0, 0, 0]),
    ("no_changed_lines", true, [1, 0, 0, 0, 0]),
    ("no_behavioral_candidates", true, [1, 2, 0, 0, 0]),
    ("complete_no_findings", true, [1, 1, 1, 0, 0]),
    ("complete_with_findings", true, [1, 1, 1, 1, 1]),
    ("partial_with_limitations", false, [0, 0, 0, 0, 0]),
    ("unsupported_input", false, [0, 0, 0, 0, 0]),
    ("analysis_failed", false, [0, 0, 0, 0, 0]),
];
const COUNTS: [&str; 5] = [
    "changed_file_count",
    "changed_line_count",
    "candidate_line_count",
    "probe_count",
    "finding_count",
];
const PROJECTION_RULES: &str = "/$defs/analysis_outcome_projection/allOf";
const OUTCOME_RULES: &str = "/$defs/analysis_outcome/allOf";

fn schema() -> Result<Value, String> {
    read_json(repo_root()?.join("schemas/ripr/check.schema.json"))
}

fn projections() -> Result<Vec<Value>, String> {
    let root = repo_root()?;
    let complete =
        read_json(root.join("tests/fixtures/verification/ripr/check-complete.valid.json"))?;
    let incomplete =
        read_json(root.join("tests/fixtures/verification/ripr/check-incomplete.valid.json"))?;
    let limitation = incomplete["analysis_outcome"]["outcome"]["limitations"][0].clone();
    if !limitation.is_object() {
        return Err("incomplete producer fixture must contain a typed limitation".to_string());
    }
    Ok(KINDS
        .iter()
        .map(|(kind, complete_flag, counts)| {
            let mut projection = complete["analysis_outcome"].clone();
            projection["analysis_complete"] = json!(complete_flag);
            projection["outcome"]["kind"] = json!(kind);
            projection["outcome"]["limitations"] = if *complete_flag {
                json!([])
            } else {
                json!([limitation.clone()])
            };
            for (field, count) in COUNTS.iter().zip(counts) {
                projection["outcome"]["counts"][*field] = json!(count);
            }
            projection
        })
        .collect())
}

fn accepts(schema: &Value, projection: &Value) -> bool {
    let mut violations = Vec::new();
    validate_value_against_schema(
        projection,
        &json!({"$ref": "#/$defs/analysis_outcome_projection"}),
        schema,
        "projection".to_string(),
        &mut violations,
    );
    violations.is_empty()
}

#[test]
fn check_outcome_schema_valid_matrix() -> Result<(), String> {
    let schema = schema()?;
    let cases = projections()?;
    assert_eq!(cases.len(), 8);
    for case in &cases {
        assert!(
            accepts(&schema, case),
            "valid contract example rejected: {case}"
        );
    }
    // EOL-only churn is allowed on every complete kind. Do not ban all limitations.
    for case in &cases[..5] {
        let mut eol = case.clone();
        eol["outcome"]["limitations"] = cases[5]["outcome"]["limitations"].clone();
        eol["outcome"]["limitations"][0]["kind"] = json!("eol_only_churn");
        assert!(
            accepts(&schema, &eol),
            "valid EOL disclosure rejected: {eol}"
        );
    }
    // The typed contract permits either a candidate or a probe, and does not
    // impose changed-file/line ordering or findings == probes on partial runs.
    for (index, counts) in [
        (1, [0, 0, 0, 0, 0]),
        (3, [0, 0, 0, 1, 0]),
        (5, [0, 0, 0, 0, 2]),
    ] {
        let mut alternate = cases[index].clone();
        for (field, count) in COUNTS.iter().zip(counts) {
            alternate["outcome"]["counts"][*field] = json!(count);
        }
        assert!(
            accepts(&schema, &alternate),
            "valid boundary rejected: {alternate}"
        );
    }
    Ok(())
}

// Each row changes exactly one field of a valid projection. The final pair
// identifies the schema rule whose removal must admit that specific mutant.
fn mutants() -> Result<Vec<(Value, &'static str, usize)>, String> {
    let cases = projections()?;
    let mut mutants = Vec::new();
    for (index, (_, complete, _)) in KINDS.iter().enumerate() {
        let mut flipped = cases[index].clone();
        flipped["analysis_complete"] = json!(!complete);
        mutants.push((flipped, PROJECTION_RULES, 0));

        let mut limitations = cases[index].clone();
        limitations["outcome"]["limitations"] = if index < 5 {
            cases[5]["outcome"]["limitations"].clone()
        } else {
            json!([])
        };
        mutants.push((limitations, OUTCOME_RULES, 0));
    }
    for (case, field, wrong, rule) in [
        (0, 0, 1, 1),
        (0, 1, 1, 1),
        (0, 2, 1, 1),
        (0, 3, 1, 1),
        (0, 4, 1, 1),
        (1, 1, 1, 2),
        (1, 2, 1, 2),
        (1, 3, 1, 2),
        (1, 4, 1, 2),
        (2, 1, 0, 3),
        (2, 2, 1, 3),
        (2, 3, 1, 3),
        (2, 4, 1, 3),
        (3, 4, 1, 4),
        (3, 2, 0, 4),
        (4, 4, 0, 5),
    ] {
        let mut changed = cases[case].clone();
        changed["outcome"]["counts"][COUNTS[field]] = json!(wrong);
        mutants.push((changed, OUTCOME_RULES, rule));
    }
    Ok(mutants)
}

#[test]
fn check_outcome_schema_rejects_contradictions() -> Result<(), String> {
    let schema = schema()?;
    let mutants = mutants()?;
    assert_eq!(mutants.len(), 32);
    for (mutant, _, _) in mutants {
        assert!(
            !accepts(&schema, &mutant),
            "contradiction accepted by schema: {mutant}"
        );
    }
    Ok(())
}

#[test]
fn check_outcome_schema_each_rule_has_a_removal_witness() -> Result<(), String> {
    let schema = schema()?;
    let mutants = mutants()?;
    for (rules, count) in [(PROJECTION_RULES, 1), (OUTCOME_RULES, 6)] {
        assert_eq!(
            schema
                .pointer(rules)
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(count)
        );
        for index in 0..count {
            let mut removed = schema.clone();
            removed
                .pointer_mut(rules)
                .and_then(Value::as_array_mut)
                .ok_or_else(|| format!("missing rule array {rules}"))?
                .remove(index);
            let witnesses: Vec<_> = mutants
                .iter()
                .filter(|(_, path, rule)| *path == rules && *rule == index)
                .collect();
            assert!(
                !witnesses.is_empty(),
                "rule has no witness: {rules}/{index}"
            );
            for (mutant, _, _) in witnesses {
                assert!(
                    accepts(&removed, mutant),
                    "removal did not admit intended mutant: {rules}/{index}: {mutant}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn check_outcome_schema_each_leaf_has_a_removal_witness() -> Result<(), String> {
    let schema = schema()?;
    let mutants = mutants()?;
    let mut leaves = vec![
        (
            format!("{PROJECTION_RULES}/0/then/properties/analysis_complete/const"),
            0,
        ),
        (
            format!("{PROJECTION_RULES}/0/else/properties/analysis_complete/const"),
            10,
        ),
        (
            format!("{OUTCOME_RULES}/0/then/properties/limitations/items/properties/kind/const"),
            1,
        ),
        (
            format!("{OUTCOME_RULES}/0/else/properties/limitations/minItems"),
            11,
        ),
    ];
    for (rule, fields, first_witness) in [
        (1, &COUNTS[..], 16),
        (2, &COUNTS[1..], 21),
        (3, &COUNTS[1..], 25),
    ] {
        for (offset, field) in fields.iter().enumerate() {
            let keyword = if rule == 3 && offset == 0 {
                "minimum"
            } else {
                "const"
            };
            leaves.push((
                format!(
                    "{OUTCOME_RULES}/{rule}/then/properties/counts/properties/{field}/{keyword}"
                ),
                first_witness + offset,
            ));
        }
    }
    leaves.extend([
        (format!("{OUTCOME_RULES}/4/then/properties/counts/properties/finding_count/const"), 29),
        (format!("{OUTCOME_RULES}/4/then/properties/counts/anyOf/0/properties/candidate_line_count/minimum"), 30),
        (format!("{OUTCOME_RULES}/4/then/properties/counts/anyOf/1/properties/probe_count/minimum"), 30),
        (format!("{OUTCOME_RULES}/5/then/properties/counts/properties/finding_count/minimum"), 31),
    ]);
    assert_eq!(leaves.len(), 21);
    for (pointer, witness) in leaves {
        let (parent, keyword) = pointer
            .rsplit_once('/')
            .ok_or("invalid constraint pointer")?;
        let mut removed = schema.clone();
        let removed_keyword = removed
            .pointer_mut(parent)
            .and_then(Value::as_object_mut)
            .ok_or_else(|| format!("missing constraint parent {parent}"))?
            .remove(keyword);
        assert!(removed_keyword.is_some(), "missing constraint {pointer}");
        assert!(
            !accepts(&schema, &mutants[witness].0),
            "witness was not rejected: {pointer}"
        );
        assert!(
            accepts(&removed, &mutants[witness].0),
            "removal did not admit witness: {pointer}"
        );
    }
    Ok(())
}

fn envelope_accepts(schema: &Value, envelope: &Value) -> bool {
    let mut violations = Vec::new();
    validate_value_against_schema(
        envelope,
        schema,
        schema,
        "check envelope".to_string(),
        &mut violations,
    );
    violations.is_empty()
}

#[test]
fn check_envelope_zero_totals_and_removal_controls() -> Result<(), String> {
    let schema = schema()?;
    let root = repo_root()?;
    let positive =
        read_json(root.join("tests/fixtures/verification/ripr/check-complete.valid.json"))?;
    let zero =
        read_json(root.join("tests/fixtures/verification/ripr/check-incomplete.valid.json"))?;
    let limited =
        read_json(root.join("tests/fixtures/verification/ripr/check-limited.valid.json"))?;
    assert_eq!(
        positive["analysis_outcome"]["outcome"]["counts"]["finding_count"],
        2
    );
    assert_eq!(
        positive["analysis_outcome"]["outcome"]["counts"]["probe_count"],
        2
    );
    assert_eq!(
        zero["analysis_outcome"]["outcome"]["counts"]["finding_count"],
        0
    );
    assert_eq!(
        zero["analysis_outcome"]["outcome"]["counts"]["probe_count"],
        0
    );
    assert!(
        positive["findings"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );
    assert_eq!(zero["findings"], json!([]));
    // Contract examples: bounded rendering retains a positive prefix of the total;
    // legacy envelopes may omit the optional typed projection entirely.
    let mut bounded = positive.clone();
    bounded["findings"] = json!([positive["findings"][0].clone()]);
    let mut legacy = positive.clone();
    legacy
        .as_object_mut()
        .ok_or("fixture must be an object")?
        .remove("analysis_outcome");
    for valid in [&positive, &zero, &limited, &bounded, &legacy] {
        assert!(
            envelope_accepts(&schema, valid),
            "valid envelope rejected: {valid}"
        );
    }
    let mutants = [
        (
            &zero,
            "/summary/findings",
            json!(1),
            "/allOf/0/then/then/properties/summary/properties/findings/const",
        ),
        (
            &zero,
            "/findings",
            json!([positive["findings"][0].clone()]),
            "/allOf/0/then/then/properties/findings/maxItems",
        ),
        (
            &positive,
            "/summary/findings",
            json!(0),
            "/allOf/0/then/else/properties/summary/properties/findings/minimum",
        ),
        (
            &positive,
            "/findings",
            json!([]),
            "/allOf/0/then/else/properties/findings/minItems",
        ),
        (
            &zero,
            "/summary/probes",
            json!(1),
            "/allOf/1/then/then/properties/summary/properties/probes/const",
        ),
        (
            &positive,
            "/summary/probes",
            json!(0),
            "/allOf/1/then/else/properties/summary/properties/probes/minimum",
        ),
    ];
    for (base, field, wrong, constraint) in mutants {
        let mut mutant = base.clone();
        *mutant
            .pointer_mut(field)
            .ok_or_else(|| format!("missing fixture field {field}"))? = wrong;
        assert!(
            !envelope_accepts(&schema, &mutant),
            "contradictory total accepted: {field}"
        );
        let (parent, keyword) = constraint
            .rsplit_once('/')
            .ok_or("invalid constraint pointer")?;
        let mut removed = schema.clone();
        assert!(
            removed
                .pointer_mut(parent)
                .and_then(Value::as_object_mut)
                .ok_or("missing total constraint parent")?
                .remove(keyword)
                .is_some()
        );
        assert!(
            envelope_accepts(&removed, &mutant),
            "total constraint lacks a removal witness: {constraint}"
        );
        let mut removed_rule = schema.clone();
        let index = usize::from(!constraint.starts_with("/allOf/0/"));
        removed_rule["allOf"]
            .as_array_mut()
            .ok_or("missing envelope rules")?
            .remove(index);
        assert!(
            envelope_accepts(&removed_rule, &mutant),
            "envelope rule lacks a removal witness"
        );
    }
    for index in 0..2 {
        let mut unguarded = schema.clone();
        assert!(
            unguarded["allOf"][index]["if"]
                .as_object_mut()
                .ok_or("missing optional projection guard")?
                .remove("required")
                .is_some()
        );
        assert!(
            !envelope_accepts(&unguarded, &legacy),
            "missing-projection guard is ineffective"
        );
    }
    Ok(())
}

use super::population::{Contribution, Population, SCHEMA};
use super::*;

fn config() -> Result<Config, String> {
    parse_config(
        r#"
schema_version = "ripr-dx-scoreboards-v1"
[[metric]]
id = "trust.abstention"
board = "trust"
title = "Abstention"
unit = "ratio"
direction = "lower_is_better"
target = 0.2
regression_pct = 0
regression_floor = 0.02
runner_dependent = false
source = "verdict-corpus:fixture#abstention_rate"
"#,
    )
}

fn cases(count: usize, abstentions: usize) -> Population {
    Population {
        schema_version: SCHEMA.to_string(),
        rate: "abstention_rate".to_string(),
        cases: (0..count)
            .map(|i| Contribution {
                case_id: format!("case-{i}"),
                definition_sha256: "a".repeat(64),
                numerator: u64::from(i < abstentions),
                denominator: 1,
            })
            .collect(),
    }
}

fn report(base: Population, current: Population) -> Result<Value, String> {
    let mut config = config()?;
    config.metric[0].source = format!("verdict-corpus:fixture#{}", current.rate);
    let boards = vec!["trust".to_string()];
    let sample = |population| Sample {
        metric: "trust.abstention".to_string(),
        repo: None,
        outcome: SampleOutcome::Corpus(population),
        detail: "committed records".to_string(),
    };
    let context = RunContext::default();
    let baseline = build_report(&config, &boards, &[sample(base)], &context, None, false);
    Ok(build_report(
        &config,
        &boards,
        &[sample(current)],
        &context,
        Some(&baseline),
        true,
    ))
}

#[test]
fn added_abstentions_do_not_regress_unchanged_common_cases() -> Result<(), String> {
    let mut current = cases(8, 1);
    for case in &mut current.cases[4..] {
        case.numerator = 1;
    }
    let report = report(cases(4, 1), current)?;
    assert_eq!(report["metrics"][0]["value"], json!(0.625));
    assert_eq!(report["metrics"][0]["status"], "below_target");
    assert_eq!(report["gate"]["status"], "pass", "{}", report["gate"]);
    Ok(())
}

#[test]
fn added_passing_cases_cannot_hide_a_retained_regression() -> Result<(), String> {
    let report = report(cases(4, 1), cases(20, 2))?;
    assert_eq!(report["metrics"][0]["value"], json!(0.1));
    assert_eq!(report["metrics"][0]["status"], "meets_target");
    assert_eq!(report["gate"]["status"], "fail", "{}", report["gate"]);
    Ok(())
}

#[test]
fn actual_recorded_retained_58_to_74_still_fails() -> Result<(), String> {
    let baseline: Value = serde_json::from_str(include_str!(
        "../../../../benchmarks/dx_scoreboard/testdata/7552-original-baseline-with-population.json"
    ))
    .map_err(|err| err.to_string())?;
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../benchmarks/dx_scoreboard/testdata/7552-rust-abstention.json"
    ))
    .map_err(|err| err.to_string())?;
    assert_eq!(
        baseline["revision"],
        "10e56371911a4e6ef8020fbcd7e5f2692fc9b7f1"
    );
    assert_eq!(
        fixture["source_revision"],
        "6da363f9b630845d82d8a073c42429d90cd03732"
    );
    let current: Population =
        serde_json::from_value(fixture["population"].clone()).map_err(|err| err.to_string())?;
    assert_eq!(current.totals()?, (129, 365));
    let mut config = config()?;
    config.metric[0].id = "trust.verdict_corpus_abstention".to_string();
    let samples = [Sample {
        metric: config.metric[0].id.clone(),
        repo: None,
        outcome: SampleOutcome::Corpus(current),
        detail: "frozen committed expected rows at 6da363f9b; no analyzer execution".to_string(),
    }];
    let report = build_report(
        &config,
        &["trust".to_string()],
        &samples,
        &RunContext::default(),
        Some(&baseline),
        true,
    );
    let compared = &report["metrics"][0]["baseline"];
    assert_eq!(compared["common"]["cases_total"], 203);
    assert_eq!(compared["common"]["baseline"]["numerator"], 58);
    assert_eq!(compared["common"]["baseline"]["denominator"], 203);
    assert_eq!(compared["common"]["current"]["numerator"], 74);
    assert_eq!(compared["common"]["current"]["denominator"], 203);
    assert_eq!(compared["added"]["numerator"], 55);
    assert_eq!(compared["added"]["denominator"], 162);
    assert_eq!(compared["changed_definitions"], json!([]));
    assert_eq!(compared["removed_cases"], json!([]));
    assert_eq!(compared["allowed_worsening"], json!(0.02));
    assert_eq!(compared["delta"], json!(0.0788));
    assert_eq!(report["gate"]["status"], "fail");
    assert_eq!(report["gate"]["regressions"][0]["current"], json!(0.3645));
    assert!(
        render_markdown(&report)
            .contains("common 203 cases: 58/203 → 74/203 **regressed**; added 162 (55/162)")
    );
    assert!(gate_failure_message(&report).contains("baseline 0.2857 -> current 0.3645"));
    Ok(())
}

#[test]
fn migration_preserves_every_original_baseline_field_and_all_five_fractions() -> Result<(), String>
{
    let mut baseline: Value = serde_json::from_str(include_str!(
        "../../../../benchmarks/dx_scoreboard/testdata/7552-original-baseline-with-population.json"
    ))
    .map_err(|err| err.to_string())?;
    let expected = [
        ("false_verdict_rate", 72, 203),
        ("false_actionable_rate", 66, 106),
        ("abstention_rate", 58, 203),
        ("false_silent_rate", 0, 97),
        ("false_exposed_rate", 6, 97),
    ];
    let mut seen = BTreeMap::new();
    for metric in baseline["metrics"]
        .as_array_mut()
        .ok_or("metrics missing")?
    {
        for sample in metric["samples"].as_array_mut().ok_or("samples missing")? {
            if !sample["population"].is_null() {
                let population: Population = serde_json::from_value(sample["population"].clone())
                    .map_err(|err| err.to_string())?;
                assert_eq!(population.cases.len(), 203);
                seen.insert(population.rate.clone(), population.totals()?);
                assert_eq!(
                    sample["population_provenance"]["revision"],
                    "10e56371911a4e6ef8020fbcd7e5f2692fc9b7f1"
                );
                assert_eq!(
                    sample["population_provenance"]["report_blob"],
                    "0b038c45bb6b2fa25faf8708c57cf41e58522e6d"
                );
                let sample = sample.as_object_mut().ok_or("sample object missing")?;
                sample.remove("population");
                sample.remove("population_provenance");
            }
        }
    }
    assert_eq!(seen.len(), 5);
    for (rate, n, d) in expected {
        assert_eq!(seen.get(rate), Some(&(n, d)));
    }
    let original = serde_json::to_string_pretty(&baseline).map_err(|err| err.to_string())? + "\n";
    assert_eq!(
        super::super::verdict_corpus::sha256_hex(original.as_bytes()),
        "2df8bc3f48d6ec1384464d7baacfe8e31a24553dcfc019c6b4f9dfbacf667ea5"
    );
    Ok(())
}

#[test]
fn additions_removals_and_changed_definitions_are_separate() -> Result<(), String> {
    let mut current = cases(5, 1);
    current.cases[1].definition_sha256 = "b".repeat(64);
    current.cases[1].numerator = 1;
    current.cases[4].numerator = 1;
    current.cases.remove(3);
    let report = report(cases(4, 1), current)?;
    let comparison = &report["metrics"][0]["baseline"];
    assert_eq!(report["gate"]["status"], "pass");
    assert_eq!(comparison["common"]["cases_total"], 2);
    assert_eq!(comparison["common"]["baseline"]["numerator"], 1);
    assert_eq!(comparison["common"]["baseline"]["denominator"], 2);
    assert_eq!(comparison["common"]["current"]["numerator"], 1);
    assert_eq!(comparison["new_cases"], json!(["case-4"]));
    assert_eq!(comparison["removed_cases"], json!(["case-3"]));
    assert_eq!(comparison["changed_definitions"], json!(["case-1"]));
    assert_eq!(comparison["added"]["numerator"], 1);
    assert_eq!(comparison["removed"]["denominator"], 1);
    Ok(())
}

#[test]
fn missing_duplicate_and_incomparable_evidence_block_the_gate() -> Result<(), String> {
    let mut duplicate = cases(4, 1);
    duplicate.cases.push(duplicate.cases[0].clone());
    let mut no_common = cases(4, 1);
    for case in &mut no_common.cases {
        case.definition_sha256 = "b".repeat(64);
    }
    let mut invalid = cases(4, 1);
    invalid.cases[0].numerator = 2;
    for current in [duplicate, no_common, invalid] {
        let report = report(cases(4, 1), current)?;
        assert_eq!(report["gate"]["status"], "fail");
        assert_eq!(
            report["gate"]["failed_comparisons"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        assert!(gate_failure_message(&report).contains("comparison failed trust.abstention"));
        assert!(render_markdown(&report).contains("**Comparison failed**"));
        assert_eq!(report["gate"]["uncompared"], json!([]));
    }
    let config = config()?;
    let scalar = Sample {
        metric: "trust.abstention".to_string(),
        repo: None,
        outcome: SampleOutcome::Value(0.25),
        detail: "legacy aggregate".to_string(),
    };
    let baseline = build_report(
        &config,
        &["trust".to_string()],
        std::slice::from_ref(&scalar),
        &RunContext::default(),
        None,
        false,
    );
    let report = build_report(
        &config,
        &["trust".to_string()],
        &[scalar],
        &RunContext::default(),
        Some(&baseline),
        true,
    );
    assert_eq!(report["gate"]["status"], "fail");
    assert!(
        gate_failure_message(&report).contains("identity must come from the recorded baseline")
    );
    Ok(())
}

#[test]
fn no_common_eligible_denominator_is_a_blocking_comparison_failure() -> Result<(), String> {
    let mut base = cases(3, 0);
    base.cases[0].denominator = 0;
    base.cases[1].denominator = 0;
    base.rate = "false_actionable_rate".to_string();
    let mut current = base.clone();
    current.cases[2].case_id = "new".to_string();
    let report = report(base, current)?;
    assert_eq!(report["gate"]["status"], "fail");
    assert_eq!(report["metrics"][0]["baseline"]["common"]["cases_total"], 2);
    assert_eq!(
        report["metrics"][0]["baseline"]["common"]["current"]["denominator"],
        0
    );
    assert!(gate_failure_message(&report).contains("no eligible unchanged common cases"));
    Ok(())
}

fn fixture_json(path: &Path, value: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(
        path,
        serde_json::to_string_pretty(value).map_err(|err| err.to_string())?,
    )
    .map_err(|err| format!("write {}: {err}", path.display()))
}

fn scoring_fixture() -> Result<PathBuf, String> {
    use super::super::verdict_corpus::{TruthState, load_corpus, stored_path};
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/rust-verdict-corpus");
    let corpus = load_corpus(&source)?;
    let mut selected: Vec<_> = corpus
        .cases
        .iter()
        .filter(|case| case.truth.state == TruthState::Discriminated)
        .take(2)
        .collect();
    selected.extend(
        corpus
            .cases
            .iter()
            .filter(|case| case.truth.state == TruthState::NotDiscriminated)
            .take(2),
    );
    assert_eq!(
        selected.len(),
        4,
        "fixture needs two cases in each eligibility cohort"
    );
    let dir = crate::tests::temp_dir("dx-population-scoring");
    fixture_json(
        &dir.join("corpus.json"),
        &read_json(&source.join("corpus.json"))?,
    )?;
    for (i, case) in selected.iter().enumerate() {
        fixture_json(
            &dir.join("cases").join(format!("{}.json", case.case_id)),
            &serde_json::to_value(case).map_err(|err| err.to_string())?,
        )?;
        fs::copy(source.join(&case.diff), dir.join(&case.diff)).map_err(|err| err.to_string())?;
        let subject = corpus
            .subjects
            .iter()
            .find(|subject| subject.subject_id == case.subject_id)
            .ok_or("fixture subject missing")?;
        fixture_json(
            &dir.join("subjects")
                .join(format!("{}.json", subject.subject_id)),
            &serde_json::to_value(subject).map_err(|err| err.to_string())?,
        )?;
        for file in &subject.retained_files {
            let relative = Path::new("subjects")
                .join(&subject.subject_id)
                .join(stored_path(&file.path));
            let target = dir.join(&relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|err| err.to_string())?;
            }
            fs::copy(source.join(relative), target).map_err(|err| err.to_string())?;
        }
        let mut row = read_json(
            &source
                .join("expected/rows")
                .join(format!("{}.json", case.case_id)),
        )?;
        let (verdict, outcome, class) = match i {
            0 => ("gap", "false_actionable", "reachable_unrevealed"),
            1 => ("credited", "ideal", "exposed"),
            2 => ("credited", "false_exposed", "exposed"),
            _ => ("gap", "ideal", "reachable_unrevealed"),
        };
        row["observed_verdict"] = json!(verdict);
        row["outcome"] = json!(outcome);
        row["observed_classifications"] = json!([class]);
        fixture_json(
            &dir.join("expected/rows")
                .join(format!("{}.json", case.case_id)),
            &row,
        )?;
    }
    Ok(dir)
}

#[test]
fn corpus_producer_keeps_nonuniform_eligibility_and_refuses_bad_rows_and_bytes()
-> Result<(), String> {
    let dir = scoring_fixture()?;
    let records = population::Records::load(&dir)?;
    assert_eq!(
        records.population("false_actionable_rate")?.totals()?,
        (1, 2)
    );
    assert_eq!(records.population("false_exposed_rate")?.totals()?, (1, 2));
    assert_eq!(records.population("false_verdict_rate")?.totals()?, (2, 4));
    assert_eq!(
        records
            .population("false_actionable_rate")?
            .cases
            .iter()
            .filter(|case| case.denominator == 0)
            .count(),
        2
    );
    let row_path = fs::read_dir(dir.join("expected/rows"))
        .map_err(|err| err.to_string())?
        .next()
        .ok_or("fixture row missing")?
        .map_err(|err| err.to_string())?
        .path();
    let original = read_json(&row_path)?;
    let mut wrong = original.clone();
    wrong["subject_id"] = json!("wrong-owner");
    fixture_json(&row_path, &wrong)?;
    assert!(
        population::Records::load(&dir)
            .is_err_and(|err| err.contains("disagrees with its scoring definition"))
    );
    fixture_json(&row_path, &original)?;
    fs::remove_file(&row_path).map_err(|err| err.to_string())?;
    assert!(population::Records::load(&dir).is_err_and(|err| err.contains("missing rows")));
    fixture_json(&row_path, &original)?;
    let corpus = super::super::verdict_corpus::load_corpus(&dir)?;
    let subject = corpus.subjects.first().ok_or("subject missing")?;
    let file = subject
        .retained_files
        .first()
        .ok_or("retained file missing")?;
    let path = dir
        .join("subjects")
        .join(&subject.subject_id)
        .join(super::super::verdict_corpus::stored_path(&file.path));
    fs::write(&path, "changed retained bytes").map_err(|err| err.to_string())?;
    assert!(population::Records::load(&dir).is_err_and(|err| err.contains("sha256")));
    fs::remove_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(())
}

#[test]
fn corpus_producer_fingerprints_diff_projection_and_truth_but_ignores_annotations()
-> Result<(), String> {
    let dir = scoring_fixture()?;
    let original = population::Records::load(&dir)?.population("abstention_rate")?;
    let header_path = dir.join("corpus.json");
    let header = read_json(&header_path)?;
    let mut annotation = header.clone();
    annotation["description"] = json!("annotation only");
    fixture_json(&header_path, &annotation)?;
    assert_eq!(
        population::Records::load(&dir)?.population("abstention_rate")?,
        original
    );
    let mut policy = header.clone();
    policy["verdict_projection"] = json!("different scoring projection");
    fixture_json(&header_path, &policy)?;
    let changed = population::Records::load(&dir)?.population("abstention_rate")?;
    assert!(
        original
            .cases
            .iter()
            .zip(&changed.cases)
            .all(|(a, b)| a.definition_sha256 != b.definition_sha256)
    );
    fixture_json(&header_path, &header)?;
    let corpus = super::super::verdict_corpus::load_corpus(&dir)?;
    let case = corpus.cases.first().ok_or("case missing")?;
    let diff_path = dir.join(&case.diff);
    let diff = fs::read_to_string(&diff_path).map_err(|err| err.to_string())?;
    fs::write(&diff_path, format!("{diff}\n")).map_err(|err| err.to_string())?;
    let changed = population::Records::load(&dir)?.population("abstention_rate")?;
    assert!(
        original
            .cases
            .iter()
            .zip(&changed.cases)
            .any(|(a, b)| a.definition_sha256 != b.definition_sha256)
    );
    fs::write(&diff_path, &diff).map_err(|err| err.to_string())?;
    let case = corpus
        .cases
        .iter()
        .find(|case| case.truth.state == super::super::verdict_corpus::TruthState::Discriminated)
        .ok_or("discriminated case missing")?;
    let case_path = dir.join("cases").join(format!("{}.json", case.case_id));
    let mut definition = read_json(&case_path)?;
    definition["truth"]["state"] = json!("not_discriminated");
    for mutant in definition["truth"]["mutants"]
        .as_array_mut()
        .ok_or("mutants missing")?
    {
        mutant["outcome"] = json!("tests_passed");
        mutant["failing_test"] = Value::Null;
    }
    definition["expected"] =
        json!({"ideal_verdict":"gap", "acceptable_verdicts":["gap","limited"]});
    fixture_json(&case_path, &definition)?;
    let row_path = dir
        .join("expected/rows")
        .join(format!("{}.json", case.case_id));
    let mut row = read_json(&row_path)?;
    row["truth"] = json!("not_discriminated");
    row["ideal_verdict"] = json!("gap");
    row["observed_verdict"] = json!("gap");
    row["outcome"] = json!("ideal");
    fixture_json(&row_path, &row)?;
    let changed = population::Records::load(&dir)?.population("abstention_rate")?;
    assert!(
        original
            .cases
            .iter()
            .zip(&changed.cases)
            .any(|(a, b)| a.case_id == case.case_id && a.definition_sha256 != b.definition_sha256)
    );
    fs::remove_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(())
}

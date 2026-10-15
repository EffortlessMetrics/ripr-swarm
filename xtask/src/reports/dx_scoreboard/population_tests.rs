use super::population::{Contribution, Population, SCHEMA};
use super::*;
use std::collections::BTreeSet;

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
    report_config(config()?, base, current)
}

fn report_config(
    mut config: Config,
    base: Population,
    current: Population,
) -> Result<Value, String> {
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
fn digest_case_cannot_exclude_worsening_common_cases() -> Result<(), String> {
    let mut baseline = cases(100, 0);
    for case in &mut baseline.cases[..3] {
        case.definition_sha256 = case.definition_sha256.to_ascii_uppercase();
    }
    let report = report(baseline, cases(100, 3))?;
    assert_eq!(report["gate"]["status"], "fail");
    assert_eq!(
        report["metrics"][0]["baseline"]["common"]["cases_total"],
        100
    );
    assert_eq!(
        report["metrics"][0]["baseline"]["changed_definitions"],
        json!([])
    );
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

#[test]
fn perl_projection_keeps_producer_and_edited_contributions_separate() -> Result<(), String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/perl-verdict-corpus");
    let records = population::Records::load(&dir)?;
    // Independently counted committed rows: producer 27 (15 discriminated,
    // 12 not fully), edited 9. Do not derive these expectations via summarize.
    for (rate, expected) in [
        ("false_verdict_rate", (8, 27)),
        ("false_actionable_rate", (5, 15)),
        ("false_exposed_rate", (3, 12)),
        ("false_silent_rate", (0, 12)),
        ("abstention_rate", (8, 27)),
    ] {
        let selected = records.population(&format!("by_packet_provenance.producer.{rate}"))?;
        assert_eq!(selected.totals()?, expected);
        assert_eq!(selected.cases.len(), 36);
        assert_eq!(
            selected
                .cases
                .iter()
                .filter(|case| case.denominator == 0)
                .count(),
            if rate == "false_actionable_rate" {
                21
            } else if rate == "false_exposed_rate" || rate == "false_silent_rate" {
                24
            } else {
                9
            }
        );
    }
    assert_eq!(records.population("abstention_rate")?.totals()?, (11, 36));
    assert_eq!(
        records
            .population("by_packet_provenance.edited_producer.abstention_rate")?
            .totals()?,
        (3, 9)
    );
    for invalid in [
        "by_packet_provenance.producer",
        "by_packet_provenance..abstention_rate",
        "by_packet_provenance.missing.abstention_rate",
        "by_packet_provenance.producer.missing_rate",
        "by_packet_provenance.producer.contradiction_rate",
        "by_packet_provenance.producer.abstention_rate.extra",
    ] {
        assert!(records.population(invalid).is_err(), "{invalid}");
    }
    let rust = population::Records::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/rust-verdict-corpus"),
    )?;
    assert!(
        rust.population("by_packet_provenance.producer.abstention_rate")
            .is_err_and(|err| err.contains("missing corpus rate"))
    );
    Ok(())
}

fn perl_fixture() -> Result<PathBuf, String> {
    // The corpus owner derives the language from this exact directory name.
    let dir = crate::tests::temp_dir("dx-population-perl").join("perl-verdict-corpus");
    crate::tests::copy_dir_recursive(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/perl-verdict-corpus"),
        &dir,
    )?;
    population::Records::load(&dir)?;
    Ok(dir)
}

#[test]
fn perl_packet_and_producer_are_scoring_identity() -> Result<(), String> {
    let dir = perl_fixture()?;
    let original = population::Records::load(&dir)?.population("abstention_rate")?;
    let corpus = super::super::verdict_corpus::load_corpus(&dir)?;
    let case = corpus
        .cases
        .iter()
        .find(|case| {
            case.perl_facts.as_ref().is_some_and(|facts| {
                facts.provenance == super::super::verdict_corpus::PacketProvenance::Producer
            })
        })
        .ok_or("producer case missing")?;
    let facts = case.perl_facts.as_ref().ok_or("packet missing")?;
    let case_path = dir.join("cases").join(format!("{}.json", case.case_id));
    let definition = read_json(&case_path)?;
    let packet_path = dir.join(&facts.packet);
    let bytes = fs::read(&packet_path).map_err(|err| err.to_string())?;
    let mut altered = bytes.clone();
    altered.push(b'\n');
    fs::write(&packet_path, &altered).map_err(|err| err.to_string())?;
    assert!(population::Records::load(&dir).is_err_and(|err| err.contains("sha256")));
    let mut repinned = definition.clone();
    repinned["perl_facts"]["sha256"] = json!(super::super::verdict_corpus::sha256_hex(&altered));
    fixture_json(&case_path, &repinned)?;
    let changed = population::Records::load(&dir)?.population("abstention_rate")?;
    assert!(
        original
            .cases
            .iter()
            .zip(&changed.cases)
            .any(|(a, b)| a.case_id == case.case_id && a.definition_sha256 != b.definition_sha256)
    );
    fs::write(&packet_path, &bytes).map_err(|err| err.to_string())?;
    fixture_json(&case_path, &definition)?;
    let header_path = dir.join("corpus.json");
    let header = read_json(&header_path)?;
    let mut annotation = header.clone();
    annotation["description"] = json!("annotation only");
    fixture_json(&header_path, &annotation)?;
    assert_eq!(
        population::Records::load(&dir)?.population("abstention_rate")?,
        original
    );
    let mut producer = header.clone();
    producer["fact_producer"]["commit"] = json!("a".repeat(40));
    fixture_json(&header_path, &producer)?;
    let changed = population::Records::load(&dir)?.population("abstention_rate")?;
    assert!(
        original
            .cases
            .iter()
            .zip(&changed.cases)
            .all(|(a, b)| a.definition_sha256 != b.definition_sha256)
    );
    producer["fact_producer"]["commit"] = json!("invalid");
    fixture_json(&header_path, &producer)?;
    assert!(population::Records::load(&dir).is_err_and(|err| err.contains("fact_producer.commit")));
    fs::remove_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(())
}

#[test]
fn perl_row_provenance_must_match_the_packet_definition() -> Result<(), String> {
    let dir = perl_fixture()?;
    let corpus = super::super::verdict_corpus::load_corpus(&dir)?;
    let case = corpus
        .cases
        .iter()
        .find(|case| {
            case.perl_facts.as_ref().is_some_and(|facts| {
                facts.provenance == super::super::verdict_corpus::PacketProvenance::Producer
            })
        })
        .ok_or("producer case missing")?;
    let row_path = dir
        .join("expected/rows")
        .join(format!("{}.json", case.case_id));
    let original = read_json(&row_path)?;
    let mut wrong = original.clone();
    wrong["packet_provenance"] = json!("edited_producer");
    fixture_json(&row_path, &wrong)?;
    assert!(
        population::Records::load(&dir)
            .is_err_and(|err| err.contains("disagrees with its scoring definition"))
    );
    let mut missing = original.clone();
    missing
        .as_object_mut()
        .ok_or("row missing")?
        .remove("packet_provenance");
    fixture_json(&row_path, &missing)?;
    assert!(
        population::Records::load(&dir)
            .is_err_and(|err| err.contains("disagrees with its scoring definition"))
    );
    fixture_json(&row_path, &original)?;
    let before = population::Records::load(&dir)?.population("abstention_rate")?;
    let case_path = dir.join("cases").join(format!("{}.json", case.case_id));
    let mut changed = read_json(&case_path)?;
    changed["perl_facts"]["provenance"] = json!("edited_producer");
    changed["perl_facts"]["edits"] = json!("provenance changed for the identity control");
    fixture_json(&case_path, &changed)?;
    fixture_json(&row_path, &wrong)?;
    let after = population::Records::load(&dir)?.population("abstention_rate")?;
    assert!(
        before
            .cases
            .iter()
            .zip(&after.cases)
            .any(|(a, b)| a.case_id == case.case_id && a.definition_sha256 != b.definition_sha256)
    );
    fs::remove_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(())
}

#[test]
fn perl_scoreboard_measures_without_blessing_an_absent_baseline() -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut config = parse_config(
        &fs::read_to_string(root.join("benchmarks/dx_scoreboard/scoreboards.toml"))
            .map_err(|err| err.to_string())?,
    )?;
    config
        .metric
        .retain(|metric| metric.id.starts_with("trust.perl_verdict_corpus_"));
    assert_eq!(config.metric.len(), 5);
    for metric in &mut config.metric {
        metric.source = metric.source.replacen(
            "verdict-corpus:fixtures/",
            &format!("verdict-corpus:{}/fixtures/", root.display()),
            1,
        );
    }
    let boards = vec!["trust".to_string()];
    let samples = file_samples(&config, &boards)?;
    let current = build_report(
        &config,
        &boards,
        &samples,
        &RunContext::default(),
        None,
        true,
    );
    assert_eq!(current["gate"]["status"], "no_baseline");
    assert_eq!(current["gate"]["failed_instruments"], json!([]));
    for (metric, n, d, status) in [
        ("false_verdict", 8_u32, 27_u32, "below_target"),
        ("false_actionable", 5, 15, "below_target"),
        ("false_exposed", 3, 12, "below_target"),
        ("false_silent", 0, 12, "meets_target"),
        ("abstention", 8, 27, "below_target"),
    ] {
        let id = format!("trust.perl_verdict_corpus_{metric}");
        let row = current["metrics"]
            .as_array()
            .ok_or("metrics missing")?
            .iter()
            .find(|row| row["id"] == id)
            .ok_or("Perl metric missing")?;
        assert_eq!(row["value"], json!(round(f64::from(n) / f64::from(d))));
        assert_eq!(row["status"], status);
        let population: Population =
            serde_json::from_value(row["samples"][0]["population"].clone())
                .map_err(|err| err.to_string())?;
        assert_eq!(population.totals()?, (u64::from(n), u64::from(d)));
    }
    let baseline: Value = serde_json::from_str(include_str!(
        "../../../../benchmarks/dx_scoreboard/testdata/7552-original-baseline-with-population.json"
    ))
    .map_err(|err| err.to_string())?;
    let compared = build_report(
        &config,
        &boards,
        &samples,
        &RunContext::default(),
        Some(&baseline),
        true,
    );
    assert_eq!(compared["gate"]["status"], "fail");
    assert_eq!(compared["gate"]["failed_instruments"], json!([]));
    assert_eq!(
        compared["gate"]["failed_comparisons"]
            .as_array()
            .map(Vec::len),
        Some(5)
    );
    let failure = gate_failure_message(&compared);
    assert!(failure.contains("trust.perl_verdict_corpus_abstention"));
    assert!(failure.contains("metric absent from baseline; case identity evidence is required"));
    Ok(())
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

fn oracle_fixture() -> Result<PathBuf, String> {
    let dir = scoring_fixture()?;
    let corpus = super::super::verdict_corpus::load_corpus(&dir)?;
    assert_eq!(corpus.cases.len(), 4);
    for (i, case) in corpus.cases.iter().enumerate() {
        let case_path = dir.join("cases").join(format!("{}.json", case.case_id));
        let row_path = dir
            .join("expected/rows")
            .join(format!("{}.json", case.case_id));
        let mut definition = read_json(&case_path)?;
        let mut row = read_json(&row_path)?;
        definition
            .as_object_mut()
            .ok_or("case object missing")?
            .remove("expected_oracle");
        row.as_object_mut()
            .ok_or("row object missing")?
            .remove("oracle");
        if i < 3 {
            let expected = json!({"kind":"exact_value", "strength":if i == 2 {"medium"} else {"strong"}, "test":"fixture_test", "spec":"RIPR-SPEC-0227#1"});
            definition["expected_oracle"] = expected.clone();
            row["oracle"] = json!({"expected":expected, "observed_kind":"exact_value", "observed_strength":"strong", "observed_all":[{"test":"fixture_test", "kind":"exact_value", "strength":"strong"}], "outcome":if i == 2 {"overclaim"} else {"match"}});
        }
        fixture_json(&case_path, &definition)?;
        fixture_json(&row_path, &row)?;
    }
    Ok(dir)
}

#[test]
fn oracle_committed_rates_keep_labeled_and_unlabeled_cases() -> Result<(), String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/rust-verdict-corpus");
    let expected_ids: BTreeSet<_> = super::super::verdict_corpus::load_corpus(&dir)?
        .cases
        .into_iter()
        .map(|case| case.case_id)
        .collect();
    assert!(
        expected_ids.len() > 43,
        "control needs unlabeled source cases"
    );
    let records = population::Records::load(&dir)?;
    for (rate, numerator) in [("oracle_accuracy", 43), ("oracle_overclaim_rate", 0)] {
        let population = records.population(&format!("oracle_accuracy.{rate}"))?;
        assert_eq!(population.totals()?, (numerator, 43));
        assert_eq!(
            population
                .cases
                .iter()
                .map(|case| case.case_id.clone())
                .collect::<BTreeSet<_>>(),
            expected_ids
        );
        assert_eq!(
            population
                .cases
                .iter()
                .filter(|case| case.denominator == 0)
                .count(),
            expected_ids.len() - 43
        );
        assert_eq!(
            population
                .cases
                .iter()
                .filter(|c| c.denominator == 1)
                .count(),
            43
        );
        assert!(
            population
                .cases
                .iter()
                .filter(|c| c.denominator == 0)
                .all(|c| c.numerator == 0)
        );
    }
    Ok(())
}

#[test]
fn oracle_rows_refuse_missing_labels_and_fabricated_claims() -> Result<(), String> {
    let dir = oracle_fixture()?;
    let corpus = super::super::verdict_corpus::load_corpus(&dir)?;
    let case = corpus.cases.first().ok_or("oracle case missing")?;
    let row_path = dir
        .join("expected/rows")
        .join(format!("{}.json", case.case_id));
    let original = read_json(&row_path)?;
    // Setup must be valid before a refusal can establish the discriminator.
    assert_eq!(
        population::Records::load(&dir)?
            .population("abstention_rate")?
            .cases
            .len(),
        4
    );
    for (pointer, replacement) in [
        ("/oracle", Value::Null),
        ("/oracle/expected/kind", json!("exact_error_variant")),
        ("/oracle/expected/strength", json!("medium")),
        ("/oracle/expected/test", json!("another_test")),
        ("/oracle/expected/spec", json!("RIPR-SPEC-0227#2")),
        ("/oracle/outcome", json!("overclaim")),
        ("/oracle/observed_kind", json!("exact_error_variant")),
        ("/oracle/observed_strength", json!("medium")),
        ("/oracle/observed_all", json!([])),
        ("/oracle/observed_all/0/test", json!("another_test")),
    ] {
        let mut wrong = original.clone();
        *wrong
            .pointer_mut(pointer)
            .ok_or("oracle fixture pointer missing")? = replacement;
        fixture_json(&row_path, &wrong)?;
        assert!(
            population::Records::load(&dir).is_err_and(|err| err.contains("oracle")),
            "accepted {pointer}"
        );
    }
    fixture_json(&row_path, &original)?;
    let case_path = dir.join("cases").join(format!("{}.json", case.case_id));
    let mut definition = read_json(&case_path)?;
    definition
        .as_object_mut()
        .ok_or("case object missing")?
        .remove("expected_oracle");
    fixture_json(&case_path, &definition)?;
    assert!(population::Records::load(&dir).is_err_and(|err| err.contains("oracle")));
    fs::remove_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(())
}

#[test]
fn oracle_labels_change_only_the_oracle_scoring_identity() -> Result<(), String> {
    let dir = oracle_fixture()?;
    let records = population::Records::load(&dir)?;
    let verdict_before = records.population("abstention_rate")?;
    let oracle_before = records.population("oracle_accuracy.oracle_accuracy")?;
    assert_eq!(oracle_before.totals()?, (2, 3));
    assert_eq!(
        records
            .population("oracle_accuracy.oracle_overclaim_rate")?
            .totals()?,
        (1, 3)
    );
    let corpus = super::super::verdict_corpus::load_corpus(&dir)?;
    let case = corpus.cases.first().ok_or("oracle case missing")?;
    let case_path = dir.join("cases").join(format!("{}.json", case.case_id));
    let row_path = dir
        .join("expected/rows")
        .join(format!("{}.json", case.case_id));
    let original_case = read_json(&case_path)?;
    let original_row = read_json(&row_path)?;
    for (field, replacement) in [
        ("kind", "exact_error_variant"),
        ("strength", "medium"),
        ("test", "other_fixture_test"),
        ("spec", "RIPR-SPEC-0227#2"),
    ] {
        let mut definition = original_case.clone();
        let mut row = original_row.clone();
        definition["expected_oracle"][field] = json!(replacement);
        row["oracle"]["expected"][field] = json!(replacement);
        match field {
            "kind" => {
                row["oracle"]["observed_kind"] = json!(replacement);
                row["oracle"]["observed_all"][0]["kind"] = json!(replacement);
            }
            "strength" => {
                row["oracle"]["observed_strength"] = json!(replacement);
                row["oracle"]["observed_all"][0]["strength"] = json!(replacement);
            }
            "test" => row["oracle"]["observed_all"][0]["test"] = json!(replacement),
            _ => {}
        }
        fixture_json(&case_path, &definition)?;
        fixture_json(&row_path, &row)?;
        let records = population::Records::load(&dir)?;
        assert_eq!(records.population("abstention_rate")?, verdict_before);
        let current = records.population("oracle_accuracy.oracle_accuracy")?;
        assert_eq!(current.totals()?, (2, 3));
        let changes = current
            .cases
            .iter()
            .zip(&oracle_before.cases)
            .filter(|(a, b)| a.definition_sha256 != b.definition_sha256)
            .count();
        assert_eq!(changes, usize::from(field != "spec"));
    }
    let mut definition = original_case;
    let mut row = original_row;
    definition
        .as_object_mut()
        .ok_or("case object missing")?
        .remove("expected_oracle");
    row.as_object_mut()
        .ok_or("row object missing")?
        .remove("oracle");
    fixture_json(&case_path, &definition)?;
    fixture_json(&row_path, &row)?;
    let records = population::Records::load(&dir)?;
    assert_eq!(records.population("abstention_rate")?, verdict_before);
    let current = records.population("oracle_accuracy.oracle_accuracy")?;
    assert_eq!(current.totals()?, (1, 2));
    let compared = report_config(config()?, oracle_before, current)?;
    assert_eq!(
        compared["metrics"][0]["baseline"]["changed_definitions"],
        json!([case.case_id])
    );
    for case in &corpus.cases {
        let case_path = dir.join("cases").join(format!("{}.json", case.case_id));
        let row_path = dir
            .join("expected/rows")
            .join(format!("{}.json", case.case_id));
        let mut definition = read_json(&case_path)?;
        let mut row = read_json(&row_path)?;
        definition
            .as_object_mut()
            .ok_or("case object missing")?
            .remove("expected_oracle");
        row.as_object_mut()
            .ok_or("row object missing")?
            .remove("oracle");
        fixture_json(&case_path, &definition)?;
        fixture_json(&row_path, &row)?;
    }
    let records = population::Records::load(&dir)?;
    assert_eq!(records.population("abstention_rate")?, verdict_before);
    assert!(
        records
            .population("oracle_accuracy.oracle_accuracy")
            .is_err_and(|err| err.contains("missing corpus rate"))
    );
    assert!(
        records
            .population("oracle_accuracy.oracle_overclaim_rate")
            .is_err_and(|err| err.contains("missing corpus rate"))
    );
    fs::remove_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(())
}

#[test]
fn oracle_additions_cannot_create_or_hide_retained_regression() -> Result<(), String> {
    let mut config = config()?;
    config.metric[0].direction = "higher_is_better".to_string();
    config.metric[0].target = 1.0;
    config.metric[0].regression_floor = 0.0;
    let oracle = |mut population: Population| {
        population.rate = "oracle_accuracy.oracle_accuracy".to_string();
        population
    };
    let unchanged = report_config(config.clone(), oracle(cases(4, 3)), oracle(cases(8, 3)))?;
    assert_eq!(unchanged["metrics"][0]["value"], json!(0.375));
    assert_eq!(unchanged["metrics"][0]["status"], "below_target");
    assert_eq!(unchanged["gate"]["status"], "pass");
    let mut current = cases(20, 2);
    for case in &mut current.cases[4..] {
        case.numerator = 1;
    }
    let regressed = report_config(config, oracle(cases(4, 3)), oracle(current))?;
    assert_eq!(regressed["metrics"][0]["value"], json!(0.9));
    assert_eq!(
        regressed["metrics"][0]["baseline"]["common"]["current"]["numerator"],
        2
    );
    assert_eq!(regressed["gate"]["status"], "fail");
    let dir = oracle_fixture()?;
    let records = population::Records::load(&dir)?;
    for rate in [
        "oracle_accuracy",
        "oracle_accuracy.unknown",
        "oracle_accuracy.oracle_accuracy.extra",
        "by_packet_provenance.producer.oracle_accuracy",
    ] {
        assert!(
            records
                .population(rate)
                .is_err_and(|err| err.contains("unsupported case rate"))
        );
    }
    fs::remove_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(())
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

#[test]
fn exact_margin_passes_and_one_more_retained_case_fails() -> Result<(), String> {
    assert_eq!(
        report(cases(50, 3), cases(50, 4))?["gate"]["status"],
        "pass"
    );
    assert_eq!(
        report(cases(50, 3), cases(50, 5))?["gate"]["status"],
        "fail"
    );
    assert_eq!(
        report(cases(50, 0), cases(50, 1))?["gate"]["status"],
        "pass"
    );
    assert_eq!(
        report(cases(50, 0), cases(50, 2))?["gate"]["status"],
        "fail"
    );
    Ok(())
}

#[test]
fn custom_margin_boundary_passes_and_next_case_fails() -> Result<(), String> {
    let mut config = config()?;
    config.metric[0].regression_floor = 0.072;
    assert_eq!(
        report_config(config.clone(), cases(625, 0), cases(625, 45))?["gate"]["status"],
        "pass"
    );
    assert_eq!(
        report_config(config, cases(625, 0), cases(625, 46))?["gate"]["status"],
        "fail"
    );
    Ok(())
}

#[test]
fn relative_and_additive_margin_boundaries_pass_and_next_case_fails() -> Result<(), String> {
    for (direction, count, before, boundary, adverse, pct, floor, additive) in [
        ("lower_is_better", 105, 100, 101, 102, 1.0, 0.0, false),
        ("lower_is_better", 10, 6, 7, 8, 15.0, 0.01, true),
        ("higher_is_better", 105, 100, 99, 98, 1.0, 0.0, false),
        ("higher_is_better", 10, 6, 5, 4, 15.0, 0.01, true),
        ("higher_is_better", 625, 46, 1, 0, 0.0, 0.072, false),
    ] {
        let mut config = config()?;
        config.metric[0].direction = direction.to_string();
        config.metric[0].regression_pct = pct;
        config.metric[0].regression_floor = floor;
        config.metric[0].regression_additive = additive;
        assert_eq!(
            report_config(config.clone(), cases(count, before), cases(count, boundary))?["gate"]["status"],
            "pass"
        );
        assert_eq!(
            report_config(config, cases(count, before), cases(count, adverse))?["gate"]["status"],
            "fail"
        );
    }
    Ok(())
}

#[test]
fn finite_margin_extremes_work_and_nonfinite_margin_fails_clearly() -> Result<(), String> {
    for (floor, current, status) in [
        (-0.0, 0, "pass"),
        (f64::MAX, 1, "pass"),
        (f64::MIN_POSITIVE, 1, "fail"),
    ] {
        let mut config = config()?;
        config.metric[0].regression_floor = floor;
        assert_eq!(
            report_config(config, cases(1, 0), cases(1, current))?["gate"]["status"],
            status
        );
    }
    let mut config = config()?;
    config.metric[0].regression_floor = f64::INFINITY;
    let report = report_config(config, cases(1, 0), cases(1, 0))?;
    assert_eq!(report["gate"]["status"], "fail");
    assert!(gate_failure_message(&report).contains("finite nonnegative margins"));
    Ok(())
}

#[test]
fn changed_eligibility_without_a_changed_definition_is_incomparable() -> Result<(), String> {
    let mut base = cases(4, 1);
    base.rate = "false_actionable_rate".to_string();
    base.cases[3].denominator = 0;
    let mut current = base.clone();
    current.cases[3].denominator = 1;
    let report = report(base, current)?;
    assert_eq!(report["gate"]["status"], "fail");
    assert!(
        gate_failure_message(&report).contains("eligible denominator changed for unchanged case")
    );
    Ok(())
}

/// #7325: the owner accepted the retained Rust abstention rise, so the five
/// Rust corpus entries are a refreshed observation. Every other byte of the
/// operational baseline still matches the #7552 witness, and each refreshed
/// sample names the revision and committed rows its population came from.
#[test]
fn operational_baseline_refreshes_only_the_five_rust_rates_with_provenance() -> Result<(), String> {
    let mut live: Value = serde_json::from_str(include_str!(
        "../../../../metrics/dx-scoreboard/baseline.json"
    ))
    .map_err(|err| err.to_string())?;
    let mut witness: Value = serde_json::from_str(include_str!(
        "../../../../benchmarks/dx_scoreboard/testdata/7552-original-baseline-with-population.json"
    ))
    .map_err(|err| err.to_string())?;
    let refreshed = [
        ("trust.false_verdict_rate", "false_verdict_rate", 29, 477),
        (
            "trust.verdict_corpus_false_actionable",
            "false_actionable_rate",
            26,
            236,
        ),
        (
            "trust.verdict_corpus_abstention",
            "abstention_rate",
            177,
            477,
        ),
        (
            "trust.verdict_corpus_false_silent",
            "false_silent_rate",
            0,
            241,
        ),
        (
            "trust.verdict_corpus_false_exposed",
            "false_exposed_rate",
            3,
            241,
        ),
    ];
    for (id, rate, n, d) in refreshed {
        let find = |report: &mut Value| -> Result<Value, String> {
            let metrics = report["metrics"].as_array_mut().ok_or("metrics missing")?;
            let at = metrics
                .iter()
                .position(|metric| metric["id"] == id)
                .ok_or("refreshed metric missing")?;
            Ok(metrics.remove(at))
        };
        let metric = find(&mut live)?;
        let original = find(&mut witness)?;
        assert_eq!(
            metric["source"],
            format!("verdict-corpus:fixtures/rust-verdict-corpus#{rate}")
        );
        let sample = &metric["samples"][0];
        assert_eq!(metric["samples"].as_array().map(Vec::len), Some(1));
        let population: Population =
            serde_json::from_value(sample["population"].clone()).map_err(|err| err.to_string())?;
        assert_eq!(population.rate, rate);
        assert_eq!(population.totals()?, (n, d));
        let provenance = &sample["population_provenance"];
        assert_eq!(provenance["kind"], "committed_expected_rows");
        assert_eq!(
            provenance["revision"],
            "2af3f38a1de75ca48d5d84555a839d50710159b6"
        );
        assert_eq!(
            provenance["rows_tree"],
            "e7f8f98f10d0d368e3d07fa2fdd86a8bad87ac22"
        );
        // Only value, status, source and samples moved.
        for key in [
            "board",
            "direction",
            "id",
            "target",
            "title",
            "unit",
            "runner_dependent",
        ] {
            assert_eq!(metric[key], original[key], "{id} {key}");
        }
    }
    assert_eq!(live, witness);
    assert_eq!(
        include_str!("../../../../metrics/dx-scoreboard/baseline.json"),
        include_str!("../../../../metrics/public-proof/dx-scoreboard.json")
    );
    Ok(())
}

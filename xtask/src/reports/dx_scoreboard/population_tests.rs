use super::*;
use super::population::{Contribution, Population, SCHEMA};

fn config() -> Result<Config, String> {
    parse_config(r#"
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
"#)
}

fn cases(count: usize, abstentions: usize) -> Population {
    Population {
        schema_version: SCHEMA.to_string(),
        rate: "abstention_rate".to_string(),
        cases: (0..count).map(|i| Contribution {
            case_id: format!("case-{i}"),
            definition_sha256: "a".repeat(64),
            numerator: u64::from(i < abstentions),
            denominator: 1,
        }).collect(),
    }
}

fn report(base: Population, current: Population) -> Result<Value, String> {
    let config = config()?;
    let boards = vec!["trust".to_string()];
    let sample = |population| Sample {
        metric: "trust.abstention".to_string(), repo: None,
        outcome: SampleOutcome::Corpus(population), detail: "committed records".to_string(),
    };
    let context = RunContext::default();
    let baseline = build_report(&config, &boards, &[sample(base)], &context, None, false);
    Ok(build_report(&config, &boards, &[sample(current)], &context, Some(&baseline), true))
}

#[test]
fn added_abstentions_do_not_regress_unchanged_common_cases() -> Result<(), String> {
    let mut current = cases(8, 1);
    for case in &mut current.cases[4..] { case.numerator = 1; }
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

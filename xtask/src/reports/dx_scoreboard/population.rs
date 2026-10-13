//! Case populations for committed verdict rates. These are record comparisons,
//! not fresh analyzer or runtime mutation measurements.

use super::super::verdict_corpus::{self, CaseRow};
use super::{MetricDef, allowed_worsening, round};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::{fs, path::Path};

pub(super) const SCHEMA: &str = "ripr-verdict-case-population-v1";

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Population {
    pub(super) schema_version: String,
    pub(super) rate: String,
    pub(super) cases: Vec<Contribution>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Contribution {
    pub(super) case_id: String,
    pub(super) definition_sha256: String,
    pub(super) numerator: u64,
    pub(super) denominator: u64,
}

impl Population {
    pub(super) fn totals(&self) -> Result<(u64, u64), String> {
        if self.schema_version != SCHEMA || self.cases.is_empty() {
            return Err("missing or unsupported case population evidence".to_string());
        }
        let mut ids = BTreeSet::new();
        let (mut numerator, mut denominator) = (0_u64, 0_u64);
        for case in &self.cases {
            let denominator_valid = match self.rate.as_str() {
                "abstention_rate" | "ideal_rate" | "false_verdict_rate" => case.denominator == 1,
                "false_actionable_rate" | "false_exposed_rate" | "false_silent_rate" => {
                    case.denominator <= 1
                }
                "contradiction_rate" => true,
                _ => return Err(format!("unsupported case rate `{}`", self.rate)),
            };
            if case.case_id.trim().is_empty() || !ids.insert(&case.case_id) {
                return Err(format!("missing or duplicate case id `{}`", case.case_id));
            }
            if case.definition_sha256.len() != 64
                || !case
                    .definition_sha256
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit())
                || case.numerator > case.denominator
                || !denominator_valid
            {
                return Err(format!(
                    "invalid definition or contribution for `{}`",
                    case.case_id
                ));
            }
            numerator = numerator
                .checked_add(case.numerator)
                .ok_or("numerator overflow")?;
            denominator = denominator
                .checked_add(case.denominator)
                .ok_or("denominator overflow")?;
        }
        Ok((numerator, denominator))
    }

    pub(super) fn value(&self) -> Option<f64> {
        let (n, d) = self.totals().ok()?;
        (d > 0).then(|| n as f64 / d as f64)
    }
}

/// Loaded once per corpus during a scoreboard run. Definition validation and
/// byte hashing are shared by all rates over that corpus.
pub(super) struct Records {
    rows: Vec<(CaseRow, String)>,
    report: Value,
}

impl Records {
    pub(super) fn load(dir: &Path) -> Result<Self, String> {
        let corpus = verdict_corpus::load_corpus(dir)?;
        let violations = verdict_corpus::validate(&corpus, dir);
        if !violations.is_empty() {
            return Err(format!(
                "invalid verdict corpus {}: {}",
                dir.display(),
                violations.join("; ")
            ));
        }
        let report = verdict_corpus::expected_report(dir)?;
        let report_json = serde_json::to_value(&report).map_err(|err| err.to_string())?;
        let mut subjects = BTreeMap::new();
        for subject in &corpus.subjects {
            let mut files = Vec::new();
            for file in &subject.retained_files {
                let path = dir
                    .join("subjects")
                    .join(&subject.subject_id)
                    .join(verdict_corpus::stored_path(&file.path));
                let bytes =
                    fs::read(&path).map_err(|err| format!("read {}: {err}", path.display()))?;
                let hash = verdict_corpus::sha256_hex(&bytes);
                if hash != file.sha256 {
                    return Err(format!(
                        "retained file {} does not match its sha256",
                        path.display()
                    ));
                }
                files.push(json!({"path": file.path, "sha256": hash}));
            }
            files.sort_by_key(|file| file["path"].as_str().unwrap_or_default().to_string());
            subjects.insert(
                subject.subject_id.as_str(),
                (
                    subject,
                    json!({
                        "origin": subject.origin.as_str(), "upstream": subject.upstream,
                        "commit": subject.commit, "files": files,
                    }),
                ),
            );
        }
        let cases: BTreeMap<_, _> = corpus
            .cases
            .iter()
            .map(|case| (case.case_id.as_str(), case))
            .collect();
        let mut rows = Vec::new();
        for row in report.rows {
            let case = cases
                .get(row.case_id.as_str())
                .ok_or_else(|| format!("row {} has no case", row.case_id))?;
            let (subject, identity) = subjects
                .get(case.subject_id.as_str())
                .ok_or_else(|| format!("case {} has no subject", case.case_id))?;
            if row.subject_id != case.subject_id
                || row.truth != case.truth.state
                || row.origin != subject.origin
                || row.anchor != format!("{}:{}", case.anchor.file, case.anchor.line)
                || row.ideal_verdict != case.expected.ideal_verdict
                || row.outcome != verdict_corpus::score(case.truth.state, row.observed_verdict)
            {
                return Err(format!(
                    "row {} disagrees with its scoring definition",
                    row.case_id
                ));
            }
            let diff_path = dir.join(&case.diff);
            let diff = fs::read(&diff_path)
                .map_err(|err| format!("read {}: {err}", diff_path.display()))?;
            let mut acceptable: Vec<_> = case
                .expected
                .acceptable_verdicts
                .iter()
                .map(|v| json!(v))
                .collect();
            acceptable.sort_by_key(Value::to_string);
            // Replay scaffolding and labeling annotations are provenance, not
            // scoring semantics. In particular legacy mutants lack mutated_line.
            let mut mutants: Vec<_> = case
                .truth
                .mutants
                .iter()
                .map(|m| json!({"replacement": m.replacement, "outcome": m.outcome}))
                .collect();
            mutants.sort_by_key(Value::to_string);
            let definition = json!({
                "schema": corpus.schema_version, "projection": corpus.verdict_projection,
                "case_id": case.case_id, "subject_id": case.subject_id,
                "anchor": case.anchor, "edit_kind": case.edit_kind,
                "truth": case.truth.state, "ideal": case.expected.ideal_verdict,
                "acceptable": acceptable, "mutants": mutants,
                "diff_sha256": verdict_corpus::sha256_hex(&diff), "subject": identity,
            });
            rows.push((
                row,
                verdict_corpus::sha256_hex(definition.to_string().as_bytes()),
            ));
        }
        Ok(Self {
            rows,
            report: report_json,
        })
    }

    pub(super) fn population(&self, rate: &str) -> Result<Population, String> {
        let mut cases = Vec::new();
        for (row, hash) in &self.rows {
            // The corpus owner supplies denominators, including zero-eligible
            // contributions and per-finding contradiction counts.
            let single = verdict_corpus::summarize(String::new(), Vec::new(), vec![row.clone()]);
            let single = serde_json::to_value(single).map_err(|err| err.to_string())?;
            let n = single[rate]["numerator"]
                .as_u64()
                .ok_or_else(|| format!("unsupported case rate `{rate}`"))?;
            let d = single[rate]["denominator"]
                .as_u64()
                .ok_or_else(|| format!("unsupported case rate `{rate}`"))?;
            cases.push(Contribution {
                case_id: row.case_id.clone(),
                definition_sha256: hash.clone(),
                numerator: n,
                denominator: d,
            });
        }
        let population = Population {
            schema_version: SCHEMA.to_string(),
            rate: rate.to_string(),
            cases,
        };
        let (n, d) = population.totals()?;
        if self.report[rate]["numerator"].as_u64() != Some(n)
            || self.report[rate]["denominator"].as_u64() != Some(d)
        {
            return Err(format!(
                "{rate} case contributions disagree with the corpus summary"
            ));
        }
        Ok(population)
    }
}

pub(super) fn is_case_rate(def: &MetricDef) -> bool {
    def.source
        .strip_prefix("verdict-corpus:")
        .is_some_and(|reference| {
            !reference
                .split_once('#')
                .is_some_and(|(_, rate)| rate.starts_with("spec_example_coverage."))
        })
}

fn read(row: &Value, rate: &str) -> Result<Population, String> {
    let samples = row["samples"]
        .as_array()
        .ok_or("missing case population sample")?;
    if samples.len() != 1 || !samples[0]["repo"].is_null() {
        return Err("missing or duplicate case population samples".to_string());
    }
    let population: Population = serde_json::from_value(samples[0]["population"].clone())
        .map_err(|err| format!("missing or invalid case population evidence: {err}; identity must come from the recorded baseline, not a refreshed rate"))?;
    population.totals()?;
    if population.rate != rate {
        return Err(format!(
            "population rate `{}` differs from `{rate}`",
            population.rate
        ));
    }
    let value = population
        .value()
        .ok_or("no eligible cases in the whole population")?;
    if samples[0]["value"].as_f64() != Some(round(value))
        || row["value"].as_f64() != Some(round(value))
    {
        return Err("case contributions disagree with the recorded value".to_string());
    }
    Ok(population)
}

pub(super) fn failure(reason: impl Into<String>) -> Value {
    json!({"comparable": false, "comparison_failed": true, "reason": reason.into()})
}

pub(super) fn compare(def: &MetricDef, row: &Value, base_row: &Value) -> Value {
    let rate = def
        .source
        .split_once('#')
        .map(|(_, rate)| rate)
        .unwrap_or_default();
    let before = match read(base_row, rate) {
        Ok(population) => population,
        Err(err) => return failure(format!("baseline: {err}")),
    };
    let now = match read(row, rate) {
        Ok(population) => population,
        Err(err) => return failure(format!("current: {err}")),
    };
    let before: BTreeMap<_, _> = before
        .cases
        .iter()
        .map(|case| (case.case_id.as_str(), case))
        .collect();
    let now: BTreeMap<_, _> = now
        .cases
        .iter()
        .map(|case| (case.case_id.as_str(), case))
        .collect();
    let mut common_before = Vec::new();
    let mut common_now = Vec::new();
    let mut changed = Vec::new();
    for (id, case) in &before {
        if let Some(current) = now.get(id) {
            if case.definition_sha256 == current.definition_sha256 {
                if rate != "contradiction_rate" && case.denominator != current.denominator {
                    return failure(format!(
                        "eligible denominator changed for unchanged case `{id}`"
                    ));
                }
                common_before.push(*case);
                common_now.push(*current);
            } else {
                changed.push(*id);
            }
        }
    }
    let additions: Vec<_> = now
        .values()
        .filter(|case| !before.contains_key(case.case_id.as_str()))
        .copied()
        .collect();
    let removals: Vec<_> = before
        .values()
        .filter(|case| !now.contains_key(case.case_id.as_str()))
        .copied()
        .collect();
    let totals = |cases: &[&Contribution]| {
        let n: u64 = cases.iter().map(|case| case.numerator).sum();
        let d: u64 = cases.iter().map(|case| case.denominator).sum();
        json!({"numerator": n, "denominator": d, "value": (d > 0).then(|| round(n as f64 / d as f64))})
    };
    let old = totals(&common_before);
    let new = totals(&common_now);
    let mut result = json!({
        "common": {"cases_total": common_now.len(), "baseline": old, "current": new},
        "new_cases": additions.iter().map(|case| &case.case_id).collect::<Vec<_>>(),
        "removed_cases": removals.iter().map(|case| &case.case_id).collect::<Vec<_>>(),
        "changed_definitions": changed,
        "added": totals(&additions), "removed": totals(&removals),
    });
    let (Some(base), Some(current)) = (old["value"].as_f64(), new["value"].as_f64()) else {
        result["comparable"] = json!(false);
        result["comparison_failed"] = json!(true);
        result["reason"] = json!(
            "no eligible unchanged common cases; additions, removals and changed definitions cannot substitute for comparison evidence"
        );
        return result;
    };
    // Gate on unrounded fractions; rounding is presentation only.
    let fraction = |v: &Value| {
        v["numerator"].as_f64().unwrap_or_default() / v["denominator"].as_f64().unwrap_or(1.0)
    };
    let allowed = allowed_worsening(def, fraction(&old));
    result["comparable"] = json!(true);
    result["value"] = json!(base);
    result["delta"] = json!(round(fraction(&new) - fraction(&old)));
    result["current"] = json!(current);
    result["allowed_worsening"] = json!(round(allowed));
    // Cross-multiply integer contributions before applying the existing
    // margins. Subtracting floating fractions falsely regresses an exactly
    // allowed change such as 3/50 -> 4/50 with a 0.02 floor.
    let sum = |cases: &[&Contribution]| -> (u128, u128) {
        (
            cases.iter().map(|case| u128::from(case.numerator)).sum(),
            cases.iter().map(|case| u128::from(case.denominator)).sum(),
        )
    };
    let ((bn, bd), (cn, cd)) = (sum(&common_before), sum(&common_now));
    let before = bn * cd;
    let after = cn * bd;
    let worsening = if def.direction == "higher_is_better" {
        before.saturating_sub(after)
    } else {
        after.saturating_sub(before)
    };
    let floor = def.regression_floor * (bd * cd) as f64;
    let percentage = def.regression_pct * before as f64 / 100.0;
    let scaled_allowed = if def.regression_additive {
        floor + percentage
    } else {
        floor.max(percentage)
    };
    result["regressed"] = json!(worsening as f64 > scaled_allowed);
    result
}

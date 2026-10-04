//! Pilot top-recommendation precision for `cargo xtask mutation-spot-check`.
//!
//! `ripr pilot` sends a developer to its top-ranked seams first, so a wrong
//! recommendation costs a wasted trip. This scores each of pilot's top
//! `PILOT_MAX_SEAMS` recommendations against the same cargo-mutants outcomes
//! the verdict scoring uses:
//!
//! - `line`: viable non-`FnValue` mutants that start on the recommended line.
//!   Any missed mutant confirms the recommendation; all caught refutes it.
//! - `owner`: when the line has none, the `FnValue` mutants of the innermost
//!   function containing the line. A missed mutant confirms; all caught refutes.
//!   Replacing the whole body is coarser than changing the seam, so the two
//!   tiers are counted separately.
//! - `unscored`: neither tier has a viable mutant.
//!
//! Precision is confirmed over confirmed plus refuted. It speaks only for the
//! recorded revisions, cargo-mutants version and this join rule.

use super::{EXPOSURE_TIMEOUT, path_arg, read_json, run_text};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

/// Recommendations scored per repository. Pilot's default is 5; ten gives
/// the scoreboard enough scored recommendations on small crates.
pub(super) const PILOT_MAX_SEAMS: usize = 10;

pub(super) const CLAIM_BOUNDARY: &str = "A pilot recommendation is confirmed when a viable mutant on its line was missed (or, with none on the line, a whole-body mutant of its innermost function was missed), refuted when every such mutant was caught, and unscored otherwise. Precision is confirmed over confirmed plus refuted, for the recorded revisions and cargo-mutants versions only.";

/// Run `ripr pilot` on `checkout` and return its ranked top seams.
pub(super) fn pilot_top_seams(
    binary: &Path,
    scratch: &Path,
    name: &str,
    checkout: &Path,
) -> Result<Vec<Value>, String> {
    let out_dir = scratch.join(format!("{name}.pilot"));
    run_text(
        &path_arg(binary),
        &[
            "pilot".to_string(),
            "--root".to_string(),
            path_arg(checkout),
            "--out".to_string(),
            path_arg(&out_dir),
            "--max-seams".to_string(),
            PILOT_MAX_SEAMS.to_string(),
            "--quiet".to_string(),
        ],
        EXPOSURE_TIMEOUT,
        "ripr pilot for spot check",
    )?;
    let summary = read_json(&out_dir.join("pilot-summary.json"))?;
    summary
        .get("top_actionable_seams")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| format!("pilot summary for `{name}` has no top_actionable_seams array"))
}

/// One viable runtime outcome with the location facts the judge needs.
struct Outcome<'a> {
    file: &'a str,
    line: u64,
    function_span: Option<(u64, u64)>,
    whole_body: bool,
    missed: bool,
}

/// Join cargo-mutants `mutants.json` (locations) to `outcomes.json`
/// (results) by mutant name, keeping only caught and missed mutants.
fn viable_outcomes<'a>(mutants: &'a Value, outcomes: &'a Value) -> Vec<Outcome<'a>> {
    let results: BTreeMap<&str, &str> = outcomes
        .get("outcomes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|record| {
            Some((
                record.pointer("/scenario/Mutant/name")?.as_str()?,
                record.get("summary")?.as_str()?,
            ))
        })
        .collect();
    mutants
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|mutant| {
            let missed = match *results.get(mutant.get("name")?.as_str()?)? {
                "MissedMutant" => true,
                "CaughtMutant" => false,
                _ => return None,
            };
            let span = |end: &str| {
                mutant
                    .pointer(&format!("/function/span/{end}/line"))
                    .and_then(Value::as_u64)
            };
            Some(Outcome {
                file: mutant.get("file")?.as_str()?,
                line: mutant.pointer("/span/start/line")?.as_u64()?,
                function_span: span("start").zip(span("end")),
                whole_body: mutant.get("genre").and_then(Value::as_str) == Some("FnValue"),
                missed,
            })
        })
        .collect()
}

/// Judge each recommendation, in pilot's rank order.
pub(super) fn judge_recommendations(
    top: &[Value],
    mutants: &Value,
    outcomes: &Value,
) -> Vec<Value> {
    let viable = viable_outcomes(mutants, outcomes);
    top.iter()
        .enumerate()
        .map(|(index, seam)| {
            let file = seam.get("file").and_then(Value::as_str).unwrap_or("");
            let line = seam.get("line").and_then(Value::as_u64).unwrap_or(0);
            let on_line = viable
                .iter()
                .filter(|o| !o.whole_body && o.file == file && o.line == line)
                .collect::<Vec<_>>();
            let (tier, joined) = if on_line.is_empty() {
                let containing = viable
                    .iter()
                    .filter(|o| o.whole_body && o.file == file)
                    .filter_map(|o| o.function_span.map(|span| (span, o)))
                    .filter(|((start, end), _)| (*start..=*end).contains(&line))
                    .collect::<Vec<_>>();
                let innermost = containing.iter().map(|((start, _), _)| *start).max();
                let owner = containing
                    .into_iter()
                    .filter(|((start, _), _)| Some(*start) == innermost)
                    .map(|(_, outcome)| outcome)
                    .collect::<Vec<_>>();
                ("owner", owner)
            } else {
                ("line", on_line)
            };
            let missed = joined.iter().filter(|o| o.missed).count();
            let caught = joined.len() - missed;
            let (verdict, tier) = match (missed, caught) {
                (0, 0) => ("unscored", "none"),
                (0, _) => ("refuted", tier),
                _ => ("confirmed", tier),
            };
            let text = |key: &str| seam.get(key).cloned().unwrap_or(Value::Null);
            json!({
                "rank": index + 1,
                "seam_id": text("seam_id"),
                "file": file,
                "line": line,
                "kind": text("kind"),
                "grip_class": text("grip_class"),
                "verdict": verdict,
                "tier": tier,
                "caught": caught,
                "missed": missed,
            })
        })
        .collect()
}

/// Pool the judged recommendations of every repository.
pub(super) fn summarize(repos: &[(String, Vec<Value>)]) -> Value {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut by_tier: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut by_class: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for (_, judged) in repos {
        for row in judged {
            let field = |key: &str| row.get(key).and_then(Value::as_str).unwrap_or("");
            let verdict = match field("verdict") {
                "confirmed" => "confirmed",
                "refuted" => "refuted",
                _ => "unscored",
            };
            *counts.entry(verdict).or_default() += 1;
            *by_tier
                .entry(field("tier").to_string())
                .or_default()
                .entry(verdict.to_string())
                .or_default() += 1;
            *by_class
                .entry(field("grip_class").to_string())
                .or_default()
                .entry(verdict.to_string())
                .or_default() += 1;
        }
    }
    let confirmed = counts.get("confirmed").copied().unwrap_or(0);
    let scored = confirmed + counts.get("refuted").copied().unwrap_or(0);
    json!({
        "max_seams_per_repo": PILOT_MAX_SEAMS,
        "claim_boundary": CLAIM_BOUNDARY,
        "recommendations_total": counts.values().sum::<usize>(),
        "scored": scored,
        "counts": counts,
        "precision": super::rate(confirmed, scored),
        "by_tier": by_tier,
        "by_grip_class": by_class,
        "repos": repos.iter().map(|(name, judged)| json!({
            "name": name,
            "recommendations": judged,
        })).collect::<Vec<_>>(),
    })
}

/// Markdown section for the pilot scoring, empty when the report has none.
pub(super) fn markdown(report: &Value) -> String {
    let Some(section) = report.get("pilot_top_recommendations") else {
        return String::new();
    };
    let count = |key: &str| {
        section
            .pointer(&format!("/counts/{key}"))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };
    let mut out = format!(
        "\n## Pilot top recommendations\n\n{CLAIM_BOUNDARY}\n\nTop {} per repository: {} confirmed, {} refuted, {} unscored; precision {}.\n\n| Repo | Rank | Seam | Kind | Grip class | Verdict | Tier | Caught | Missed |\n| --- | --- | --- | --- | --- | --- | --- | --- | --- |\n",
        section
            .get("max_seams_per_repo")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        count("confirmed"),
        count("refuted"),
        count("unscored"),
        section
            .get("precision")
            .and_then(Value::as_f64)
            .map_or_else(|| "n/a".to_string(), |rate| format!("{:.1}%", rate * 100.0)),
    );
    for repo in section
        .get("repos")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let name = repo.get("name").and_then(Value::as_str).unwrap_or("");
        for row in repo
            .get("recommendations")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let text = |key: &str| row.get(key).and_then(Value::as_str).unwrap_or("");
            let number = |key: &str| row.get(key).and_then(Value::as_u64).unwrap_or(0);
            out.push_str(&format!(
                "| {name} | {} | `{}:{}` | {} | {} | {} | {} | {} | {} |\n",
                number("rank"),
                text("file"),
                number("line"),
                text("kind"),
                text("grip_class"),
                text("verdict"),
                text("tier"),
                number("caught"),
                number("missed"),
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mutant(name: &str, line: u64, genre: &str, function: (u64, u64)) -> Value {
        json!({
            "name": name,
            "file": "src/a.rs",
            "genre": genre,
            "span": {"start": {"line": line, "column": 1}, "end": {"line": line, "column": 9}},
            "function": {"span": {"start": {"line": function.0}, "end": {"line": function.1}}},
        })
    }

    fn outcome(name: &str, summary: &str) -> Value {
        json!({"scenario": {"Mutant": {"name": name}}, "summary": summary})
    }

    fn seam(line: u64) -> Value {
        json!({"seam_id": format!("s{line}"), "file": "src/a.rs", "line": line, "kind": "call_presence", "grip_class": "weakly_gripped"})
    }

    #[test]
    fn line_mutants_decide_before_whole_body_mutants() {
        let mutants = json!([
            mutant("op-caught", 3, "BinaryOperator", (1, 9)),
            mutant("op-missed", 4, "BinaryOperator", (1, 9)),
            mutant("op-unviable", 5, "BinaryOperator", (1, 9)),
            mutant("body-missed", 2, "FnValue", (1, 9)),
        ]);
        let outcomes = json!({"outcomes": [
            outcome("op-caught", "CaughtMutant"),
            outcome("op-missed", "MissedMutant"),
            outcome("op-unviable", "Unviable"),
            outcome("body-missed", "MissedMutant"),
        ]});
        let judged = judge_recommendations(&[seam(3), seam(4), seam(5)], &mutants, &outcomes);
        let verdicts = judged
            .iter()
            .map(|row| (row["verdict"].as_str(), row["tier"].as_str()))
            .collect::<Vec<_>>();
        // A caught operator on line 3 refutes it even though the body
        // mutant was missed; line 5's only line mutant is unviable, so the
        // missed whole-body mutant decides it.
        assert_eq!(
            verdicts,
            vec![
                (Some("refuted"), Some("line")),
                (Some("confirmed"), Some("line")),
                (Some("confirmed"), Some("owner")),
            ]
        );
    }

    #[test]
    fn owner_tier_reads_only_the_innermost_function() {
        let mutants = json!([
            mutant("outer-missed", 1, "FnValue", (1, 20)),
            mutant("inner-caught", 6, "FnValue", (5, 8)),
        ]);
        let outcomes = json!({"outcomes": [
            outcome("outer-missed", "MissedMutant"),
            outcome("inner-caught", "CaughtMutant"),
        ]});
        let judged = judge_recommendations(&[seam(7), seam(12), seam(30)], &mutants, &outcomes);
        let verdicts = judged
            .iter()
            .map(|row| row["verdict"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            verdicts,
            vec![Some("refuted"), Some("confirmed"), Some("unscored")]
        );
    }

    #[test]
    fn summary_precision_excludes_unscored_recommendations() {
        let judged = vec![
            json!({"verdict": "confirmed", "tier": "line", "grip_class": "ungripped"}),
            json!({"verdict": "refuted", "tier": "owner", "grip_class": "weakly_gripped"}),
            json!({"verdict": "refuted", "tier": "line", "grip_class": "weakly_gripped"}),
            json!({"verdict": "unscored", "tier": "none", "grip_class": "ungripped"}),
        ];
        let summary = summarize(&[("a".to_string(), judged)]);
        assert_eq!(summary["scored"], 3);
        assert_eq!(summary["recommendations_total"], 4);
        assert_eq!(summary["precision"], json!(0.333));
        assert_eq!(summary["by_grip_class"]["weakly_gripped"]["refuted"], 2);
        assert!(
            markdown(&json!({"pilot_top_recommendations": summary})).contains("precision 33.3%")
        );
        assert_eq!(markdown(&json!({})), "");
    }
}

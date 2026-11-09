//! Semantic compaction for the `json-terse` rendering mode.
//!
//! `json-compact` (#13103) only removes insignificant whitespace: its parsed
//! document is byte-value identical to `json`. `json-terse` goes one layer
//! further for LLM/agent consumers that pay per token on findings:
//!
//! 1. Repeated prose templates in `evidence_path`, `evidence`, `missing` and
//!    `assertion_texts` are replaced by stable short codes (`E0001`, ...), and
//!    the full templates are emitted once in an `evidence_legend` block at the
//!    document head. Expanding the codes through the legend reconstructs the
//!    exact original strings, in order.
//! 2. Known aliases are omitted when they equal their owner:
//!    `suggested_next_action` (owner `recommended_next_step`), and the
//!    finding-level `observed_values` / `missing_discriminators` (owner: the
//!    same-named arrays inside `activation`, #13673), `canonical_gap_id` (owner
//!    `canonical_gap.id`) and basis-free `evidence_stages` (owner: the `ripr`
//!    stage objects, in reach..discriminate order). The document records each
//!    dropped alias once in `dropped_aliases`. Other fields remain verbatim even
//!    when values match.
//!
//! What terse does *not* do: it never drops a finding, never changes a
//! verdict/classification/severity/confidence, and never touches nested
//! prose outside the four coded surfaces (ripr stage summaries,
//! related-test oracle lines, repair cards stay inline). See
//! `docs/OUTPUT_SCHEMA.md` "json-terse" for the consumer-facing contract.

use serde_json::Value;
#[cfg(test)]
use std::collections::BTreeMap;
use std::collections::HashMap;

/// Prose below this UTF-8 byte threshold stays verbatim even when repeated.
const MIN_LEGEND_TEXT_LEN: usize = 24;

/// Finding-array fields whose repeated sentence templates are coded.
const PROSE_ARRAYS: [&str; 3] = ["evidence_path", "evidence", "missing"];

/// Map field whose values (assertion source lines) are coded the same way.
const ASSERTION_TEXTS: &str = "assertion_texts";

/// Marker stamped at the document head so consumers can key on the rendering
/// profile without sniffing for the legend.
const RENDERING_MODE_KEY: &str = "rendering";
const RENDERING_MODE_VALUE: &str = "terse";
const LEGEND_KEY: &str = "evidence_legend";
const DROPPED_ALIASES_KEY: &str = "dropped_aliases";
/// Every alias terse may omit, in `dropped_aliases` emission order.
const KNOWN_ALIASES: [&str; 5] = [
    "suggested_next_action",
    "observed_values",
    "missing_discriminators",
    "canonical_gap_id",
    "evidence_stages",
];
/// The `ripr` stage keys in the order `evidence_stages` rows are emitted.
const STAGE_ORDER: [&str; 5] = ["reach", "infect", "propagate", "observe", "discriminate"];
// Entry costs include a comma each. The outer comma replaces the final one,
// leaving the field's key quotes, colon and braces as shared overhead.
const LEGEND_FIELD_OVERHEAD: usize = LEGEND_KEY.len() + 5;

/// Compacts an already-rendered, stamped check JSON document (`json-terse`).
///
/// The input is this crate's own producer output, so a parse failure is a
/// production bug, not a recoverable transport state: fail closed with an
/// error rather than silently passing a full document through as "terse".
pub(crate) fn render_terse(rendered: &str) -> Result<String, String> {
    let mut document: Value = serde_json::from_str(rendered)
        .map_err(|err| format!("failed to render terse JSON: {err}"))?;
    let Value::Object(ref mut root) = document else {
        return Err("failed to render terse JSON: check document is not an object".to_string());
    };

    let mut coder = LegendCoder::default();
    if let Some(Value::Array(findings)) = root.get("findings") {
        for finding in findings.iter().filter_map(Value::as_object) {
            for key in PROSE_ARRAYS {
                if let Some(Value::Array(entries)) = finding.get(key) {
                    for text in entries.iter().filter_map(Value::as_str) {
                        coder.observe(text);
                    }
                }
            }
            if let Some(Value::Object(assertions)) = finding.get(ASSERTION_TEXTS) {
                for text in assertions.values().filter_map(Value::as_str) {
                    coder.observe(text);
                }
            }
        }
    }
    coder.select_legend()?;
    if let Some(Value::Array(findings)) = root.get_mut("findings") {
        for finding in findings.iter_mut().filter_map(Value::as_object_mut) {
            compact_finding(finding, &mut coder);
        }
    }

    if !coder.legend.is_empty() {
        let mut legend = serde_json::Map::new();
        for (code, text) in &coder.legend {
            legend.insert(code.clone(), Value::String(text.clone()));
        }
        root.insert(LEGEND_KEY.to_string(), Value::Object(legend));
    }
    let mut dropped = Vec::new();
    for (alias, was_dropped) in KNOWN_ALIASES.iter().zip(coder.dropped_aliases) {
        if was_dropped {
            dropped.push(Value::String((*alias).to_string()));
        }
    }
    if !dropped.is_empty() {
        root.insert(DROPPED_ALIASES_KEY.to_string(), Value::Array(dropped));
    }
    root.insert(
        RENDERING_MODE_KEY.to_string(),
        Value::String(RENDERING_MODE_VALUE.to_string()),
    );

    let mut minified = serde_json::to_string(&document)
        .map_err(|err| format!("failed to render terse JSON: {err}"))?;
    minified.push('\n');
    Ok(minified)
}

#[derive(Default)]
struct LegendCoder {
    /// Code -> template, in first-appearance order. Emitted at document head.
    legend: Vec<(String, String)>,
    /// Template -> code, for deduplicating repeat appearances.
    by_text: HashMap<String, String>,
    /// Counts on all coded surfaces also reserve literal code-shaped strings.
    counts: HashMap<String, usize>,
    /// Distinct templates in first-appearance order, independent of map order.
    order: Vec<String>,
    /// Each known alias (indexed like [`KNOWN_ALIASES`]) is recorded once,
    /// regardless of the finding count.
    dropped_aliases: [bool; 5],
}

impl LegendCoder {
    fn observe(&mut self, text: &str) {
        let count = self.counts.entry(text.to_string()).or_default();
        if *count == 0 {
            self.order.push(text.to_string());
        }
        *count += 1;
    }

    fn select_legend(&mut self) -> Result<(), String> {
        let mut number = 1usize;
        let mut total_savings = 0usize;
        for text in &self.order {
            let count = self.counts.get(text).copied().unwrap_or(0);
            if count < 2 || text.len() < MIN_LEGEND_TEXT_LEN {
                continue;
            }
            // Reserve every literal on a coded surface, even short strings,
            // so expansion cannot mistake an original literal for a code.
            let code = loop {
                let candidate = format!("E{number:04}");
                if !self.counts.contains_key(&candidate) {
                    break candidate;
                }
                number += 1;
            };
            let encoded_len = serde_json::to_string(text)
                .map_err(|err| format!("failed to render terse JSON: {err}"))?
                .len();
            let code_len = code.len() + 2; // JSON quotes; codes need no escaping.
            let replacement_savings = count.saturating_mul(encoded_len.saturating_sub(code_len));
            let entry_cost = code_len + 1 + encoded_len + 1;
            // Require a standalone margin regardless of encounter order.
            // A tiny byte win can cost more model tokens; every admitted
            // value must fund a whole legend field by itself. The actual
            // field cost is still charged only once to the selected set.
            if replacement_savings <= entry_cost + LEGEND_FIELD_OVERHEAD {
                continue;
            }
            total_savings = total_savings.saturating_add(replacement_savings - entry_cost);
            self.by_text.insert(text.clone(), code.clone());
            self.legend.push((code, text.clone()));
            number += 1;
        }
        // Charge the shared field once to the complete admitted set.
        if total_savings <= LEGEND_FIELD_OVERHEAD {
            self.by_text.clear();
            self.legend.clear();
        }
        Ok(())
    }

    fn code_for(&self, text: &str) -> Option<String> {
        self.by_text.get(text).cloned()
    }
}

fn compact_finding(finding: &mut serde_json::Map<String, Value>, coder: &mut LegendCoder) {
    for key in PROSE_ARRAYS {
        let Some(Value::Array(entries)) = finding.get_mut(key) else {
            continue;
        };
        for entry in entries.iter_mut() {
            if let Some(text) = entry.as_str()
                && let Some(code) = coder.code_for(text)
            {
                *entry = Value::String(code);
            }
        }
    }
    if let Some(Value::Object(assertions)) = finding.get_mut(ASSERTION_TEXTS) {
        for value in assertions.values_mut() {
            if let Some(text) = value.as_str()
                && let Some(code) = coder.code_for(text)
            {
                *value = Value::String(code);
            }
        }
    }
    drop_same_value_aliases(finding, coder);
}

/// Omit only the documented aliases: equal values in unrelated fields do not
/// establish an alias relationship. The kept field is the decoding authority,
/// independent of map order or other coincidentally identical values.
fn drop_same_value_aliases(finding: &mut serde_json::Map<String, Value>, coder: &mut LegendCoder) {
    if let Some(action) = finding.get("suggested_next_action").and_then(Value::as_str)
        && action.len() >= MIN_LEGEND_TEXT_LEN
        && finding.get("recommended_next_step").and_then(Value::as_str) == Some(action)
    {
        finding.remove("suggested_next_action");
        coder.dropped_aliases[0] = true;
    }
    // #13673: the finding-level arrays repeat `activation` verbatim. An empty
    // array costs less than its legend entry, so only non-empty copies drop.
    for (index, key) in [(1, "observed_values"), (2, "missing_discriminators")] {
        let owner = finding
            .get("activation")
            .and_then(|activation| activation.get(key));
        let alias = finding.get(key);
        if let (Some(owner), Some(alias)) = (owner, alias)
            && owner == alias
            && alias.as_array().is_some_and(|items| !items.is_empty())
        {
            finding.remove(key);
            coder.dropped_aliases[index] = true;
        }
    }
    // #13673: `canonical_gap_id` is the same string as `canonical_gap.id`.
    let gap_id = finding.get("canonical_gap_id").and_then(Value::as_str);
    let owner_id = finding
        .get("canonical_gap")
        .and_then(|gap| gap.get("id"))
        .and_then(Value::as_str);
    if gap_id.is_some() && gap_id == owner_id {
        finding.remove("canonical_gap_id");
        coder.dropped_aliases[3] = true;
    }
    // #13673: `evidence_stages` rows restate `ripr` (stage, state, confidence).
    // A row carrying `basis` (or any extra key) is not a pure restatement, so
    // the whole array stays inline then.
    if finding
        .get("evidence_stages")
        .is_some_and(|rows| stage_rows_restate_ripr(rows, finding.get("ripr")))
    {
        finding.remove("evidence_stages");
        coder.dropped_aliases[4] = true;
    }
}

/// True only when `rows` is exactly the five `{stage,state,confidence}` rows
/// rebuilt from `ripr` in [`STAGE_ORDER`], so restoring is lossless.
fn stage_rows_restate_ripr(rows: &Value, ripr: Option<&Value>) -> bool {
    let (Some(rows), Some(ripr)) = (rows.as_array(), ripr) else {
        return false;
    };
    rows.len() == STAGE_ORDER.len()
        && rows.iter().zip(STAGE_ORDER).all(|(row, stage)| {
            let owner = ripr.get(stage);
            row.as_object().is_some_and(|row| row.len() == 3)
                && row.get("stage").and_then(Value::as_str) == Some(stage)
                && row.get("state").is_some()
                && row.get("state") == owner.and_then(|o| o.get("state"))
                && row.get("confidence").is_some()
                && row.get("confidence") == owner.and_then(|o| o.get("confidence"))
        })
}

/// Test-only: expands every terse-coded string back to its full template, so
/// contract tests can assert round-trip equivalence against the `json`
/// document without re-deriving the coding rules.
#[cfg(test)]
pub(crate) fn expand_terse_string(text: &str, legend: &BTreeMap<String, String>) -> String {
    legend
        .get(text)
        .cloned()
        .unwrap_or_else(|| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_DOC: &str = r#"{
  "schema_version": "0.2",
  "tool": "ripr",
  "mode": "draft",
  "root": ".",
  "summary": {"findings": 2},
  "findings": [
    {
      "id": "probe:a",
      "classification": "weakly_exposed",
      "severity": "warn",
      "confidence": 0.8,
      "probe": {"id": "probe:a"},
      "evidence_path": [
        "reach yes: Related tests appear to reach the changed behavior",
        "infection yes: Reachable tests can plausibly activate this changed behavior"
      ],
      "evidence": [
        "Related tests appear to reach the changed behavior",
        "Reachable tests can plausibly activate this changed behavior"
      ],
      "assertion_texts": {"10": "assert_eq!(masked(2)(3), 2);"},
      "recommended_next_step": "Add a focused assertion that discriminates the changed behavior.",
      "suggested_next_action": "Add a focused assertion that discriminates the changed behavior.",
      "language": "rust"
    },
    {
      "id": "probe:b",
      "classification": "exposed",
      "severity": "error",
      "confidence": 0.9,
      "probe": {"id": "probe:b"},
      "evidence_path": [
        "reach yes: Related tests appear to reach the changed behavior"
      ],
      "evidence": [
        "Related tests appear to reach the changed behavior"
      ],
      "assertion_texts": {"12": "assert_eq!(masked(2)(3), 2);"},
      "language": "rust"
    }
  ]
}
"#;

    fn legend_of(terse: &Value) -> BTreeMap<String, String> {
        let mut legend = BTreeMap::new();
        if let Some(entries) = terse.get(LEGEND_KEY).and_then(Value::as_object) {
            for (code, text) in entries {
                if let Some(text) = text.as_str() {
                    legend.insert(code.clone(), text.to_string());
                }
            }
        }
        legend
    }

    fn expand_array(
        terse_finding: &Value,
        key: &str,
        legend: &BTreeMap<String, String>,
    ) -> Vec<String> {
        terse_finding
            .get(key)
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|text| expand_terse_string(text, legend))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn terse_aliases_keep_their_named_owner_and_record_once() -> Result<(), String> {
        let text = "Long coincidentally equal values are not all semantic aliases.";
        let different = "Keep a distinct suggested next action verbatim.";
        let full = serde_json::json!({"findings": [
            {"id": text, "alignment_reason": text, "recommended_next_step": text,
             "suggested_next_action": text, "static_limit_kind": text},
            {"recommended_next_step": text, "suggested_next_action": text},
            {"recommended_next_step": text, "suggested_next_action": different},
            {"recommended_next_step": "short", "suggested_next_action": "short"}
        ]});
        let input = serde_json::to_string(&full).map_err(|error| error.to_string())?;
        let output = super::render_terse(&input)?;
        let terse: Value = serde_json::from_str(&output).map_err(|error| error.to_string())?;
        assert_eq!(
            terse["dropped_aliases"],
            serde_json::json!(["suggested_next_action"])
        );
        for key in [
            "id",
            "alignment_reason",
            "recommended_next_step",
            "static_limit_kind",
        ] {
            assert_eq!(terse["findings"][0][key], full["findings"][0][key], "{key}");
        }
        for index in [0, 1] {
            assert!(
                terse["findings"][index]
                    .get("suggested_next_action")
                    .is_none()
            );
            assert_eq!(terse["findings"][index]["recommended_next_step"], text);
        }
        for index in [2, 3] {
            assert_eq!(terse["findings"][index], full["findings"][index]);
        }
        Ok(())
    }

    #[test]
    fn terse_drops_finding_activation_copies_only_when_equal_and_non_empty() -> Result<(), String> {
        let values =
            serde_json::json!([{"line": 8, "value": "age = 30", "context": "function_argument"}]);
        let missing = serde_json::json!([{"value": "age == 18", "reason": "no input at 18"}]);
        let other =
            serde_json::json!([{"line": 9, "value": "age = 31", "context": "function_argument"}]);
        let full = serde_json::json!({"findings": [
            {"activation": {"observed_values": values, "missing_discriminators": missing},
             "observed_values": values, "missing_discriminators": missing},
            {"activation": {"observed_values": values, "missing_discriminators": []},
             "observed_values": other, "missing_discriminators": []},
            {"observed_values": values, "missing_discriminators": missing}
        ]});
        let input = serde_json::to_string(&full).map_err(|error| error.to_string())?;
        let terse: Value = serde_json::from_str(&super::render_terse(&input)?)
            .map_err(|error| error.to_string())?;
        assert_eq!(
            terse["dropped_aliases"],
            serde_json::json!(["observed_values", "missing_discriminators"])
        );
        // Equal non-empty copies drop; the activation owner keeps the value.
        assert!(terse["findings"][0].get("observed_values").is_none());
        assert!(terse["findings"][0].get("missing_discriminators").is_none());
        assert_eq!(
            terse["findings"][0]["activation"],
            full["findings"][0]["activation"]
        );
        // A differing copy, an empty copy and a finding without an owner stay.
        assert_eq!(terse["findings"][1], full["findings"][1]);
        assert_eq!(terse["findings"][2], full["findings"][2]);
        Ok(())
    }

    #[test]
    fn terse_drops_gap_id_and_stage_rows_only_when_they_restate_their_owner() -> Result<(), String>
    {
        let ripr = serde_json::json!({
            "reach": {"state": "yes", "confidence": "high", "summary": "r"},
            "infect": {"state": "weak", "confidence": "medium", "summary": "i"},
            "propagate": {"state": "yes", "confidence": "medium", "summary": "p"},
            "observe": {"state": "yes", "confidence": "medium", "summary": "o"},
            "discriminate": {"state": "no", "confidence": "low", "summary": "d"}
        });
        let rows = serde_json::json!([
            {"stage": "reach", "state": "yes", "confidence": "high"},
            {"stage": "infect", "state": "weak", "confidence": "medium"},
            {"stage": "propagate", "state": "yes", "confidence": "medium"},
            {"stage": "observe", "state": "yes", "confidence": "medium"},
            {"stage": "discriminate", "state": "no", "confidence": "low"}
        ]);
        let mut with_basis = rows.clone();
        with_basis[4]["basis"] = serde_json::json!([{"code": "C1", "summary": "cap"}]);
        let mut wrong_state = rows.clone();
        wrong_state[1]["state"] = serde_json::json!("yes");
        let gap = serde_json::json!({"id": "gap:a", "owner": "f"});
        let full = serde_json::json!({"findings": [
            {"ripr": ripr, "evidence_stages": rows,
             "canonical_gap": gap, "canonical_gap_id": "gap:a"},
            {"ripr": ripr, "evidence_stages": with_basis,
             "canonical_gap": gap, "canonical_gap_id": "gap:b"},
            {"ripr": ripr, "evidence_stages": wrong_state}
        ]});
        let input = serde_json::to_string(&full).map_err(|error| error.to_string())?;
        let terse: Value = serde_json::from_str(&super::render_terse(&input)?)
            .map_err(|error| error.to_string())?;
        assert_eq!(
            terse["dropped_aliases"],
            serde_json::json!(["canonical_gap_id", "evidence_stages"])
        );
        assert!(terse["findings"][0].get("evidence_stages").is_none());
        assert!(terse["findings"][0].get("canonical_gap_id").is_none());
        // Basis rows, a different gap id and a state mismatch are all kept.
        assert_eq!(terse["findings"][1], full["findings"][1]);
        assert_eq!(terse["findings"][2], full["findings"][2]);
        Ok(())
    }

    #[test]
    fn terse_round_trips_prose_and_drops_only_alias_fields() -> Result<(), String> {
        let full: Value = serde_json::from_str(FULL_DOC).map_err(|error| error.to_string())?;
        let terse_text = super::render_terse(FULL_DOC)?;
        let terse: Value = serde_json::from_str(&terse_text).map_err(|error| error.to_string())?;
        let legend = legend_of(&terse);

        // Head block: rendering marker, legend, dropped-alias record.
        assert_eq!(
            terse.get(RENDERING_MODE_KEY).and_then(Value::as_str),
            Some(RENDERING_MODE_VALUE)
        );
        assert!(
            !legend.is_empty(),
            "repeated prose must be coded: {terse_text}"
        );
        assert_eq!(
            terse
                .get(DROPPED_ALIASES_KEY)
                .and_then(Value::as_array)
                .map(|keys| keys.len()),
            Some(1),
            "only suggested_next_action duplicates a same-level value: {terse_text}"
        );

        // No finding dropped, verdicts and identity fields untouched.
        let full_findings = full
            .get("findings")
            .and_then(Value::as_array)
            .cloned()
            .ok_or("full document has no findings array")?;
        let terse_findings = terse
            .get("findings")
            .and_then(Value::as_array)
            .cloned()
            .ok_or("terse document has no findings array")?;
        assert_eq!(full_findings.len(), terse_findings.len());
        for (full_finding, terse_finding) in full_findings.iter().zip(terse_findings.iter()) {
            for key in [
                "id",
                "classification",
                "severity",
                "confidence",
                "probe",
                "language",
            ] {
                assert_eq!(full_finding[key], terse_finding[key], "{key} moved");
            }
            for key in super::PROSE_ARRAYS {
                let original: Vec<String> = full_finding
                    .get(key)
                    .and_then(serde_json::Value::as_array)
                    .map(|entries| {
                        entries
                            .iter()
                            .filter_map(|entry| entry.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                assert_eq!(
                    expand_array(terse_finding, key, &legend),
                    original,
                    "{key} did not round-trip"
                );
            }
            // assertion_texts values round-trip through the legend.
            let full_asserts = full_finding
                .get(ASSERTION_TEXTS)
                .and_then(serde_json::Value::as_object)
                .cloned()
                .unwrap_or_default();
            let terse_asserts = terse_finding
                .get(ASSERTION_TEXTS)
                .and_then(serde_json::Value::as_object)
                .cloned()
                .unwrap_or_default();
            assert_eq!(full_asserts.len(), terse_asserts.len());
            for (line, text) in &terse_asserts {
                let terse_text = text
                    .as_str()
                    .ok_or("terse assertion_texts value not a string")?;
                let full_text = full_asserts
                    .get(line)
                    .and_then(serde_json::Value::as_str)
                    .ok_or("full assertion_texts value missing")?;
                assert_eq!(expand_terse_string(terse_text, &legend), full_text);
            }
            // The dropped alias value survives in the kept field.
            if terse
                .get(DROPPED_ALIASES_KEY)
                .and_then(Value::as_array)
                .is_some_and(|keys| keys.iter().any(|key| key == "suggested_next_action"))
            {
                assert_eq!(
                    terse_finding["suggested_next_action"],
                    Value::Null,
                    "alias key must be omitted"
                );
                assert_eq!(
                    terse_finding["recommended_next_step"],
                    full_finding["recommended_next_step"]
                );
            }
        }
        // Identity/head fields untouched.
        for key in ["schema_version", "tool", "mode", "root", "summary"] {
            assert_eq!(full[key], terse[key], "{key} moved");
        }
        Ok(())
    }

    #[test]
    fn terse_is_smaller_than_compact_on_repeated_prose() -> Result<(), String> {
        let terse = super::render_terse(FULL_DOC)?;
        let compact = crate::output::compact_json::finish(
            FULL_DOC.to_string(),
            &crate::output::format::OutputFormat::JsonCompact,
        );
        assert!(
            terse.len() < compact.len(),
            "terse {} must beat compact {} on repeated prose",
            terse.len(),
            compact.len()
        );
        Ok(())
    }

    #[test]
    fn terse_codes_only_profitable_repetitions_and_avoids_literal_codes() -> Result<(), String> {
        let long = "A repeated sentence with enough content to pay for its legend entry.";
        let unique = "A unique sentence long enough to have been coded before.";
        let escaped = "Escaped quotes \" and backslashes \\ and Unicode 雪 repeat here.";
        let input = serde_json::json!({"findings": [{
            "id": "same-id", "evidence": ["E0001", unique, long, long, escaped, escaped],
            "missing": ["short", "short"], "assertion_texts": {"1": long}
        }]});
        let original = serde_json::to_string(&input).map_err(|error| error.to_string())?;
        let output = super::render_terse(&original)?;
        assert_eq!(output, super::render_terse(&original)?);
        let terse: Value = serde_json::from_str(&output).map_err(|error| error.to_string())?;
        let legend = legend_of(&terse);
        assert!(!legend.contains_key("E0001"));
        assert_eq!(terse["findings"][0]["evidence"][0], "E0001");
        assert_eq!(terse["findings"][0]["evidence"][1], unique);
        assert!(legend.values().any(|text| text == long));
        assert!(legend.values().any(|text| text == escaped));
        for (original, compact) in input["findings"][0]["evidence"]
            .as_array()
            .ok_or("missing original evidence")?
            .iter()
            .zip(
                terse["findings"][0]["evidence"]
                    .as_array()
                    .ok_or("missing terse evidence")?,
            )
        {
            let text = compact.as_str().ok_or("non-string evidence")?;
            assert_eq!(
                Some(expand_terse_string(text, &legend).as_str()),
                original.as_str()
            );
        }
        assert_eq!(terse["findings"][0]["id"], "same-id");
        Ok(())
    }

    #[test]
    fn terse_keeps_marginal_repetitions_inline_in_either_order() -> Result<(), String> {
        let marginal = "123456789012345678901234";
        let profitable =
            "A much longer sentence repeated often enough to pay for its legend entry.";
        let mut sizes = Vec::new();
        for evidence in [
            vec![marginal, profitable, profitable, profitable, marginal],
            vec![profitable, profitable, profitable, marginal, marginal],
        ] {
            let input = serde_json::json!({"findings": [{"evidence": evidence}]});
            let original = serde_json::to_string(&input).map_err(|error| error.to_string())?;
            let output = super::render_terse(&original)?;
            let terse: Value = serde_json::from_str(&output).map_err(|error| error.to_string())?;
            let legend = legend_of(&terse);
            assert_eq!(terse["rendering"], "terse");
            assert_eq!(legend.len(), 1);
            assert!(legend.values().any(|text| text == profitable));
            assert!(!legend.values().any(|text| text == marginal));
            let entries = terse["findings"][0]["evidence"]
                .as_array()
                .ok_or("missing terse evidence")?;
            for (entry, expected) in entries.iter().zip(&evidence) {
                let text = entry.as_str().ok_or("expected string")?;
                assert_eq!(expand_terse_string(text, &legend), *expected);
                if *expected == marginal {
                    assert_eq!(text, marginal);
                }
            }
            sizes.push(output.len());
        }
        assert_eq!(sizes[0], sizes[1]);
        Ok(())
    }

    #[test]
    fn terse_requires_a_standalone_margin_for_each_legend_entry() -> Result<(), String> {
        // Collective byte savings do not override the per-entry admission
        // margin. A 41-byte repeated value exactly breaks even alone; the
        // next byte pays the complete field cost and is admitted.
        for (width, count, codes) in [
            (24usize, 1usize, false),
            (26, 4, false),
            (24, 7, false),
            (41, 1, false),
            (42, 1, true),
        ] {
            let evidence: Vec<String> = (0..count)
                .flat_map(|index| {
                    let text = format!("{index:0width$}");
                    [text.clone(), text]
                })
                .collect();
            let mut inline = serde_json::json!({"findings": [{"evidence": evidence}]});
            let original = serde_json::to_string(&inline).map_err(|error| error.to_string())?;
            let output = super::render_terse(&original)?;
            let terse: Value = serde_json::from_str(&output).map_err(|error| error.to_string())?;
            let legend = legend_of(&terse);
            inline["rendering"] = Value::String("terse".to_string());
            if codes {
                assert_eq!(legend.len(), count);
                let inline_text =
                    serde_json::to_string(&inline).map_err(|error| error.to_string())?;
                assert!(output.len() < inline_text.len() + 1);
                let entries = terse["findings"][0]["evidence"]
                    .as_array()
                    .ok_or("missing terse evidence")?;
                for (entry, expected) in entries.iter().zip(&evidence) {
                    let code = entry.as_str().ok_or("expected code")?;
                    assert_eq!(legend.get(code), Some(expected));
                }
            } else {
                assert_eq!(terse, inline);
                assert!(legend.is_empty());
            }
        }
        Ok(())
    }

    #[test]
    fn terse_keeps_empty_and_singleton_documents_inline() -> Result<(), String> {
        for input in [
            r#"{"findings":[]}"#,
            r#"{"findings":[{"evidence":["Only one long sentence should stay in the output."]}]}"#,
            r#"{"findings":[{"evidence":["First unique long sentence remains inline.","Second unique long sentence remains inline."]}]}"#,
        ] {
            let output = super::render_terse(input)?;
            let value: Value = serde_json::from_str(&output).map_err(|error| error.to_string())?;
            let original: Value = serde_json::from_str(input).map_err(|error| error.to_string())?;
            assert_eq!(value["findings"], original["findings"]);
            assert!(value.get(LEGEND_KEY).is_none());
            // The profile marker is an unavoidable constant overhead.
            let compact = crate::output::compact_json::finish(
                input.to_string(),
                &crate::output::format::OutputFormat::JsonCompact,
            );
            assert!(output.len() > compact.len());
        }
        Ok(())
    }

    #[test]
    fn terse_fails_closed_on_non_json_input() -> Result<(), String> {
        let error = match super::render_terse("not json") {
            Err(error) => error,
            Ok(document) => {
                return Err(format!("non-JSON input must not pass through: {document}"));
            }
        };
        assert!(error.contains("terse"), "unexpected error: {error}");
        Ok(())
    }

    #[test]
    fn terse_never_drops_unrelated_fields_even_when_values_repeat() -> Result<(), String> {
        // Equal values do not make `language` and `note` semantic aliases.
        let doc = r#"{"schema_version":"0.2","findings":[{"id":"probe:x","language":"Reachable tests can plausibly activate this changed behavior","note":"Reachable tests can plausibly activate this changed behavior"}]}"#;
        let terse = super::render_terse(doc)?;
        let terse: Value = serde_json::from_str(&terse).map_err(|error| error.to_string())?;
        let finding = &terse["findings"][0];
        assert_eq!(
            finding["language"].as_str(),
            Some("Reachable tests can plausibly activate this changed behavior")
        );
        assert_eq!(finding["note"], finding["language"]);
        Ok(())
    }
}

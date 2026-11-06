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
//! 2. The known `suggested_next_action` alias is omitted when it equals
//!    `recommended_next_step`; the document records the alias once in
//!    `dropped_aliases`. Other fields remain verbatim even when values match.
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
    if coder.dropped_next_action_alias {
        root.insert(
            DROPPED_ALIASES_KEY.to_string(),
            Value::Array(vec![Value::String("suggested_next_action".to_string())]),
        );
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
    /// The known alias is recorded once, regardless of the finding count.
    dropped_next_action_alias: bool,
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

/// Omit only the documented alias: equal text in unrelated fields does not
/// establish an alias relationship. The kept field is the decoding authority,
/// independent of map order or other coincidentally identical string values.
fn drop_same_value_aliases(finding: &mut serde_json::Map<String, Value>, coder: &mut LegendCoder) {
    let Some(action) = finding.get("suggested_next_action").and_then(Value::as_str) else {
        return;
    };
    if action.len() >= MIN_LEGEND_TEXT_LEN
        && finding.get("recommended_next_step").and_then(Value::as_str) == Some(action)
    {
        finding.remove("suggested_next_action");
        coder.dropped_next_action_alias = true;
    }
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
        let compact = super::super::render_compact(FULL_DOC);
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
            assert!(output.len() > super::super::render_compact(input).len());
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

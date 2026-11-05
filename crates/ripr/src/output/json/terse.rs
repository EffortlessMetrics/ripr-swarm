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
//! 2. A finding field whose string value is byte-identical to an earlier
//!    field in the same finding object is emitted once; the later key is
//!    omitted and recorded in `dropped_aliases`. Identity fields (id,
//!    classification, severity, ...) are exempt and never dropped.
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

/// Prose shorter than this stays verbatim: coding it would cost more bytes
/// in legend + code than it saves.
const MIN_LEGEND_TEXT_LEN: usize = 24;

/// Finding-array fields whose repeated sentence templates are coded.
const PROSE_ARRAYS: [&str; 3] = ["evidence_path", "evidence", "missing"];

/// Map field whose values (assertion source lines) are coded the same way.
const ASSERTION_TEXTS: &str = "assertion_texts";

/// Fields that are identity or verdict-bearing and must survive verbatim even
/// when their value coincidentally equals another field's value.
const NEVER_DROP: [&str; 12] = [
    "id",
    "classification",
    "severity",
    "confidence",
    "schema_version",
    "probe",
    "ripr",
    "evidence_path",
    "evidence",
    "missing",
    "assertion_texts",
    "language",
];

/// Marker stamped at the document head so consumers can key on the rendering
/// profile without sniffing for the legend.
const RENDERING_MODE_KEY: &str = "rendering";
const RENDERING_MODE_VALUE: &str = "terse";
const LEGEND_KEY: &str = "evidence_legend";
const DROPPED_ALIASES_KEY: &str = "dropped_aliases";

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
    if !coder.dropped_aliases.is_empty() {
        root.insert(
            DROPPED_ALIASES_KEY.to_string(),
            Value::Array(
                coder
                    .dropped_aliases
                    .iter()
                    .map(|alias| Value::String(alias.clone()))
                    .collect(),
            ),
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
    /// Field names omitted as same-value aliases, for `dropped_aliases`.
    dropped_aliases: Vec<String>,
}

impl LegendCoder {
    fn code_for(&mut self, text: &str) -> String {
        if let Some(code) = self.by_text.get(text) {
            return code.clone();
        }
        let code = format!("E{:04}", self.legend.len() + 1);
        self.by_text.insert(text.to_string(), code.clone());
        self.legend.push((code.clone(), text.to_string()));
        code
    }
}

fn compact_finding(finding: &mut serde_json::Map<String, Value>, coder: &mut LegendCoder) {
    for key in PROSE_ARRAYS {
        let Some(Value::Array(entries)) = finding.get_mut(key) else {
            continue;
        };
        for entry in entries.iter_mut() {
            if let Some(text) = entry.as_str()
                && text.len() >= MIN_LEGEND_TEXT_LEN
            {
                *entry = Value::String(coder.code_for(text));
            }
        }
    }
    if let Some(Value::Object(assertions)) = finding.get_mut(ASSERTION_TEXTS) {
        for value in assertions.values_mut() {
            if let Some(text) = value.as_str()
                && text.len() >= MIN_LEGEND_TEXT_LEN
            {
                *value = Value::String(coder.code_for(text));
            }
        }
    }
    drop_same_value_aliases(finding, coder);
}

/// Emits a scalar string field once per finding: a later field whose string
/// value is byte-identical to an earlier field's value is removed, and the
/// removed key is recorded in `dropped_aliases`. The value always survives in
/// the earlier field, so the semantic content is unchanged. Short values
/// (file names, "rust", "yes") are shared vocabulary, not duplication, and
/// stay. `NEVER_DROP` identity/verdict fields are exempt on both sides.
fn drop_same_value_aliases(finding: &mut serde_json::Map<String, Value>, coder: &mut LegendCoder) {
    let mut seen: HashMap<String, ()> = HashMap::new();
    let keys: Vec<String> = finding.keys().cloned().collect();
    for key in keys {
        let Some(value) = finding.get(&key) else {
            continue;
        };
        let Some(text) = value.as_str() else {
            continue;
        };
        if NEVER_DROP.contains(&key.as_str()) || text.len() < MIN_LEGEND_TEXT_LEN {
            seen.insert(text.to_string(), ());
            continue;
        }
        if seen.contains_key(text) {
            finding.remove(&key);
            coder.dropped_aliases.push(key);
        } else {
            seen.insert(text.to_string(), ());
        }
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
    fn terse_never_drops_denied_fields_even_when_values_repeat() -> Result<(), String> {
        // `language` equals a long prose value in the same object; the
        // denylist must keep both.
        let doc = r#"{"schema_version":"0.2","findings":[{"id":"probe:x","language":"Reachable tests can plausibly activate this changed behavior","note":"Reachable tests can plausibly activate this changed behavior"}]}"#;
        let terse = super::render_terse(doc)?;
        let terse: Value = serde_json::from_str(&terse).map_err(|error| error.to_string())?;
        let finding = &terse["findings"][0];
        assert_eq!(
            finding["language"].as_str(),
            Some("Reachable tests can plausibly activate this changed behavior")
        );
        assert_eq!(finding["note"], Value::Null);
        Ok(())
    }
}

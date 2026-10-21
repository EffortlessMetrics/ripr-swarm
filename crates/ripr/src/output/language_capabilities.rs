//! Renderers for `ripr capabilities`.
//!
//! Projects the closed table in `domain::language_capability` and the
//! compile-time adapter availability. Availability and semantic support are
//! separate fields on purpose: a compiled adapter does not raise a cell, and a
//! cell does not imply the adapter is compiled or enabled.

use crate::domain::LanguageId;
use crate::domain::language_capability::{
    CAPABILITY_CELLS, COMMON_NON_CLAIMS, CapabilityCell, LANGUAGE_CAPABILITIES_SCHEMA_VERSION,
    SupportState, cell_id, semantic_digest,
};
use serde_json::{Value, json};

const CLAIM_BOUNDARY: &str = "Static capability registry for this ripr build. It states what each language, framework and mode can establish and where it must widen or stop; it does not prove an adapter's output is correct, does not report whether a language is enabled for a repository (see `ripr doctor`), and does not promote any support tier.";

fn selected(language: Option<LanguageId>) -> Vec<&'static CapabilityCell> {
    CAPABILITY_CELLS
        .iter()
        .filter(|cell| language.is_none_or(|wanted| cell.language == wanted))
        .collect()
}

fn language_status(language: LanguageId) -> &'static str {
    match language {
        LanguageId::Rust => "stable",
        LanguageId::TypeScript | LanguageId::JavaScript | LanguageId::Python | LanguageId::Perl => {
            "preview"
        }
    }
}

pub(crate) fn render_json(language: Option<LanguageId>) -> String {
    let cells = selected(language);
    let languages: Vec<Value> = LanguageId::ALL
        .into_iter()
        .filter(|candidate| language.is_none_or(|wanted| wanted == *candidate))
        .map(|candidate| {
            json!({
                "language": candidate.as_str(),
                "language_status": language_status(candidate),
                "adapter_compiled": candidate.is_available(),
                "required_feature": candidate.required_feature(),
            })
        })
        .collect();
    let rows: Vec<Value> = cells
        .iter()
        .map(|cell| {
            json!({
                "id": cell_id(cell),
                "language": cell.language.as_str(),
                "framework": cell.framework,
                "mode": cell.mode.as_str(),
                "family": cell.family.as_str(),
                "state": cell.state.as_str(),
                "tier_ceiling": cell.tier_ceiling.as_str(),
                "limit": cell.limit,
                "proof_refs": cell.proof_refs,
            })
        })
        .collect();
    let vocabulary: Vec<&str> = SupportState::ALL.iter().map(|s| s.as_str()).collect();
    let document = json!({
        "schema_version": LANGUAGE_CAPABILITIES_SCHEMA_VERSION,
        "kind": "ripr_language_capabilities",
        "semantic_digest": semantic_digest(CAPABILITY_CELLS),
        "availability_note": "adapter_compiled is build availability only; it neither implies nor limits a cell's state",
        "filtered_by": language.map(|wanted| json!({"language": wanted.as_str()})),
        "digest_scope": "whole_table",
        "languages": languages,
        "support_state_vocabulary": vocabulary,
        "absent_cell_meaning": "not_proven",
        "cells": rows,
        "non_claims": COMMON_NON_CLAIMS,
        "claim_boundary": CLAIM_BOUNDARY,
    });
    let mut text = serde_json::to_string_pretty(&document).unwrap_or_else(|_| "{}".to_string());
    text.push('\n');
    text
}

pub(crate) fn render_human(language: Option<LanguageId>) -> String {
    let mut out = String::from("ripr language capabilities (static registry)\n");
    for candidate in LanguageId::ALL {
        if language.is_some_and(|wanted| wanted != candidate) {
            continue;
        }
        out.push_str(&format!(
            "\n{} [{}; adapter {}]\n",
            candidate.display_name(),
            language_status(candidate),
            if candidate.is_available() {
                "compiled"
            } else {
                "not compiled"
            }
        ));
        for cell in CAPABILITY_CELLS.iter().filter(|c| c.language == candidate) {
            out.push_str(&format!(
                "  {}/{}/{}: {} (ceiling {})\n",
                cell.framework,
                cell.mode.as_str(),
                cell.family.as_str(),
                cell.state.as_str(),
                cell.tier_ceiling.as_str()
            ));
            if let Some(limit) = cell.limit {
                out.push_str(&format!("      limit: {limit}\n"));
            }
        }
    }
    out.push_str(
        "\nStatic evidence only: no runtime mutation outcome and no sufficiency claim.\nA combination with no row is not_proven. Use --json for the machine form.\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(language: Option<LanguageId>) -> Value {
        serde_json::from_str(&render_json(language)).unwrap_or(Value::Null)
    }

    #[test]
    fn json_lists_every_cell_and_every_routed_language() {
        let doc = parsed(None);
        assert_eq!(doc["kind"], "ripr_language_capabilities");
        assert_eq!(
            doc["cells"].as_array().map(Vec::len),
            Some(CAPABILITY_CELLS.len())
        );
        assert_eq!(
            doc["languages"].as_array().map(Vec::len),
            Some(LanguageId::ALL.len())
        );
        assert_eq!(
            doc["support_state_vocabulary"].as_array().map(Vec::len),
            Some(7)
        );
    }

    #[test]
    fn language_filter_narrows_cells_but_keeps_the_whole_table_digest() {
        let all = parsed(None);
        let python = parsed(Some(LanguageId::Python));
        let cells = python["cells"].as_array().cloned().unwrap_or_default();
        assert!(!cells.is_empty());
        assert!(cells.iter().all(|c| c["language"] == "python"));
        assert_eq!(all["semantic_digest"], python["semantic_digest"]);
        assert_eq!(python["filtered_by"]["language"], "python");
        assert!(all["filtered_by"].is_null());
        assert_eq!(python["digest_scope"], "whole_table");
    }

    #[test]
    fn availability_does_not_change_cell_state() {
        let doc = parsed(Some(LanguageId::Perl));
        let repair = doc["cells"]
            .as_array()
            .and_then(|cells| cells.iter().find(|c| c["family"] == "repair_routing"))
            .cloned()
            .unwrap_or(Value::Null);
        assert_eq!(repair["state"], "not_proven");
        assert!(doc["languages"][0]["adapter_compiled"].is_boolean());
    }

    #[test]
    fn human_output_names_limits_and_the_absent_row_rule() {
        let text = render_human(Some(LanguageId::TypeScript));
        assert!(text.contains("supported_with_required_fallback"), "{text}");
        assert!(text.contains("typescript_diff_first"), "{text}");
        assert!(text.contains("not_proven"), "{text}");
    }
}

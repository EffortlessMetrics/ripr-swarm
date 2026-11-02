//! Renderers for `ripr capabilities`.
//!
//! Projects the closed table in `domain::language_capability` and the
//! compile-time adapter availability. Availability and semantic support are
//! separate fields on purpose: a compiled adapter does not raise a cell, and a
//! cell does not imply the adapter is compiled or enabled.

use crate::domain::LanguageId;
use crate::domain::language_capability::{
    CAPABILITY_CELLS, COMMON_NON_CLAIMS, CapabilityCell, LANGUAGE_CAPABILITIES_SCHEMA_VERSION,
    SupportState, cell_id, render_registry_block, semantic_digest,
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
        LanguageId::TypeScript
        | LanguageId::JavaScript
        | LanguageId::Python
        | LanguageId::Perl
        | LanguageId::Go => "preview",
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

/// The generated `docs/status/SUPPORT_TIERS.md` registry block (markers
/// included, trailing newline added), from the one renderer in the domain.
/// Whole table only; `cargo xtask capability-docs` consumes this.
pub(crate) fn render_markdown() -> String {
    let mut text = render_registry_block(CAPABILITY_CELLS);
    text.push('\n');
    text
}

/// One plain sentence per language for the default screen (#10696). The cell
/// rows, state names and spec references stay in `--json` and `--markdown`,
/// which are the machine and registry forms of the same table.
fn plain_summary(language: LanguageId) -> &'static str {
    match language {
        LanguageId::Rust => {
            "finds changed code your tests may not catch, from a diff or the whole repo, and can point at one focused test to add for each gap."
        }
        LanguageId::Python => {
            "finds test gaps in changed code for pytest and unittest and points at where to add a test. It does not write tests."
        }
        LanguageId::TypeScript | LanguageId::JavaScript => {
            "finds test gaps in changed code from the diff. Results are advisory, and it does not write tests."
        }
        LanguageId::Perl => {
            "not supported yet: it needs a special ripr build and a Perl facts exporter, and default builds only report that Perl files changed."
        }
        LanguageId::Go => {
            "not supported yet: Go has a closed identity token only; default builds do not analyze `.go` files."
        }
    }
}

pub(crate) fn render_human(language: Option<LanguageId>) -> String {
    let mut out = String::from("ripr language support\n\n");
    for candidate in LanguageId::ALL {
        if language.is_some_and(|wanted| wanted != candidate) {
            continue;
        }
        let build_note = if candidate.is_available() {
            ""
        } else {
            "; not in this build"
        };
        out.push_str(&format!(
            "{} ({}{build_note}): {}\n",
            candidate.display_name(),
            language_status(candidate),
            plain_summary(candidate)
        ));
    }
    out.push_str(
        "\nripr reads code; it does not run your tests, and a finding is not a measured result.\nFor every language, mode and limit, use --json (or --markdown for the support table).\n",
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
    fn human_output_has_exactly_one_summary_line_per_language() {
        let all = render_human(None);
        for language in LanguageId::ALL {
            let lines = all
                .lines()
                .filter(|line| line.starts_with(&format!("{} (", language.display_name())))
                .count();
            assert_eq!(lines, 1, "{language:?}: {all}");
        }
        let one = render_human(Some(LanguageId::Python));
        assert_eq!(
            one.lines()
                .filter(|line| line.contains(" (preview"))
                .count(),
            1,
            "{one}"
        );
    }

    #[test]
    fn go_language_id_serializes_as_go_wire_token() {
        let json = serde_json::to_string(&LanguageId::Go).unwrap_or_default();
        assert_eq!(json, "\"go\"");
        let round_trip: LanguageId = serde_json::from_str(&json).unwrap_or(LanguageId::Rust);
        assert_eq!(round_trip, LanguageId::Go);
    }

    #[test]
    fn human_output_is_one_plain_sentence_per_language_without_registry_tokens() {
        let text = render_human(Some(LanguageId::TypeScript));
        // Build-feature aware: an uncompiled adapter still gets its sentence,
        // marked "not in this build".
        let marker = if LanguageId::TypeScript.is_available() {
            "TypeScript (preview): finds test gaps"
        } else {
            "TypeScript (preview; not in this build): finds test gaps"
        };
        assert!(text.contains(marker), "{text}");
        assert!(!text.contains("Python"), "{text}");
        let all = render_human(None);
        for language in LanguageId::ALL {
            assert!(all.contains(language.display_name()), "{language:?}: {all}");
        }
        for token in [
            "RIPR-SPEC",
            "not_proven",
            "static_facts",
            "repair_routing",
            "supported_with_required_fallback",
            "usable_alpha",
            "typescript_diff_first",
            "ceiling",
        ] {
            assert!(!all.contains(token), "{token}: {all}");
        }
        assert!(all.contains("--json"), "{all}");
    }
}

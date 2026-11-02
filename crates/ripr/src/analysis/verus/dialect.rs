//! Line-oriented Verus dialect lexer v1 (`verus-dialect-lexer-v1`).
//!
//! Operates only on text inside `verus! { .. }` token trees discovered by
//! `analysis::probes::macro_definition::verus_regions`. No rust-analyzer, no
//! macro expansion, and no solver.

use crate::analysis::probes::verus_region_line_spans;

pub(crate) const PARSER_VERSION: &str = "verus-dialect-lexer-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerusItemMode {
    Exec,
    Spec,
    Proof,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerusClauseKind {
    Requires,
    Ensures,
    Recommends,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VerusClauseFact {
    pub kind: VerusClauseKind,
    pub line: usize,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerusTrustBoundary {
    /// Ordinary checked item with no trusted/external markers on the header.
    Checked,
    /// `external_body` or equivalent: body is not verified here.
    ExternalBody,
    Assume,
    Admit,
    /// `#[verifier::external]` / excluded external item.
    VerifierExternal,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VerusGuardFact {
    pub owner_name: String,
    pub mode: VerusItemMode,
    pub header_start_line: usize,
    pub header_end_line: usize,
    pub region_start_line: usize,
    pub region_end_line: usize,
    pub requires: Vec<VerusClauseFact>,
    pub ensures: Vec<VerusClauseFact>,
    pub recommends: Vec<VerusClauseFact>,
    pub trust: VerusTrustBoundary,
    pub parser_version: String,
}

pub(crate) fn verus_guard_facts(source: &str) -> Vec<VerusGuardFact> {
    let mut out = Vec::new();
    for (ordinal, (region_start_line, region_end_line)) in
        verus_region_line_spans(source).into_iter().enumerate()
    {
        let start = region_start_line.saturating_sub(1);
        let end = region_end_line;
        if start >= end {
            continue;
        }
        let lines: Vec<&str> = source.lines().collect();
        if end > lines.len() {
            continue;
        }
        let slice = &lines[start..end];
        out.extend(parse_region_items(
            slice,
            region_start_line,
            region_end_line,
            ordinal,
        ));
    }
    out
}

pub(crate) fn guard_fact_at_line<'a>(
    facts: &'a [VerusGuardFact],
    line: usize,
) -> Option<&'a VerusGuardFact> {
    facts.iter().find(|fact| {
        line >= fact.header_start_line && line <= fact.header_end_line.max(fact.region_end_line)
    })
}

fn parse_region_items(
    region_lines: &[&str],
    region_start_line: usize,
    region_end_line: usize,
    _ordinal: usize,
) -> Vec<VerusGuardFact> {
    let mut facts = Vec::new();
    let mut index = 0usize;
    while index < region_lines.len() {
        let absolute_line = region_start_line + index;
        let (attrs, consumed) = collect_leading_attributes(region_lines, index);
        index += consumed;
        if index >= region_lines.len() {
            break;
        }
        let Some((mode, name, mut header_index)) = parse_item_header(region_lines, index) else {
            index += 1;
            continue;
        };
        let header_start = absolute_line;
        let trust = trust_from_attributes(&attrs);
        let mut requires = Vec::new();
        let mut ensures = Vec::new();
        let mut recommends = Vec::new();
        while header_index < region_lines.len() {
            let line = region_lines[header_index].trim();
            if line.is_empty() {
                header_index += 1;
                continue;
            }
            if line.starts_with('}') || line == "}" {
                break;
            }
            if line.contains('{') {
                break;
            }
            if let Some((kind, text)) = parse_clause_line(line) {
                let clause_line = region_start_line + header_index;
                let fact = VerusClauseFact {
                    kind,
                    line: clause_line,
                    text,
                };
                match kind {
                    VerusClauseKind::Requires => requires.push(fact),
                    VerusClauseKind::Ensures => ensures.push(fact),
                    VerusClauseKind::Recommends => recommends.push(fact),
                }
                header_index += 1;
                continue;
            }
            break;
        }
        let header_end = region_start_line + header_index.saturating_sub(1);
        facts.push(VerusGuardFact {
            owner_name: name,
            mode,
            header_start_line: header_start,
            header_end_line: header_end.max(header_start),
            region_start_line,
            region_end_line,
            requires,
            ensures,
            recommends,
            trust,
            parser_version: PARSER_VERSION.to_string(),
        });
        index = header_index.max(index + 1);
    }
    facts
}

fn collect_leading_attributes(region_lines: &[&str], mut index: usize) -> (Vec<String>, usize) {
    let start = index;
    let mut attrs = Vec::new();
    while index < region_lines.len() {
        let trimmed = region_lines[index].trim();
        if trimmed.is_empty() {
            index += 1;
            continue;
        }
        if trimmed.starts_with("#[") {
            attrs.push(trimmed.to_string());
            index += 1;
            continue;
        }
        break;
    }
    (attrs, index - start)
}

fn trust_from_attributes(attrs: &[String]) -> VerusTrustBoundary {
    for attr in attrs {
        let lower = attr.to_ascii_lowercase();
        if lower.contains("external_body") {
            return VerusTrustBoundary::ExternalBody;
        }
        if lower.contains("verifier::external") || lower.contains("verifier(external") {
            return VerusTrustBoundary::VerifierExternal;
        }
        if lower.contains("assume_specification") || lower.contains("assume(") {
            return VerusTrustBoundary::Assume;
        }
        if lower.contains("admit(") || lower.contains("admit ") {
            return VerusTrustBoundary::Admit;
        }
    }
    VerusTrustBoundary::Checked
}

fn parse_item_header(
    region_lines: &[&str],
    index: usize,
) -> Option<(VerusItemMode, String, usize)> {
    let line = region_lines[index].trim();
    if line.is_empty() || line.starts_with("//") {
        return None;
    }
    let mut tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.first() == Some(&"pub") {
        tokens.remove(0);
    }
    let (mode, fn_index) = if tokens.first() == Some(&"spec") && tokens.get(1) == Some(&"fn") {
        (VerusItemMode::Spec, 2)
    } else if tokens.first() == Some(&"proof") && tokens.get(1) == Some(&"fn") {
        (VerusItemMode::Proof, 2)
    } else if tokens.first() == Some(&"fn") {
        (VerusItemMode::Exec, 1)
    } else {
        return None;
    };
    let name_token = tokens.get(fn_index)?;
    let name = name_token.split('(').next().unwrap_or("").trim();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some((mode, name.to_string(), index + 1))
}

fn parse_clause_line(line: &str) -> Option<(VerusClauseKind, String)> {
    let trimmed = line.trim().trim_end_matches(',');
    let (kind, rest) = if let Some(rest) = trimmed.strip_prefix("requires") {
        (VerusClauseKind::Requires, rest.trim())
    } else if let Some(rest) = trimmed.strip_prefix("ensures") {
        (VerusClauseKind::Ensures, rest.trim())
    } else if let Some(rest) = trimmed.strip_prefix("recommends") {
        (VerusClauseKind::Recommends, rest.trim())
    } else {
        return None;
    };
    if rest.is_empty() {
        return None;
    }
    Some((kind, rest.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_requires_ensures_produce_owner_bound_guard_fact() {
        let source = "use vstd::prelude::*;\nverus! {\nfn inc(x: u32) -> (r: u32)\n    requires x < 100,\n    ensures r <= 200,\n{\n    x + 2\n}\n}\n";
        let facts = verus_guard_facts(source);
        assert_eq!(facts.len(), 1);
        let fact = &facts[0];
        assert_eq!(fact.owner_name, "inc");
        assert_eq!(fact.mode, VerusItemMode::Exec);
        assert_eq!(fact.requires.len(), 1);
        assert_eq!(fact.ensures.len(), 1);
        assert_eq!(fact.trust, VerusTrustBoundary::Checked);
        assert_eq!(fact.parser_version, PARSER_VERSION);
    }

    #[test]
    fn spec_and_proof_modes_stay_distinct() {
        let source = "verus! {\nspec fn s(x: u32) -> bool\n    recommends x > 0,\n{ x > 1 }\nproof fn p(x: int) {\n    assert(x >= 0);\n}\n}\n";
        let facts = verus_guard_facts(source);
        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].mode, VerusItemMode::Spec);
        assert_eq!(facts[0].recommends.len(), 1);
        assert_eq!(facts[0].requires.len(), 0);
        assert_eq!(facts[1].mode, VerusItemMode::Proof);
    }

    #[test]
    fn external_body_records_trust_without_implies_verified_body() {
        let source = "verus! {\n#[verifier(external_body)]\nfn trusted(x: u32) -> (r: u32)\n    ensures r == x,\n{\n    x\n}\n}\n";
        let facts = verus_guard_facts(source);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].trust, VerusTrustBoundary::ExternalBody);
    }

    #[test]
    fn comment_and_string_verus_tokens_do_not_parse() {
        let source = "// verus! { fn f() {} }\nfn g() {}\n";
        assert!(verus_guard_facts(source).is_empty());
    }

    #[test]
    fn guard_fact_at_line_resolves_header_clause() {
        let source = "verus! {\nfn f(x: u32) -> (r: u32)\n    ensures r == x,\n{ x }\n}\n";
        let facts = verus_guard_facts(source);
        assert!(guard_fact_at_line(&facts, 3).is_some_and(|f| f.owner_name == "f"));
    }

    #[test]
    fn same_name_outside_verus_region_is_not_admitted() {
        let source = "fn inc(x: u32) -> u32 { x }\nverus! {\nfn inc(x: u32) -> (r: u32)\n    ensures r == x,\n{ x }\n}\n";
        let facts = verus_guard_facts(source);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].region_start_line, 2);
    }
}

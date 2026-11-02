//! Structured acknowledgement ledger for scoped gate decisions (#1966).
//!
//! PR labels may trigger acknowledgement, but scoped records bind to explicit
//! canonical gap identity (and optional seam narrowing). Legacy `ripr-waive`
//! remains a global compatibility path when no structured ledger is supplied,
//! or for unmatched blocking candidates when a label is present alongside
//! structured input.

use crate::output::suppressions::is_iso_date;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const SCHEMA_VERSION: u32 = 1;

pub(crate) const MATCH_KIND_EXACT_CANONICAL: &str = "exact_canonical_gap_id";
pub(crate) const MATCH_KIND_LEGACY_LABEL: &str = "legacy_acknowledgement_label";

pub(crate) const LIMITATION_EXPIRED: &str = "acknowledgement_expired";
pub(crate) const LIMITATION_OVER_MATCH: &str = "acknowledgement_over_max_matches";
pub(crate) const LIMITATION_WRONG_ROOT: &str = "acknowledgement_wrong_root";
#[expect(
    dead_code,
    reason = "policy:1966: reserved for future wrong-repository row matching"
)]
pub(crate) const LIMITATION_MALFORMED: &str = "acknowledgement_malformed";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StructuredAcknowledgementRecord {
    pub(crate) acknowledgement_id: String,
    pub(crate) canonical_gap_id: String,
    pub(crate) seam_id: Option<String>,
    pub(crate) reason: String,
    pub(crate) actor: String,
    pub(crate) created_at: String,
    pub(crate) expires_at: Option<String>,
    pub(crate) review_after: Option<String>,
    pub(crate) max_matches: u32,
    pub(crate) repository: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct StructuredAcknowledgementLedger {
    pub(crate) root: Option<String>,
    pub(crate) records: Vec<StructuredAcknowledgementRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AppliedStructuredAcknowledgement {
    pub(crate) acknowledgement_id: String,
    pub(crate) match_kind: String,
    pub(crate) actor: String,
    pub(crate) reason: String,
    pub(crate) expires_at: Option<String>,
    pub(crate) review_after: Option<String>,
    pub(crate) global_scope: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CandidateAcknowledgement {
    None,
    Applied(AppliedStructuredAcknowledgement),
    LegacyGlobal { label: String },
    Limitation { kind: String, detail: String },
}

#[derive(Default)]
pub(crate) struct MatchCounters {
    per_id: BTreeMap<String, u32>,
}

impl MatchCounters {
    pub(crate) fn count(&self, id: &str) -> u32 {
        self.per_id.get(id).copied().unwrap_or(0)
    }

    pub(crate) fn record_match(&mut self, id: &str) {
        *self.per_id.entry(id.to_string()).or_insert(0) += 1;
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLedger {
    schema_version: u32,
    root: Option<String>,
    #[serde(default)]
    acknowledgements: Vec<RawRecord>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRecord {
    acknowledgement_id: String,
    canonical_gap_id: String,
    seam_id: Option<String>,
    reason: String,
    actor: String,
    created_at: String,
    expires_at: Option<String>,
    review_after: Option<String>,
    #[serde(default = "default_max_matches")]
    max_matches: u32,
    repository: Option<String>,
}

fn default_max_matches() -> u32 {
    1
}

pub(crate) fn validate_ledger_root(
    ledger: &StructuredAcknowledgementLedger,
    gate_root: &str,
    display: &str,
) -> Result<(), String> {
    if let Some(root) = ledger.root.as_deref()
        && normalize_root(root) != normalize_root(gate_root)
    {
        return Err(format!(
            "{LIMITATION_WRONG_ROOT}: structured acknowledgements `{display}` root `{root}` does not match gate root `{gate_root}`"
        ));
    }
    Ok(())
}

pub(crate) fn load_structured_acknowledgements_from_text(
    text: &str,
    display: &str,
) -> Result<StructuredAcknowledgementLedger, String> {
    let raw: RawLedger = toml::de::from_str(text).map_err(|err| {
        format!(
            "structured acknowledgements `{display}` is not valid TOML: {err}; fix the ledger or omit --acknowledgements when using label-only compatibility"
        )
    })?;
    if raw.schema_version != SCHEMA_VERSION {
        return Err(format!(
            "structured acknowledgements `{display}` has unsupported schema_version {}; expected {SCHEMA_VERSION}",
            raw.schema_version
        ));
    }
    let mut seen_ids = BTreeSet::new();
    let mut records = Vec::new();
    for (index, record) in raw.acknowledgements.into_iter().enumerate() {
        let line_hint = index + 1;
        validate_record_fields(&record, display, line_hint)?;
        if !seen_ids.insert(record.acknowledgement_id.clone()) {
            return Err(format!(
                "structured acknowledgements `{display}` contains duplicate acknowledgement_id `{}`",
                record.acknowledgement_id
            ));
        }
        records.push(StructuredAcknowledgementRecord {
            acknowledgement_id: record.acknowledgement_id,
            canonical_gap_id: record.canonical_gap_id,
            seam_id: record.seam_id,
            reason: record.reason,
            actor: record.actor,
            created_at: record.created_at,
            expires_at: record.expires_at,
            review_after: record.review_after,
            max_matches: record.max_matches.max(1),
            repository: record.repository,
        });
    }
    Ok(StructuredAcknowledgementLedger {
        root: raw.root,
        records,
    })
}

fn validate_record_fields(record: &RawRecord, display: &str, index: usize) -> Result<(), String> {
    let prefix = format!("structured acknowledgements `{display}` record #{index}");
    require_non_blank(
        &record.acknowledgement_id,
        &format!("{prefix} acknowledgement_id"),
    )?;
    require_non_blank(
        &record.canonical_gap_id,
        &format!("{prefix} canonical_gap_id"),
    )?;
    require_non_blank(&record.reason, &format!("{prefix} reason"))?;
    require_non_blank(&record.actor, &format!("{prefix} actor"))?;
    validate_optional_date(&record.created_at, &format!("{prefix} created_at"))?;
    if let Some(expires) = &record.expires_at {
        validate_optional_date(expires, &format!("{prefix} expires_at"))?;
    }
    if let Some(review) = &record.review_after {
        validate_optional_date(review, &format!("{prefix} review_after"))?;
    }
    if record.max_matches == 0 {
        return Err(format!("{prefix} max_matches must be at least 1 (got 0)"));
    }
    Ok(())
}

fn require_non_blank(value: &str, field: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{field} must be non-empty"))
    } else {
        Ok(())
    }
}

fn validate_optional_date(value: &str, field: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{field} must be non-empty"))
    } else if is_iso_date(value) {
        Ok(())
    } else {
        Err(format!(
            "{field} must be an ISO-8601 YYYY-MM-DD date (got `{value}`)"
        ))
    }
}

pub(crate) fn resolve_candidate_acknowledgement(
    ledger: Option<&StructuredAcknowledgementLedger>,
    counters: &mut MatchCounters,
    today: &str,
    gate_root: &str,
    canonical_gap_id: Option<&str>,
    seam_id: Option<&str>,
    legacy_label: Option<&str>,
    structured_loaded: bool,
) -> CandidateAcknowledgement {
    if let Some(ledger) = ledger {
        if let Some(canonical) = canonical_gap_id {
            for record in &ledger.records {
                if record.canonical_gap_id != canonical {
                    continue;
                }
                if let Some(expected_seam) = record.seam_id.as_deref()
                    && seam_id != Some(expected_seam)
                {
                    continue;
                }
                if let Some(repo) = record.repository.as_deref()
                    && normalize_root(repo) != normalize_root(gate_root)
                {
                    continue;
                }
                if let Some(expires) = record.expires_at.as_deref()
                    && expires < today
                {
                    return CandidateAcknowledgement::Limitation {
                        kind: LIMITATION_EXPIRED.to_string(),
                        detail: format!(
                            "acknowledgement `{}` expired on {expires}",
                            record.acknowledgement_id
                        ),
                    };
                }
                let used = counters.count(&record.acknowledgement_id);
                if used >= record.max_matches {
                    return CandidateAcknowledgement::Limitation {
                        kind: LIMITATION_OVER_MATCH.to_string(),
                        detail: format!(
                            "acknowledgement `{}` exceeded max_matches={}",
                            record.acknowledgement_id, record.max_matches
                        ),
                    };
                }
                counters.record_match(&record.acknowledgement_id);
                return CandidateAcknowledgement::Applied(AppliedStructuredAcknowledgement {
                    acknowledgement_id: record.acknowledgement_id.clone(),
                    match_kind: MATCH_KIND_EXACT_CANONICAL.to_string(),
                    actor: record.actor.clone(),
                    reason: record.reason.clone(),
                    expires_at: record.expires_at.clone(),
                    review_after: record.review_after.clone(),
                    global_scope: false,
                });
            }
        }
        if let Some(label) = legacy_label {
            return CandidateAcknowledgement::LegacyGlobal {
                label: label.to_string(),
            };
        }
        if structured_loaded {
            return CandidateAcknowledgement::None;
        }
    }
    if let Some(label) = legacy_label {
        return CandidateAcknowledgement::LegacyGlobal {
            label: label.to_string(),
        };
    }
    CandidateAcknowledgement::None
}

fn normalize_root(value: &str) -> String {
    value.replace('\\', "/").trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_ids_fail_closed() -> Result<(), String> {
        let text = r#"
schema_version = 1
[[acknowledgements]]
acknowledgement_id = "dup"
canonical_gap_id = "gap:a"
reason = "r"
actor = "a"
created_at = "2026-01-01"
[[acknowledgements]]
acknowledgement_id = "dup"
canonical_gap_id = "gap:b"
reason = "r"
actor = "a"
created_at = "2026-01-01"
"#;
        let err = load_structured_acknowledgements_from_text(text, "ledger.toml")
            .err()
            .ok_or_else(|| "expected duplicate failure".to_string())?;
        assert!(err.contains("duplicate acknowledgement_id"));
        Ok(())
    }

    #[test]
    fn scoped_match_respects_max_matches() -> Result<(), String> {
        let text = r#"
schema_version = 1
[[acknowledgements]]
acknowledgement_id = "one"
canonical_gap_id = "gap:a"
reason = "r"
actor = "a"
created_at = "2026-01-01"
max_matches = 1
"#;
        let ledger = load_structured_acknowledgements_from_text(text, "ledger.toml")?;
        let mut counters = MatchCounters::default();
        let first = resolve_candidate_acknowledgement(
            Some(&ledger),
            &mut counters,
            "2026-06-01",
            "/repo",
            Some("gap:a"),
            None,
            None,
            true,
        );
        assert!(matches!(first, CandidateAcknowledgement::Applied(_)));
        let second = resolve_candidate_acknowledgement(
            Some(&ledger),
            &mut counters,
            "2026-06-01",
            "/repo",
            Some("gap:a"),
            None,
            None,
            true,
        );
        assert!(matches!(
            second,
            CandidateAcknowledgement::Limitation { .. }
        ));
        Ok(())
    }
}

//! Structured acknowledgement ledger for scoped gate decisions (#1966).
//!
//! PR labels may trigger acknowledgement, but scoped records bind to explicit
//! canonical gap identity (and optional seam narrowing). Legacy `ripr-waive`
//! remains a global compatibility path when no structured ledger is supplied,
//! or for unmatched blocking candidates when a label is present alongside
//! structured input.

use crate::output::baseline_delta::normalize_root_for_comparison;
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
        && normalize_root_for_comparison(root) != normalize_root_for_comparison(gate_root)
    {
        return Err(format!(
            "{LIMITATION_WRONG_ROOT}: structured acknowledgements `{display}` root `{root}` does not match gate root `{gate_root}`; fix the ledger root or run the gate from the matching repository root"
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
            "structured acknowledgements `{display}` has unsupported schema_version {}; expected {SCHEMA_VERSION}; set schema_version to {SCHEMA_VERSION} in the ledger",
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
        return Err(format!(
            "{prefix} max_matches must be at least 1 (got 0); set max_matches to 1 or more"
        ));
    }
    Ok(())
}

fn require_non_blank(value: &str, field: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!(
            "{field} must be non-empty; add a value for it in the acknowledgements ledger"
        ))
    } else {
        Ok(())
    }
}

fn validate_optional_date(value: &str, field: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!(
            "{field} must be non-empty; add a value for it in the acknowledgements ledger"
        ))
    } else if is_iso_date(value) {
        Ok(())
    } else {
        Err(format!(
            "{field} must be an ISO-8601 YYYY-MM-DD date (got `{value}`); use a date such as 2026-12-31"
        ))
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "policy:1966: structured acknowledgement matching needs ledger, counters, identity, and legacy label context"
)]
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
            // A renewal row is usually appended beside the row it replaces, so an
            // expired or exhausted row must not hide a later row that applies (#13032).
            // Keep the first limitation and return it only when no row applies.
            let mut limitation: Option<CandidateAcknowledgement> = None;
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
                    && normalize_root_for_comparison(repo)
                        != normalize_root_for_comparison(gate_root)
                {
                    continue;
                }
                if let Some(expires) = record.expires_at.as_deref()
                    && expires < today
                {
                    limitation.get_or_insert_with(|| CandidateAcknowledgement::Limitation {
                        kind: LIMITATION_EXPIRED.to_string(),
                        detail: format!(
                            "acknowledgement `{}` expired on {expires}",
                            record.acknowledgement_id
                        ),
                    });
                    continue;
                }
                let used = counters.count(&record.acknowledgement_id);
                if used >= record.max_matches {
                    limitation.get_or_insert_with(|| CandidateAcknowledgement::Limitation {
                        kind: LIMITATION_OVER_MATCH.to_string(),
                        detail: format!(
                            "acknowledgement `{}` exceeded max_matches={}",
                            record.acknowledgement_id, record.max_matches
                        ),
                    });
                    continue;
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
            // Every matching row was limited: report the limitation, never the
            // legacy label, so an expired row cannot become a global waiver.
            if let Some(limitation) = limitation {
                return limitation;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equivalent_root_spellings_match_gate_root() -> Result<(), String> {
        let ledger = StructuredAcknowledgementLedger {
            root: Some("repo".to_string()),
            records: Vec::new(),
        };
        validate_ledger_root(&ledger, "./repo", "ledger.toml")?;
        Ok(())
    }

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

    fn resolve_for_gap_a(
        ledger: &StructuredAcknowledgementLedger,
        counters: &mut MatchCounters,
        legacy_label: Option<&str>,
    ) -> CandidateAcknowledgement {
        resolve_candidate_acknowledgement(
            Some(ledger),
            counters,
            "2026-06-01",
            "/repo",
            Some("gap:a"),
            None,
            legacy_label,
            true,
        )
    }

    fn two_row_ledger(
        first: &str,
        second: &str,
    ) -> Result<StructuredAcknowledgementLedger, String> {
        let text = format!(
            r#"
schema_version = 1
[[acknowledgements]]
acknowledgement_id = "first"
canonical_gap_id = "gap:a"
reason = "r"
actor = "a"
created_at = "2026-01-01"
{first}
[[acknowledgements]]
acknowledgement_id = "second"
canonical_gap_id = "gap:a"
reason = "r"
actor = "a"
created_at = "2026-01-01"
{second}
"#
        );
        load_structured_acknowledgements_from_text(&text, "ledger.toml")
    }

    fn applied_id(result: &CandidateAcknowledgement) -> Result<&str, String> {
        match result {
            CandidateAcknowledgement::Applied(applied) => Ok(applied.acknowledgement_id.as_str()),
            other => Err(format!(
                "expected an applied acknowledgement, got {other:?}"
            )),
        }
    }

    #[test]
    fn renewed_row_wins_over_an_earlier_expired_row() -> Result<(), String> {
        let ledger = two_row_ledger(
            r#"expires_at = "2026-03-01""#,
            r#"expires_at = "2026-12-31""#,
        )?;
        let mut counters = MatchCounters::default();
        let result = resolve_for_gap_a(&ledger, &mut counters, None);
        assert_eq!(applied_id(&result)?, "second");
        assert_eq!(counters.count("first"), 0);
        assert_eq!(counters.count("second"), 1);
        Ok(())
    }

    #[test]
    fn valid_row_wins_regardless_of_order_against_an_expired_row() -> Result<(), String> {
        let ledger = two_row_ledger(
            r#"expires_at = "2026-12-31""#,
            r#"expires_at = "2026-03-01""#,
        )?;
        let mut counters = MatchCounters::default();
        let result = resolve_for_gap_a(&ledger, &mut counters, None);
        assert_eq!(applied_id(&result)?, "first");
        assert_eq!(counters.count("second"), 0);
        Ok(())
    }

    #[test]
    fn renewed_row_wins_over_an_earlier_exhausted_row() -> Result<(), String> {
        let ledger = two_row_ledger("max_matches = 1", "max_matches = 1")?;
        let mut counters = MatchCounters::default();
        let first = resolve_for_gap_a(&ledger, &mut counters, None);
        assert_eq!(applied_id(&first)?, "first");
        let second = resolve_for_gap_a(&ledger, &mut counters, None);
        assert_eq!(applied_id(&second)?, "second");
        let third = resolve_for_gap_a(&ledger, &mut counters, None);
        match third {
            CandidateAcknowledgement::Limitation { kind, detail } => {
                assert_eq!(kind, LIMITATION_OVER_MATCH);
                assert!(
                    detail.contains("`first`"),
                    "first limitation is kept: {detail}"
                );
            }
            other => return Err(format!("expected over-match limitation, got {other:?}")),
        }
        Ok(())
    }

    #[test]
    fn all_matching_rows_limited_reports_the_first_limitation_not_the_legacy_label()
    -> Result<(), String> {
        let ledger = two_row_ledger(
            r#"expires_at = "2026-03-01""#,
            r#"expires_at = "2026-04-01""#,
        )?;
        let mut counters = MatchCounters::default();
        let result = resolve_for_gap_a(&ledger, &mut counters, Some("ripr-waive"));
        match result {
            CandidateAcknowledgement::Limitation { kind, detail } => {
                assert_eq!(kind, LIMITATION_EXPIRED);
                assert!(
                    detail.contains("`first`"),
                    "first limitation is kept: {detail}"
                );
            }
            other => return Err(format!("expected expired limitation, got {other:?}")),
        }
        assert_eq!(counters.count("first") + counters.count("second"), 0);
        Ok(())
    }
}

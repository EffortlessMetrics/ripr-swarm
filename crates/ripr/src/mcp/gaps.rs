//! Canonical gap-item projection for the MCP adapter.
//!
//! A [`GapItem`] is the MCP-facing view of one producer-owned
//! [`crate::domain::Finding`]. The projection is built once when a completed
//! snapshot is committed; MCP never re-runs classification, ranking, or
//! evidence production. List responses carry only the small summary document;
//! the complete bounded evidence document is served lazily by
//! `ripr_get_gap` / `ripr://gap/{canonical_id}`.

use super::repair_admission::{SeamAdmission, SeamVerdict};
use crate::domain::Finding;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;

pub(crate) const GAP_LIST_SCHEMA_VERSION: &str = "ripr-mcp-gap-list-v2";
pub(crate) const GAP_SCHEMA_VERSION: &str = "ripr-mcp-gap-v1";

/// `ripr_get_gap` / `ripr://gap/{canonical_id}` never authorizes an
/// edit by itself: the readiness block reports the producer repair-readiness
/// facts projected at snapshot commit time, and `ripr_prepare_repair` (#3090)
/// is the only route that binds a repair transaction — and only when every
/// readiness gate is established.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RepairFixSite {
    pub(crate) test_name: String,
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) oracle: Option<String>,
    pub(crate) oracle_kind: &'static str,
}

/// Producer-fact repair readiness for one canonical item, computed once when
/// the snapshot is committed. The evaluation is a fail-closed conjunction of
/// producer facts already on the finding; MCP never upgrades a missing fact
/// and never runs its own classification:
///
/// 1. candidate actionability — the shared `#3281` predicate
///    [`Finding::is_candidate_actionable`];
/// 2. an established discriminator — the canonical gap names a non-empty
///    normalized discriminator, and any producer-named missing discriminator
///    sits on a weakly exposed finding, where the named value is the repair
///    target (#8717). A producer that names its missing discriminators on
///    any other class refuses as `missing_discriminator`; a finding whose
///    canonical gap was withheld behind the finding's own typed static
///    limitation refuses as
///    `static_limitation`; only a producer that names no missing
///    discriminator and populates no canonical gap at all for the language
///    refuses as `discriminator_not_populated_for_language`, so one document
///    cannot contradict its own `discriminator_availability` block (#5268);
/// 3. an established fix site — a strong, high-confidence directly-related
///    test, on a path the shared edit-cage test-surface predicate accepts.
///
/// The first failing gate in this fixed order is the typed reason.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RepairReadiness {
    pub(crate) ready: bool,
    pub(crate) fix_site: Option<RepairFixSite>,
    /// First failing gate (`not_candidate_actionable`, `missing_discriminator`,
    /// `discriminator_not_populated_for_language`, `static_limitation`,
    /// `fix_site_not_established`, `fix_site_not_test_surface`); `None` only
    /// when every gate is established.
    pub(crate) ineligibility: Option<&'static str>,
    /// The shared repair validator's own explanation when the seam-level
    /// admission refused an otherwise finding-ready item (#8717).
    pub(crate) seam_detail: Option<String>,
}

impl RepairReadiness {
    /// Conjoin the shared seam-level repair validator's verdict (#8717).
    /// A finding-level refusal stays the first failing gate, except that a
    /// truncated inventory refuses first; a finding-ready item becomes not
    /// ready, keeping no fix site, when the validator that backs `ripr repair`
    /// does not admit its seam.
    pub(crate) fn with_seam_verdict(mut self, verdict: Option<&SeamVerdict>) -> Self {
        // The CLI errors on a truncated inventory before it resolves any item
        // (#9108), so that refusal outranks every finding-level gate.
        if let Some(SeamVerdict::Refused { code, detail }) = verdict
            && (self.ready || *code == crate::mcp::repair_admission::SEAM_INVENTORY_TRUNCATED)
        {
            self.ready = false;
            self.fix_site = None;
            self.ineligibility = Some(*code);
            self.seam_detail = Some(detail.clone());
        }
        self
    }

    /// The human sentence for this readiness: the shared validator's own
    /// explanation when it refused, else the finding-level gate sentence.
    pub(crate) fn detail(&self) -> String {
        match &self.seam_detail {
            Some(detail) => detail.clone(),
            None => self.reason().to_string(),
        }
    }

    pub(crate) fn from_finding(finding: &Finding, root: &Path) -> Self {
        if !finding.is_candidate_actionable() {
            return Self {
                ready: false,
                fix_site: None,
                ineligibility: Some("not_candidate_actionable"),
                seam_detail: None,
            };
        }
        let producer_named_missing_discriminator =
            !finding.activation.missing_discriminators.is_empty()
                || finding.missing.iter().any(|entry| {
                    entry.starts_with(crate::domain::MISSING_DISCRIMINATOR_VALUE_PREFIX)
                });
        // A named missing discriminator on a weakly exposed finding is the
        // repair target itself (#8717): the existing test reaches the changed
        // behavior without pinning the named value, and `ripr repair` extends
        // that test. It stays a refusal for every other class, where the
        // missing value does not describe a weak-test repair. Whether the
        // item is actually startable remains the shared seam validator's
        // decision, conjoined after these finding-level gates.
        let named_missing_is_repair_target = producer_named_missing_discriminator
            && finding.class == crate::domain::ExposureClass::WeaklyExposed;
        if producer_named_missing_discriminator && !named_missing_is_repair_target {
            return Self {
                ready: false,
                fix_site: None,
                ineligibility: Some("missing_discriminator"),
                seam_detail: None,
            };
        }
        let discriminator_unpopulated = finding
            .canonical_gap
            .as_ref()
            .is_none_or(|gap| gap.normalized_discriminator.trim().is_empty());
        if discriminator_unpopulated {
            // The refusal must name the actual producer condition: a finding
            // the producer withheld behind its own typed static limitation
            // (Python omits the canonical gap exactly then) refuses as
            // `static_limitation`; only a language producer that does not
            // populate canonical gaps at all refuses as
            // `discriminator_not_populated_for_language` (#5268).
            let ineligibility = if finding.static_limit_kind.is_some() {
                "static_limitation"
            } else {
                "discriminator_not_populated_for_language"
            };
            return Self {
                ready: false,
                fix_site: None,
                ineligibility: Some(ineligibility),
                seam_detail: None,
            };
        }
        // The gate contract is "a strong, high-confidence directly-related
        // test on a shared edit-cage test-surface path", so the path
        // predicate joins the search: a qualifying test-surface candidate
        // later in the list wins over a non-surface candidate earlier.
        let candidates = finding
            .related_tests
            .iter()
            .filter(|test| {
                test.oracle_strength == crate::domain::OracleStrength::Strong
                    && test.relation_confidence == Some(crate::domain::RelationConfidence::High)
            })
            .map(|test| RepairFixSite {
                test_name: test.name.clone(),
                file: crate::output::path::repository_relative_path_text(root, &test.file),
                line: test.line,
                oracle: test.oracle.clone(),
                oracle_kind: test.oracle_kind.as_str(),
            })
            .collect::<Vec<_>>();
        let Some(first) = candidates.first().cloned() else {
            return Self {
                ready: false,
                fix_site: None,
                ineligibility: Some("fix_site_not_established"),
                seam_detail: None,
            };
        };
        let Some(fix_site) = candidates
            .into_iter()
            .find(|site| crate::analysis::is_test_surface_path(&site.file))
        else {
            return Self {
                ready: false,
                fix_site: Some(first),
                ineligibility: Some("fix_site_not_test_surface"),
                seam_detail: None,
            };
        };
        Self {
            ready: true,
            fix_site: Some(fix_site),
            ineligibility: None,
            seam_detail: None,
        }
    }

    pub(crate) fn reason(&self) -> &'static str {
        match self.ineligibility {
            Some("not_candidate_actionable") => {
                "the producer did not establish the changed source as candidate-current, so no repair target is actionable (#3281)"
            }
            Some("missing_discriminator") => {
                "the producer did not establish a discriminator for the changed behavior (no normalized discriminator, or a producer-named missing discriminator on a finding that is not weakly exposed)"
            }
            Some("discriminator_not_populated_for_language") => {
                "the producer named no missing discriminator but has not populated a normalized discriminator for this language's findings yet, so the discriminator gate cannot be established from available producer facts; repair stays unavailable until the language producer populates canonical gaps (#5268)"
            }
            Some("static_limitation") => {
                "the producer withheld this finding's canonical gap behind a typed static limitation on the finding itself; the limitation, not the language, explains why no normalized discriminator is established"
            }
            Some("fix_site_not_established") => {
                "no strong, high-confidence directly-related test establishes an exact fix site"
            }
            Some("fix_site_not_test_surface") => {
                "the strongest established fix site is not a test-surface path; only test-surface paths can be the authored edit target (inline `#[cfg(test)]` modules don't qualify their file)"
            }
            Some(
                crate::mcp::repair_admission::SEAM_ALREADY_GRIPPED
                | crate::mcp::repair_admission::SEAM_GATE_REFUSED
                | crate::mcp::repair_admission::SEAM_AMBIGUOUS
                | crate::mcp::repair_admission::SEAM_NOT_FOUND
                | crate::mcp::repair_admission::SEAM_INVENTORY_TRUNCATED,
            ) => {
                "the shared seam-level repair validator behind `ripr repair` does not admit this item"
            }
            Some(_other) => "the producer did not establish every repair-readiness fact",
            None => {
                "every evaluated producer fact is established: candidate-current source, an established discriminator, and a strong directly-related test fix site on a test surface"
            }
        }
    }
}

pub(crate) struct GapItem {
    pub(crate) canonical_id: String,
    pub(crate) finding_id: String,
    pub(crate) file: String,
    /// Exposure class wire token (`exposed`, `weakly_exposed`, ...) captured
    /// at projection time, so list filters re-read stored facts (#7464).
    class: String,
    /// Language wire string (`rust`, `python`, ...) or `None` when the
    /// producer populated no language; stored for the same filter route.
    language: Option<String>,
    /// Small deterministic document used by `ripr_list_gaps`; its serialized
    /// byte length is the item's budget payload bytes. It projects stored
    /// class/language facts and the source line alongside the evidence core.
    /// The summary also carries the commit-time repair-readiness projection
    /// (`repair_packet_ready` plus the typed first-failing-gate
    /// `repair_ineligibility`, null when ready), so agents can sort
    /// actionable items without one `ripr_get_gap` call per candidate
    /// (#7464).
    pub(crate) list_summary: Value,
    list_summary_bytes: usize,
    /// Complete bounded evidence document (without the per-request snapshot
    /// binding and links, which are injected at serve time). The repair
    /// projection reads `changed_behavior` and the discriminator from it.
    pub(crate) evidence_core: Value,
    pub(crate) evidence_bytes: usize,
    /// Producer candidate-actionability captured at projection time; the
    /// shared budget maps it to eligibility so MCP never invents its own.
    candidate_actionable: bool,
    /// Deterministic digest of `evidence_core`, bound into the snapshot
    /// identity so an evidence change yields a new snapshot id instead of
    /// serving altered bytes under the old identity.
    pub(crate) evidence_sha256: String,
    /// Producer-fact repair readiness projected at commit time; the evidence
    /// core embeds it so the snapshot identity binds readiness changes too.
    pub(crate) repair_readiness: RepairReadiness,
}

impl GapItem {
    /// `root` is the analyzed workspace root: item locations render through
    /// the shared finding-location owner (#5996), so an MCP item names the
    /// finding's file with the same workspace-relative string the check,
    /// context and LSP surfaces emit. Related-test and fix-site files keep
    /// the repository-relative renderer (#5254 item 6), which additionally
    /// tolerates verbatim/UNC/case spelling drift.
    #[cfg(test)]
    pub(crate) fn from_finding(finding: &Finding, root: &Path) -> Result<Self, String> {
        Self::from_finding_admitted(finding, root, &SeamAdmission::unevaluated())
    }

    /// [`Self::from_finding`] with the shared seam-level repair validator's
    /// verdict conjoined into the readiness projection (#8717), so the
    /// list summary, the evidence document, the snapshot identity and
    /// `ripr_prepare_repair` all carry the one admission decision.
    pub(crate) fn from_finding_admitted(
        finding: &Finding,
        root: &Path,
        admission: &SeamAdmission,
    ) -> Result<Self, String> {
        let canonical_id = finding
            .canonical_gap
            .as_ref()
            .map(|gap| gap.id.clone())
            .unwrap_or_else(|| finding.id.clone());
        let file = crate::analysis::finding_location_text(root, &finding.probe.location.file);
        let class = finding.class.as_str().to_string();
        let language = finding
            .language
            .as_ref()
            .map(|language| language.as_str().to_string());
        let line = finding.probe.location.line;

        let repair_readiness = RepairReadiness::from_finding(finding, root)
            .with_seam_verdict(admission.verdict(&finding.id).as_ref());
        let list_summary = json!({
            "canonical_id": canonical_id,
            "class": class,
            "language": language,
            "file": file,
            "line": line,
            "repair_packet_ready": repair_readiness.ready,
            "repair_ineligibility": repair_readiness.ineligibility,
        });
        let list_summary_bytes = serialized_bytes(&list_summary)?;

        let evidence_core = gap_evidence_core(finding, &canonical_id, root, &repair_readiness)?;
        let (evidence_bytes, evidence_sha256) = serialized_evidence_identity(&evidence_core)?;
        Ok(Self {
            canonical_id,
            finding_id: finding.id.clone(),
            file,
            class,
            language,
            list_summary,
            list_summary_bytes,
            evidence_core,
            evidence_bytes,
            candidate_actionable: finding.is_candidate_actionable(),
            evidence_sha256,
            repair_readiness,
        })
    }

    pub(crate) fn class(&self) -> &str {
        &self.class
    }

    pub(crate) fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    /// Commit-time repair readiness: `true` only when every producer gate
    /// was established. Matches the `readiness.repair_packet_ready` fact the
    /// same item's `ripr_get_gap` document reports (#7464).
    pub(crate) fn repair_ready(&self) -> bool {
        self.repair_readiness.ready
    }

    pub(crate) fn list_summary_bytes(&self) -> usize {
        self.list_summary_bytes
    }

    /// The complete item document for one committed snapshot. The caller
    /// binds the snapshot identity and the resource links; a missing repair
    /// link stays an explicit `null`, never prose invention.
    pub(crate) fn document(&self, snapshot_id: &str) -> Value {
        json!({
            "schema_version": GAP_SCHEMA_VERSION,
            "snapshot_id": snapshot_id,
            "item": serde_json::Value::Object({
                let mut object = self
                    .evidence_core
                    .as_object()
                    .cloned()
                    .unwrap_or_default();
                object.insert(
                    "links".to_string(),
                    json!({
                        "snapshot": format!("ripr://snapshot/{snapshot_id}"),
                        "gap": format!("ripr://gap/{}", self.canonical_id),
                        "repair_attempt": Value::Null,
                        "repair_attempt_note": "ripr://repair-attempt/{attempt_id} binds once ripr_prepare_repair creates the session transaction for this item (#3090)",
                    }),
                );
                object
            }),
            "claim_boundary": "Static exposure evidence for one canonical item. This document does not edit source, execute tests or mutation, or authorize a repair transaction.",
            "limitations": [
                "evidence is static analysis only; no runtime, mutation, or test-execution result is claimed",
                "missing or limited fields remain typed limitations and never become repair-ready",
            ],
        })
    }
}

/// Producer-owned evidence fields, projected once per finding. Fields the
/// producer does not populate stay typed `null`/`absent` states in the
/// serialized finding values rather than being back-filled here.
fn gap_evidence_core(
    finding: &Finding,
    canonical_id: &str,
    root: &Path,
    readiness: &RepairReadiness,
) -> Result<Value, String> {
    let activation = serde_json::to_value(&finding.activation)
        .map_err(|error| format!("serialize activation evidence: {error}"))?;
    let related_tests = finding
        .related_tests
        .iter()
        .map(|test| {
            json!({
                "name": test.name,
                "file": crate::output::path::repository_relative_path_text(root, &test.file),
                "line": test.line,
                "oracle": test.oracle,
                "oracle_kind": test.oracle_kind.as_str(),
                "oracle_strength": test.oracle_strength.as_str(),
                "relation_reason": test.relation_reason.map(|reason| reason.as_str()),
                "relation_confidence": test.relation_confidence.map(|confidence| confidence.as_str()),
                "miss": test.miss.map(|miss| miss.as_str()),
                "why": crate::output::related_test_miss::related_test_miss_reason(
                    test,
                    &finding.activation.missing_discriminators,
                ),
            })
        })
        .collect::<Vec<_>>();
    let source_currentness = serde_json::to_value(finding.source_currentness)
        .map_err(|error| format!("serialize source currentness: {error}"))?;
    let static_limitation = finding
        .static_limit_kind
        .as_ref()
        .map(serde_json::to_value)
        .transpose()
        .map_err(|error| format!("serialize static limitation: {error}"))?;
    let canonical_gap = finding
        .canonical_gap
        .as_ref()
        .map(|gap| {
            json!({
                "id": gap.id,
                "language": gap.language,
                "owner": gap.owner,
                "behavior_kind": gap.behavior_kind,
                "probe_kind": gap.probe_kind,
                "normalized_discriminator": gap.normalized_discriminator,
            })
        })
        .unwrap_or(Value::Null);

    Ok(json!({
        "canonical_id": canonical_id,
        "finding_id": finding.id,
        "class": finding.class.as_str(),
        "language": finding.language.as_ref().map(|language| language.as_str()),
        "location": {
            "file": crate::analysis::finding_location_text(root, &finding.probe.location.file),
            "line": finding.probe.location.line,
            "column": finding.probe.location.column,
        },
        "changed_behavior": {
            "expression": finding.probe.expression,
            "before": finding.probe.before,
            "after": finding.probe.after,
            "delta_kind": finding.probe.delta.as_str(),
            "probe_family": finding.probe.family.as_str(),
            "expected_sinks": finding.probe.expected_sinks,
        },
        "causal_attribution": canonical_gap,
        "discriminator_availability": activation,
        "related_tests_total": finding.related_tests_total(),
        "related_tests": related_tests,
        "evidence": finding.evidence,
        "missing": finding.missing,
        "static_limitation": static_limitation,
        "source_currentness": source_currentness,
        "recommended_next_step": finding.recommended_next_step,
        "readiness": {
            "repair_packet_ready": readiness.ready,
            "reason": readiness.detail(),
            "prepared_by": "#3090",
            "evaluation_basis": "producer facts on the committed finding: candidate actionability (#3281), the canonical gap's normalized discriminator (a producer-named missing discriminator is accepted only on a weakly exposed finding, where it is the repair target), and the strongest directly-related related-test grip; conjoined with the shared seam-level repair validator behind `ripr repair`, which must admit the item's seam (#8717)",
        },
        "repair_boundary": {
            "allowed_edit_surface": "none_declared",
            "must_not_change": "none_declared",
            "prepared_by": "#3090",
        },
    }))
}

/// Budget items for the shared [`crate::lsp::diagnostic_budget`] authority.
/// Eligibility is the producer's candidate-actionability predicate captured
/// at projection time; MCP does not invent its own filter or ranking. The
/// selection key uses the same unset (128) ranks the LSP delivery bridge
/// uses, so the budget's evidence-owned order (selection key, then canonical
/// id, then document) is the only ordering applied.
pub(crate) fn budget_items(
    items: &[GapItem],
) -> Vec<crate::lsp::diagnostic_budget::DiagnosticBudgetItem> {
    use crate::lsp::diagnostic_budget::{
        DiagnosticBudgetEligibility, DiagnosticBudgetItem, DiagnosticSelectionKey,
    };
    items
        .iter()
        .map(|item| DiagnosticBudgetItem {
            canonical_id: item.canonical_id.clone(),
            document: item.file.clone(),
            payload_bytes: item.list_summary_bytes(),
            inline_detail_bytes: item
                .evidence_bytes
                .saturating_sub(item.list_summary_bytes()),
            eligibility: if item.candidate_actionable {
                DiagnosticBudgetEligibility::Actionable
            } else {
                DiagnosticBudgetEligibility::ProfileFiltered
            },
            selection_key: DiagnosticSelectionKey {
                repair_route_rank: 128,
                causal_rank: 128,
                evidence_rank: 128,
            },
        })
        .collect()
}

/// Extract the canonical item id from a `ripr://gap/{id}` resource URI.
///
/// The producer gap id embeds a workspace-relative path (Python, and since
/// #5268 Rust), so the suffix is read whole: the `ripr://gap/` prefix alone
/// routes the URI, and only an empty suffix fails. Returns `None` for any
/// other URI, including the exact status resource.
pub(crate) fn gap_resource_id(uri: &str) -> Option<&str> {
    uri.strip_prefix("ripr://gap/").filter(|id| !id.is_empty())
}

/// Extract the snapshot id from a `ripr://snapshot/{id}` resource URI.
pub(crate) fn snapshot_resource_id(uri: &str) -> Option<&str> {
    uri.strip_prefix("ripr://snapshot/")
        .filter(|id| !id.is_empty() && !id.contains('/'))
}

/// Percent-decode a captured resource-id segment (#8736). Canonical ids
/// embed workspace-relative file paths raw — spaces, `#`, and non-ASCII
/// characters are literal — while URI-emitting clients must percent-encode
/// those characters to form a valid URI. Each `%XX` with two hex digits
/// (either case) decodes to its byte; a stray `%` or a short tail stays
/// literal, so a capture that is already raw decodes to itself. Returns
/// `None` when nothing decoded or the decode is not UTF-8: the resource
/// route then keeps the raw capture and its ordinary miss mapping.
pub(crate) fn percent_decode_resource_id(segment: &str) -> Option<String> {
    if !segment.contains('%') {
        return None;
    }
    let bytes = segment.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    let mut changed = false;
    while index < bytes.len() {
        let rest = &bytes[index..];
        if rest[0] == b'%'
            && rest.len() >= 3
            && let (Some(high), Some(low)) = (hex_value(rest[1]), hex_value(rest[2]))
        {
            decoded.push(high * 16 + low);
            index += 3;
            changed = true;
        } else {
            decoded.push(rest[0]);
            index += 1;
        }
    }
    if !changed {
        return None;
    }
    String::from_utf8(decoded)
        .ok()
        .filter(|decoded| decoded != segment)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn serialized_bytes(value: &Value) -> Result<usize, String> {
    struct LengthWriter(usize);

    impl std::io::Write for LengthWriter {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            self.0 += buffer.len();
            Ok(buffer.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let mut writer = LengthWriter(0);
    serde_json::to_writer(&mut writer, value)
        .map_err(|error| format!("serialize gap item: {error}"))?;
    Ok(writer.0)
}

/// Count and hash the same encoded evidence bytes without retaining them or
/// serializing twice. Snapshot identity binds the digest, so an evidence change
/// still produces a new identity even when its encoded length is unchanged.
fn serialized_evidence_identity(value: &impl serde::Serialize) -> Result<(usize, String), String> {
    struct IdentityWriter {
        bytes: usize,
        digest: Sha256,
    }

    impl std::io::Write for IdentityWriter {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            self.bytes += buffer.len();
            self.digest.update(buffer);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let mut writer = IdentityWriter {
        bytes: 0,
        digest: Sha256::new(),
    };
    // Preserve the first serialization failure's context from the previous
    // length pass. The same Value and infallible writer supplied both passes.
    serde_json::to_writer(&mut writer, value)
        .map_err(|error| format!("serialize gap item: {error}"))?;
    Ok((writer.bytes, format!("{:x}", writer.digest.finalize())))
}

/// One fully-established candidate finding shared by the `gaps` and `repair`
/// test suites: candidate-current source, a normalized discriminator, and one
/// strong, high-confidence directly-related test on a test surface.
#[cfg(test)]
pub(crate) fn test_finding() -> Result<Finding, String> {
    use crate::domain::{
        ActivationEvidence, Confidence, DeltaKind, Probe, ProbeFamily, ProbeId, RelatedTest,
        RiprEvidence, SourceLocation, StageEvidence, StageState,
    };
    let stage = || StageEvidence::new(StageState::Unknown, Confidence::Unknown, "test");
    Ok(Finding {
        id: "finding:test:1".to_string(),
        canonical_gap: Some(crate::domain::FindingCanonicalGap {
            id: "gap:test:1".to_string(),
            language: "rust".to_string(),
            file: "src/lib.rs".to_string(),
            owner: "checkout".to_string(),
            behavior_kind: "predicate".to_string(),
            probe_kind: "predicate".to_string(),
            normalized_discriminator: "total == 10000".to_string(),
        }),
        probe: Probe {
            id: ProbeId("probe:test:1".to_string()),
            location: SourceLocation::new("src/lib.rs", 12, 5),
            owner: Some(crate::domain::SymbolId("symbol:checkout".to_string())),
            family: ProbeFamily::Predicate,
            delta: DeltaKind::Value,
            before: Some("total > 10000".to_string()),
            after: Some("total >= 10000".to_string()),
            expression: "total >= 10000".to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        },
        class: crate::domain::ExposureClass::WeaklyExposed,
        ripr: RiprEvidence {
            reach: stage(),
            infect: stage(),
            propagate: stage(),
            reveal: crate::domain::RevealEvidence {
                observe: stage(),
                discriminate: stage(),
            },
        },
        confidence: 0.5,
        evidence: vec!["the changed comparison reaches the checkout return".to_string()],
        missing: vec!["No strong discriminator was detected".to_string()],
        flow_sinks: Vec::new(),
        activation: ActivationEvidence::default(),
        stop_reasons: Vec::new(),
        related_tests_matched_total: Some(1),
        related_tests: vec![RelatedTest {
            name: "checkout_totals".to_string(),
            file: std::path::PathBuf::from("tests/checkout.rs"),
            line: 40,
            oracle: Some("assert_eq!(total, 10001)".to_string()),
            oracle_kind: crate::domain::OracleKind::ExactValue,
            oracle_strength: crate::domain::OracleStrength::Strong,
            relation_reason: Some(crate::domain::RelationReason::DirectOwnerCall),
            relation_confidence: Some(crate::domain::RelationConfidence::High),
            miss: None,
        }],
        recommended_next_step: Some("add a boundary assertion for 10000".to_string()),
        language: Some(crate::domain::LanguageId::Rust),
        language_status: None,
        owner_kind: None,
        static_limit_kind: None,
        changed_sink: Some("total".to_string()),
        observed_sink: None,
        oracle_alignment: None,
        alignment_reason: None,
        source_currentness: crate::domain::SourceCurrentness::CandidateCurrent,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn finding() -> Result<Finding, String> {
        test_finding()
    }

    #[test]
    fn related_test_totals_bind_equal_windows_to_distinct_evidence() -> Result<(), String> {
        let mut source = finding()?;
        let row = source.related_tests[0].clone();
        source.related_tests = (0..8)
            .map(|index| {
                let mut row = row.clone();
                row.name = format!("checkout_{index}");
                row.line += index;
                row
            })
            .collect();
        source.related_tests_matched_total = Some(12);
        let twelve = GapItem::from_finding(&source, Path::new("."))?;
        source.related_tests_matched_total = Some(13);
        let thirteen = GapItem::from_finding(&source, Path::new("."))?;
        assert_eq!(twelve.evidence_core["related_tests_total"], 12);
        assert_eq!(thirteen.evidence_core["related_tests_total"], 13);
        assert_eq!(
            twelve.evidence_core["related_tests"],
            thirteen.evidence_core["related_tests"]
        );
        assert_eq!(
            twelve.evidence_core["related_tests"]
                .as_array()
                .map(Vec::len),
            Some(8)
        );
        let mut only_total_changed = twelve.evidence_core.clone();
        only_total_changed["related_tests_total"] = json!(13);
        assert_eq!(only_total_changed, thirteen.evidence_core);
        assert_eq!(twelve.evidence_bytes, thirteen.evidence_bytes);
        assert_ne!(twelve.evidence_sha256, thirteen.evidence_sha256);
        assert_eq!(twelve.repair_readiness, thirteen.repair_readiness);
        assert_eq!(twelve.list_summary, thirteen.list_summary);

        for (total, retained) in [(1, 1), (0, 0)] {
            source.related_tests.truncate(retained);
            source.related_tests_matched_total = Some(total);
            let item = GapItem::from_finding(&source, Path::new("."))?;
            assert_eq!(item.evidence_core["related_tests_total"], json!(total));
            assert_eq!(
                item.evidence_core["related_tests"].as_array().map(Vec::len),
                Some(retained)
            );
        }
        Ok(())
    }

    #[test]
    fn legacy_serialized_findings_project_retained_totals_without_digest_drift()
    -> Result<(), String> {
        let mut source = finding()?;
        let row = source.related_tests[0].clone();
        source.related_tests = (0..3)
            .map(|index| {
                let mut row = row.clone();
                row.name = format!("checkout_{index}");
                row.line += index;
                row
            })
            .collect();
        source.related_tests_matched_total = Some(3);
        let explicit = GapItem::from_finding(&source, Path::new("."))?;
        let serialized = serde_json::to_value(&source).map_err(|error| error.to_string())?;
        for metadata in [None, Some(Value::Null), Some(json!(0))] {
            let mut legacy = serialized.clone();
            let object = legacy
                .as_object_mut()
                .ok_or("Finding must serialize as an object")?;
            object.remove("related_tests_matched_total");
            if let Some(metadata) = metadata {
                object.insert("related_tests_matched_total".to_string(), metadata);
            }
            let decoded: Finding =
                serde_json::from_value(legacy).map_err(|error| error.to_string())?;
            assert_eq!(decoded.related_tests.len(), 3);
            let projected = GapItem::from_finding(&decoded, Path::new("."))?;
            assert_eq!(projected.evidence_core["related_tests_total"], 3);
            assert_eq!(projected.evidence_core, explicit.evidence_core);
            assert_eq!(projected.evidence_bytes, explicit.evidence_bytes);
            assert_eq!(projected.evidence_sha256, explicit.evidence_sha256);
            assert_eq!(projected.repair_readiness, explicit.repair_readiness);
        }
        Ok(())
    }

    /// #5996: one finding, one location string on every surface. The MCP
    /// item file must join byte-for-byte with the check JSON `probe.file`,
    /// the context packet `probe.file` and the human finding header for the
    /// same finding — including when the analyzed root is a verbatim Windows
    /// path (the MCP server's root spelling), which previously leaked the
    /// raw `//?/` producer join no other surface produced and no editor
    /// resolves.
    ///
    /// #8743 scope note: the packet here is rendered with the default
    /// configuration, and this test claims parity only for fields that are
    /// configuration-independent — the location strings above. `severity`
    /// is config-resolved on the packet since 1.1, so the production MCP
    /// route (which renders with the session's repo config) may legitimately
    /// carry a different `severity` value under a user override; full-packet
    /// parity is only claimed when the effective configurations match.
    #[test]
    fn one_finding_renders_one_location_string_on_every_surface_under_default_config()
    -> Result<(), String> {
        // check-local-context forbids drive-letter path literals; assemble
        // the verbatim root from parts (the `//?/` text lives in the owner's
        // own unit tests).
        let drive = "F:";
        let backslash = '\\';
        let root = PathBuf::from(format!(
            "{backslash}{backslash}?{backslash}{drive}{backslash}repo"
        ));
        let mut finding = finding()?;
        finding.probe.location =
            crate::domain::SourceLocation::new(root.join("src").join("main.rs"), 4, 1);
        let expected = "./src/main.rs";

        let item = GapItem::from_finding(&finding, &root)?;
        if item.file != expected {
            return Err(format!(
                "MCP item file must be workspace-relative: {}",
                item.file
            ));
        }
        let location_file = item
            .evidence_core
            .get("location")
            .and_then(|location| location.get("file"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                format!(
                    "gap evidence must carry a location file: {}",
                    item.evidence_core
                )
            })?;
        if location_file != expected {
            return Err(format!(
                "MCP gap location file must be workspace-relative: {location_file}"
            ));
        }

        let output = crate::app::CheckOutput {
            schema_version: crate::app::CHECK_OUTPUT_SCHEMA_VERSION.to_string(),
            harness_projections: Vec::new(),
            tool: "ripr".to_string(),
            mode: crate::app::Mode::Draft,
            root: root.clone(),
            base: None,
            analysis_outcome: None,
            summary: crate::domain::Summary::default(),
            findings: vec![finding.clone()],
            preview_language_advisories: Vec::new(),
            language_runs: Vec::new(),
            no_scope_provided: false,
            unanalyzed_working_tree: false,
            untracked_working_tree_source_paths: Vec::new(),
            ignored_working_tree_source_paths: Vec::new(),
            unlinked_python_tests: None,
            suppression: None,
            partial_scope: None,
            analyzed_revisions: None,
        };
        let check_json: Value = serde_json::from_str(&crate::output::json::render(&output))
            .map_err(|error| format!("parse check JSON: {error}"))?;
        if check_json
            .pointer("/findings/0/probe/file")
            .and_then(Value::as_str)
            != Some(expected)
        {
            return Err(format!("check JSON probe.file must match: {check_json}"));
        }

        let packet: Value = serde_json::from_str(&crate::output::json::render_context_packet(
            &finding,
            5,
            &crate::config::RiprConfig::default(),
            &root,
        ))
        .map_err(|error| format!("parse context packet: {error}"))?;
        if packet.pointer("/probe/file").and_then(Value::as_str) != Some(expected) {
            return Err(format!("context packet probe.file must match: {packet}"));
        }

        let header = crate::output::human::render_finding_with_config(
            &finding,
            &crate::config::RiprConfig::default(),
            &root,
            crate::output::human::JsonPointerWording::CheckFlag,
        );
        if !header
            .lines()
            .next()
            .is_some_and(|line| line.contains(expected))
        {
            return Err(format!(
                "human header must carry the same location: {header}"
            ));
        }
        Ok(())
    }

    #[test]
    fn evidence_identity_serializes_once_and_matches_encoded_bytes() -> Result<(), String> {
        struct Observed<'a> {
            value: &'a Value,
            calls: std::cell::Cell<usize>,
        }

        impl serde::Serialize for Observed<'_> {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                self.calls.set(self.calls.get() + 1);
                serde::Serialize::serialize(self.value, serializer)
            }
        }

        let item = GapItem::from_finding(&finding()?, Path::new("."))?;
        let values = [
            Value::Null,
            json!({}),
            json!([]),
            json!({
                "escaped": "quote: \" backslash: \\ newline: \n",
                "unicode": "é e\u{301} 😀",
                "nested": [true, false, 42, -17, 0.125, null, {"empty": ""}],
                "large": "x".repeat(65_536),
            }),
            item.evidence_core,
        ];
        for value in values {
            let bytes = serde_json::to_vec(&value).map_err(|error| error.to_string())?;
            let observed = Observed {
                value: &value,
                calls: std::cell::Cell::new(0),
            };
            let (length, digest) = serialized_evidence_identity(&observed)?;
            assert_eq!(observed.calls.get(), 1, "evidence must serialize only once");
            assert_eq!(length, bytes.len());
            assert_eq!(digest, format!("{:x}", Sha256::digest(&bytes)));
        }
        Ok(())
    }

    #[test]
    fn evidence_identity_preserves_the_first_serialization_error_context() {
        struct Refused;

        impl serde::Serialize for Refused {
            fn serialize<S: serde::Serializer>(&self, _serializer: S) -> Result<S::Ok, S::Error> {
                Err(serde::ser::Error::custom(
                    "injected evidence encoding failure",
                ))
            }
        }

        assert_eq!(
            serialized_evidence_identity(&Refused),
            Err("serialize gap item: injected evidence encoding failure".to_string())
        );
    }

    #[test]
    fn gap_projection_preserves_evidence_length_and_digest() -> Result<(), String> {
        let mut finding = finding()?;
        finding.recommended_next_step = Some("assert \"é😀\"\nwith a \\ escape".to_string());
        let item = GapItem::from_finding(&finding, Path::new("."))?;
        let bytes = serde_json::to_vec(&item.evidence_core).map_err(|error| error.to_string())?;
        assert_eq!(item.evidence_bytes, bytes.len());
        assert_eq!(item.evidence_sha256, format!("{:x}", Sha256::digest(bytes)));
        Ok(())
    }

    #[test]
    fn canonical_id_prefers_the_producer_gap_identity() -> Result<(), String> {
        let item = GapItem::from_finding(&finding()?, Path::new("."))?;
        if item.canonical_id != "gap:test:1" || item.finding_id != "finding:test:1" {
            return Err(format!(
                "canonical identity must prefer the producer gap id: {item:?}",
                item = (item.canonical_id.clone(), item.finding_id.clone())
            ));
        }
        if item.list_summary.pointer("/class").and_then(Value::as_str) != Some("weakly_exposed") {
            return Err(format!(
                "unexpected class wire token: {}",
                item.list_summary
            ));
        }
        if item.file != "src/lib.rs"
            || item.list_summary.pointer("/line").and_then(Value::as_u64) != Some(12)
        {
            return Err(format!(
                "location projection drifted: {}:{}",
                item.file, item.list_summary
            ));
        }
        Ok(())
    }

    #[test]
    fn fallback_identity_uses_the_finding_id() -> Result<(), String> {
        let mut finding = finding()?;
        finding.canonical_gap = None;
        let item = GapItem::from_finding(&finding, Path::new("."))?;
        if item.canonical_id != "finding:test:1" {
            return Err(format!(
                "without a canonical gap the finding id must be the item identity: {}",
                item.canonical_id
            ));
        }
        Ok(())
    }

    #[test]
    fn eligibility_follows_the_producer_actionability_predicate() -> Result<(), String> {
        use crate::lsp::diagnostic_budget::DiagnosticBudgetEligibility;
        let actionable = budget_items(&[GapItem::from_finding(&finding()?, Path::new("."))?]);
        let Some(actionable_item) = actionable.first() else {
            return Err("budget items must not be empty".to_string());
        };
        if actionable_item.eligibility != DiagnosticBudgetEligibility::Actionable {
            return Err("candidate-current findings must stay actionable".to_string());
        }
        let mut base_deleted = finding()?;
        base_deleted.source_currentness = crate::domain::SourceCurrentness::BaseDeleted;
        let filtered = budget_items(&[GapItem::from_finding(&base_deleted, Path::new("."))?]);
        let Some(filtered_item) = filtered.first() else {
            return Err("budget items must not be empty".to_string());
        };
        if filtered_item.eligibility != DiagnosticBudgetEligibility::ProfileFiltered {
            return Err(
                "non-candidate-current findings must map to the producer's filtered state"
                    .to_string(),
            );
        }
        Ok(())
    }

    #[test]
    fn evidence_document_reports_producer_repair_readiness() -> Result<(), String> {
        let item = GapItem::from_finding(&finding()?, Path::new("."))?;
        let document = item.document("snapshot:sha256:abc");
        let readiness = document
            .pointer("/item/readiness/repair_packet_ready")
            .ok_or_else(|| "gap document lost its readiness block".to_string())?;
        if readiness != &serde_json::json!(true) {
            return Err(format!(
                "the fully established fixture finding must project repair readiness: {document}"
            ));
        }
        let reason = document
            .pointer("/item/readiness/reason")
            .and_then(Value::as_str)
            .ok_or_else(|| "readiness needs its evaluation reason".to_string())?;
        if reason.is_empty() {
            return Err("readiness reason must not be empty".to_string());
        }
        if document
            .pointer("/item/readiness/prepared_by")
            .and_then(Value::as_str)
            != Some("#3090")
        {
            return Err("readiness lost the repair-slice owner".to_string());
        }
        if document.pointer("/item/repair_boundary/allowed_edit_surface")
            != Some(&serde_json::json!("none_declared"))
        {
            return Err("gap evidence must not invent an allowed edit surface".to_string());
        }
        if document.pointer("/item/links/repair_attempt") != Some(&Value::Null) {
            return Err("the repair-attempt link must stay an explicit null".to_string());
        }
        let snapshot_link = document
            .pointer("/item/links/snapshot")
            .and_then(Value::as_str)
            .ok_or_else(|| "gap document lost its snapshot link".to_string())?;
        if snapshot_link != "ripr://snapshot/snapshot:sha256:abc" {
            return Err(format!("snapshot link drifted: {snapshot_link}"));
        }
        Ok(())
    }

    #[test]
    fn readiness_stays_fail_closed_on_missing_producer_facts() -> Result<(), String> {
        let mut no_discriminator = finding()?;
        no_discriminator.canonical_gap = None;
        let item = GapItem::from_finding(&no_discriminator, Path::new("."))?;
        if item.repair_readiness.ready
            || item.repair_readiness.ineligibility
                != Some("discriminator_not_populated_for_language")
        {
            return Err(format!(
                "a finding without a canonical gap must refuse repair as an unpopulated language producer fact: {:?}",
                item.repair_readiness.ineligibility
            ));
        }

        let mut named_missing = finding()?;
        named_missing.activation.missing_discriminators.push(
            crate::domain::MissingDiscriminatorFact {
                value: "total == 10000".to_string(),
                reason: "boundary not asserted".to_string(),
                flow_sink: None,
            },
        );
        // #8717: on a weakly exposed finding the named value is the repair
        // target, so the discriminator gate passes and the finding is ready.
        let item = GapItem::from_finding(&named_missing, Path::new("."))?;
        if !item.repair_readiness.ready || item.repair_readiness.ineligibility.is_some() {
            return Err(format!(
                "a weak finding with a named missing discriminator is the repair target: {:?}",
                item.repair_readiness.ineligibility
            ));
        }
        // Discriminating negative: the same named value on any non-weak class
        // keeps the typed refusal.
        for class in [
            crate::domain::ExposureClass::ReachableUnrevealed,
            crate::domain::ExposureClass::NoStaticPath,
            crate::domain::ExposureClass::InfectionUnknown,
            crate::domain::ExposureClass::PropagationUnknown,
            crate::domain::ExposureClass::StaticUnknown,
            crate::domain::ExposureClass::Exposed,
        ] {
            let mut other = named_missing.clone();
            other.class = class.clone();
            let item = GapItem::from_finding(&other, Path::new("."))?;
            if item.repair_readiness.ready
                || item.repair_readiness.ineligibility != Some("missing_discriminator")
            {
                return Err(format!(
                    "a named missing discriminator on {class:?} must stay the typed refusal: {:?}",
                    item.repair_readiness.ineligibility
                ));
            }
        }
        // A weak finding whose canonical gap is absent stays refused even
        // with a named missing discriminator.
        let mut ungapped = named_missing.clone();
        ungapped.canonical_gap = None;
        let item = GapItem::from_finding(&ungapped, Path::new("."))?;
        if item.repair_readiness.ready
            || item.repair_readiness.ineligibility
                != Some("discriminator_not_populated_for_language")
        {
            return Err(format!(
                "a named missing discriminator cannot substitute for a populated gap: {:?}",
                item.repair_readiness.ineligibility
            ));
        }
        // The fix-site gate still applies to the weak repair target.
        let mut no_site = named_missing.clone();
        no_site.related_tests.clear();
        let item = GapItem::from_finding(&no_site, Path::new("."))?;
        if item.repair_readiness.ready
            || item.repair_readiness.ineligibility != Some("fix_site_not_established")
        {
            return Err(format!(
                "a weak finding without a fix site must stay refused: {:?}",
                item.repair_readiness.ineligibility
            ));
        }

        let mut base_deleted = finding()?;
        base_deleted.source_currentness = crate::domain::SourceCurrentness::BaseDeleted;
        let item = GapItem::from_finding(&base_deleted, Path::new("."))?;
        if item.repair_readiness.ready
            || item.repair_readiness.ineligibility != Some("not_candidate_actionable")
        {
            return Err(format!(
                "base-side evidence must not be repair-ready: {:?}",
                item.repair_readiness.ineligibility
            ));
        }

        let mut weak_grip = finding()?;
        for test in &mut weak_grip.related_tests {
            test.oracle_strength = crate::domain::OracleStrength::Weak;
        }
        let item = GapItem::from_finding(&weak_grip, Path::new("."))?;
        if item.repair_readiness.ready
            || item.repair_readiness.ineligibility != Some("fix_site_not_established")
        {
            return Err(format!(
                "a finding without a strong directly-related test must not be repair-ready: {:?}",
                item.repair_readiness.ineligibility
            ));
        }
        Ok(())
    }

    fn ready_readiness() -> RepairReadiness {
        RepairReadiness {
            ready: true,
            fix_site: Some(RepairFixSite {
                test_name: "a".to_string(),
                file: "tests/t.rs".to_string(),
                line: 2,
                oracle: Some("assert_eq!(f(), 1);".to_string()),
                oracle_kind: "exact_value",
            }),
            ineligibility: None,
            seam_detail: None,
        }
    }

    /// #8717: the shared seam validator's refusal is a conjunct of
    /// readiness. A finding-ready item it refuses is not ready, drops its fix
    /// site, and keeps the validator's own typed code and explanation; an
    /// admitted or unevaluated verdict changes nothing, and a finding-level
    /// refusal stays the first failing gate.
    #[test]
    fn seam_verdict_is_a_conjunct_of_repair_readiness() -> Result<(), String> {
        let refused = SeamVerdict::Refused {
            code: crate::mcp::repair_admission::SEAM_ALREADY_GRIPPED,
            detail: "the shared repair gate refuses: already grip".to_string(),
        };
        let blocked = ready_readiness().with_seam_verdict(Some(&refused));
        if blocked.ready
            || blocked.fix_site.is_some()
            || blocked.ineligibility != Some("seam_already_gripped")
            || !blocked.detail().contains("already grip")
        {
            return Err(format!("refused verdict must block readiness: {blocked:?}"));
        }
        for verdict in [Some(&SeamVerdict::Admitted), None] {
            let kept = ready_readiness().with_seam_verdict(verdict);
            if !kept.ready || kept.ineligibility.is_some() || kept.fix_site.is_none() {
                return Err(format!("admitted/unevaluated must keep ready: {kept:?}"));
            }
        }
        let finding_level = RepairReadiness {
            ready: false,
            fix_site: None,
            ineligibility: Some("missing_discriminator"),
            seam_detail: None,
        }
        .with_seam_verdict(Some(&refused));
        if finding_level.ineligibility != Some("missing_discriminator")
            || finding_level.seam_detail.is_some()
        {
            return Err(format!(
                "the finding-level refusal must stay first: {finding_level:?}"
            ));
        }
        Ok(())
    }

    /// The evaluated-admission default is fail-closed: a finding the shared
    /// validator did not cover is refused, never admitted.
    #[test]
    fn evaluated_admission_refuses_an_uncovered_finding() -> Result<(), String> {
        let evaluated = SeamAdmission::evaluate(
            &[],
            &[],
            crate::app::agent_brief::AgentBriefPolicy::from_config(
                &crate::config::RiprConfig::default(),
            ),
            &crate::mcp::repair_admission::InventoryCompleteness::Complete,
        );
        match evaluated.verdict("probe:uncovered") {
            Some(SeamVerdict::Refused { code, .. })
                if code == crate::mcp::repair_admission::SEAM_NOT_FOUND =>
            {
                Ok(())
            }
            other => Err(format!("uncovered finding must be refused: {other:?}")),
        }
    }

    /// #9108: the CLI errors before resolving any item when the repo-wide
    /// inventory is truncated, so MCP must refuse every finding with the
    /// typed truncation reason. Without the fix the empty inventory below
    /// yields `seam_not_found` for the truncated case too.
    #[test]
    fn truncated_inventory_refuses_before_any_resolution() -> Result<(), String> {
        use crate::mcp::repair_admission::{InventoryCompleteness, SEAM_INVENTORY_TRUNCATED};
        let config = crate::config::RiprConfig::default();
        let policy = crate::app::agent_brief::AgentBriefPolicy::from_config(&config);
        let findings = [finding()?];
        let id = findings[0].id.as_str();
        let complete =
            SeamAdmission::evaluate(&findings, &[], policy, &InventoryCompleteness::Complete);
        match complete.verdict(id) {
            Some(SeamVerdict::Refused { code, .. }) if code != SEAM_INVENTORY_TRUNCATED => {}
            other => return Err(format!("complete inventory must not truncate: {other:?}")),
        }
        for completeness in [
            InventoryCompleteness::Truncated {
                analyzed: 3,
                total: 7,
            },
            InventoryCompleteness::Unverified("inventory failed".to_string()),
        ] {
            let evaluated = SeamAdmission::evaluate(&findings, &[], policy, &completeness);
            match evaluated.verdict(id) {
                Some(SeamVerdict::Refused { code, detail }) if code == SEAM_INVENTORY_TRUNCATED => {
                    if matches!(completeness, InventoryCompleteness::Truncated { .. })
                        && !(detail.contains("3 of 7")
                            && detail.contains("RIPR_REPO_EXPOSURE_SEAM_LIMIT"))
                    {
                        return Err(format!("detail must name the counts and remedy: {detail}"));
                    }
                }
                other => return Err(format!("{completeness:?} must refuse: {other:?}")),
            }
        }
        // The refusal blocks readiness with its typed reason and a specific
        // explanation, never the generic fallback.
        let refused = SeamVerdict::Refused {
            code: SEAM_INVENTORY_TRUNCATED,
            detail: "truncated".to_string(),
        };
        let blocked = ready_readiness().with_seam_verdict(Some(&refused));
        if blocked.ready
            || blocked.ineligibility != Some(SEAM_INVENTORY_TRUNCATED)
            || blocked.reason().contains("did not establish every")
        {
            return Err(format!("truncation must block readiness: {blocked:?}"));
        }
        // Precedence: a finding failing a finding-level gate still gets the
        // inventory-level refusal, but keeps its own gate when the verdict is
        // an ordinary seam refusal.
        let unready = RepairReadiness {
            ready: false,
            fix_site: None,
            ineligibility: Some("discriminator_not_populated_for_language"),
            seam_detail: None,
        };
        let truncated = unready.clone().with_seam_verdict(Some(&refused));
        if truncated.ineligibility != Some(SEAM_INVENTORY_TRUNCATED)
            || truncated.seam_detail.is_none()
        {
            return Err(format!("truncation must outrank the gate: {truncated:?}"));
        }
        let ordinary = SeamVerdict::Refused {
            code: "seam_gate_refused",
            detail: "gate".to_string(),
        };
        if unready.clone().with_seam_verdict(Some(&ordinary)) != unready {
            return Err("an ordinary seam refusal must not outrank a finding gate".to_string());
        }
        Ok(())
    }

    /// #9108: the completeness fact is read exactly like the CLI reads it:
    /// an error or `analyzed < total` refuses; an exact fit or no limit
    /// does not.
    #[test]
    fn completeness_mirrors_the_cli_truncation_check() {
        use crate::analysis::{SeamLimitInfo, SeamLimitSource};
        use crate::mcp::repair_admission::InventoryCompleteness as C;
        let info = |analyzed, total| {
            Ok(Some(SeamLimitInfo {
                analyzed,
                total,
                source: SeamLimitSource::Default,
            }))
        };
        assert_eq!(
            C::from_inventory_limit(info(3, 7)),
            C::Truncated {
                analyzed: 3,
                total: 7
            }
        );
        assert_eq!(C::from_inventory_limit(info(7, 7)), C::Complete);
        assert_eq!(C::from_inventory_limit(Ok(None)), C::Complete);
        // Parity with the shared predicate the CLI gates read: for every
        // analyzed/total pair the MCP verdict is truncated iff the CLI's is.
        for analyzed in 0..4usize {
            for total in 0..4usize {
                let limit = SeamLimitInfo {
                    analyzed,
                    total,
                    source: SeamLimitSource::Default,
                };
                assert_eq!(
                    matches!(
                        C::from_inventory_limit(Ok(Some(limit.clone()))),
                        C::Truncated { .. }
                    ),
                    analyzed < total,
                    "analyzed {analyzed} total {total}"
                );
                assert_eq!(limit.is_truncated(), analyzed < total);
            }
        }
        assert_eq!(
            C::from_inventory_limit(Err("boom".to_string())),
            C::Unverified("boom".to_string())
        );
    }

    /// #5210: the non-surface refusal names the inline-test boundary, so
    /// an MCP caller learns the permanent scope from the refusal itself.
    #[test]
    fn non_surface_reason_names_the_inline_test_boundary() -> Result<(), String> {
        let readiness = RepairReadiness {
            ready: false,
            fix_site: None,
            ineligibility: Some("fix_site_not_test_surface"),
            seam_detail: None,
        };
        let reason = readiness.reason();
        for needle in [
            "only test-surface paths can be the authored edit target",
            "inline",
            "#[cfg(test)]",
            "don't qualify their file",
        ] {
            if !reason.contains(needle) {
                return Err(format!(
                    "non-surface reason must name the inline boundary ({needle}): {reason}"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn unpopulated_language_discriminator_reason_agrees_with_its_own_availability_block()
    -> Result<(), String> {
        // #5268: when the producer named no missing discriminator but never
        // populated a normalized discriminator either (every Rust finding
        // today), one served document must not contradict itself: the
        // readiness refusal names the unpopulated producer condition instead
        // of claiming the producer failed to establish a discriminator.
        let mut unpopulated = finding()?;
        unpopulated.canonical_gap = None;
        unpopulated.missing = Vec::new();
        unpopulated.class = crate::domain::ExposureClass::Exposed;
        let item = GapItem::from_finding(&unpopulated, Path::new("."))?;
        let document = item.document("snapshot:sha256:abc");
        let reason = document
            .pointer("/item/readiness/reason")
            .and_then(Value::as_str)
            .ok_or_else(|| "readiness lost its reason".to_string())?;
        if document
            .pointer("/item/readiness/repair_packet_ready")
            .and_then(Value::as_bool)
            != Some(false)
        {
            return Err(format!(
                "an unpopulated discriminator must stay fail-closed: {document}"
            ));
        }
        let availability = document
            .pointer("/item/discriminator_availability/missing_discriminators")
            .and_then(Value::as_array)
            .ok_or_else(|| "availability block lost its missing discriminators".to_string())?;
        if !availability.is_empty() {
            return Err(format!(
                "the mismatch control needs an availability block with nothing missing: {availability:?}"
            ));
        }
        if !reason.contains("normalized discriminator") || !reason.contains("not populated") {
            return Err(format!(
                "the readiness reason must name the unpopulated producer condition, not a contradiction: {reason}"
            ));
        }
        if reason.contains("did not establish a discriminator") {
            return Err(format!(
                "the readiness reason must not claim the producer failed to establish a discriminator while the same document shows nothing missing: {reason}"
            ));
        }
        Ok(())
    }

    #[test]
    fn static_limited_python_finding_refuses_as_static_limitation_not_language_gap()
    -> Result<(), String> {
        // A Python producer withholds the canonical gap exactly when the
        // finding carries its own typed static limitation
        // (`analysis/language/python/classify.rs` gates the gap on
        // `static_limit.is_none()`), so such a finding must refuse as
        // `static_limitation` — the limitation, not language-wide
        // non-population, explains the missing fact.
        let mut limited = finding()?;
        limited.canonical_gap = None;
        limited.missing = Vec::new();
        limited.language = Some(crate::domain::LanguageId::Python);
        limited.static_limit_kind = Some(crate::domain::StaticLimitKind::UnsupportedSyntax);
        let item = GapItem::from_finding(&limited, Path::new("."))?;
        if item.repair_readiness.ready
            || item.repair_readiness.ineligibility != Some("static_limitation")
        {
            return Err(format!(
                "a static-limited finding must refuse as static_limitation, not as a language-wide gap: {:?}",
                item.repair_readiness.ineligibility
            ));
        }
        let document = item.document("snapshot:sha256:abc");
        let reason = document
            .pointer("/item/readiness/reason")
            .and_then(Value::as_str)
            .ok_or_else(|| "readiness lost its reason".to_string())?;
        if !reason.contains("static limitation") || reason.contains("not populated") {
            return Err(format!(
                "the static-limited refusal must name the finding's limitation, not the language: {reason}"
            ));
        }
        Ok(())
    }

    /// #5268 residual: once the Rust producer populates canonical gaps, a
    /// producer-shaped Rust finding (the pricing boundary seam) must obey the
    /// same gate ordering as every other language — a producer-named missing
    /// discriminator keeps the typed refusal ahead of the populated gap, and
    /// clearing it (the boundary got pinned by a test) lets the same finding
    /// through the discriminator gate.
    #[test]
    fn populated_rust_gap_opens_the_discriminator_gate_for_weak_named_or_nothing_missing()
    -> Result<(), String> {
        let mut boundary = finding()?;
        boundary.language = Some(crate::domain::LanguageId::Rust);
        boundary.canonical_gap = Some(crate::domain::FindingCanonicalGap {
            id: "gap:rust:src/lib.rs:discount:predicate_boundary:predicate:total==100".to_string(),
            language: "rust".to_string(),
            file: "src/lib.rs".to_string(),
            owner: "discount".to_string(),
            behavior_kind: "predicate_boundary".to_string(),
            probe_kind: "predicate".to_string(),
            normalized_discriminator: "total==100".to_string(),
        });
        boundary
            .activation
            .missing_discriminators
            .push(crate::domain::MissingDiscriminatorFact {
                value: "total == 100".to_string(),
                reason: "no test call pins the equality boundary".to_string(),
                flow_sink: None,
            });
        // Weak + named missing discriminator + populated gap: the repair
        // target, ready at the finding level (#8717).
        let item = GapItem::from_finding(&boundary, Path::new("."))?;
        if !item.repair_readiness.ready {
            return Err(format!(
                "a weak Rust finding naming its missing discriminator with the gap populated is the repair target: {:?}",
                item.repair_readiness.ineligibility
            ));
        }
        // The same named value on a non-weak class keeps the typed refusal
        // ahead of the populated gap.
        let mut unrevealed = boundary.clone();
        unrevealed.class = crate::domain::ExposureClass::ReachableUnrevealed;
        let item = GapItem::from_finding(&unrevealed, Path::new("."))?;
        if item.repair_readiness.ready
            || item.repair_readiness.ineligibility != Some("missing_discriminator")
        {
            return Err(format!(
                "a named missing discriminator on a non-weak class must keep the typed refusal even with the gap populated: {:?}",
                item.repair_readiness.ineligibility
            ));
        }
        // The boundary got pinned: the producer names nothing missing, the
        // finding is exposed, and the populated gap opens the gate — the
        // finding then fails or passes on the remaining (fix-site) gates
        // exactly like the fixture's strong test surface allows.
        boundary.activation.missing_discriminators.clear();
        boundary.missing.clear();
        boundary.class = crate::domain::ExposureClass::Exposed;
        let item = GapItem::from_finding(&boundary, Path::new("."))?;
        if !item.repair_readiness.ready {
            return Err(format!(
                "an exposed Rust finding with a populated gap and nothing missing must pass the discriminator gate: {:?}",
                item.repair_readiness.ineligibility
            ));
        }
        Ok(())
    }

    #[test]
    fn complete_and_incomplete_evidence_stay_distinct() -> Result<(), String> {
        let mut with_gap = finding()?;
        with_gap.missing = Vec::new();
        let complete = GapItem::from_finding(&with_gap, Path::new("."))?;
        let incomplete = GapItem::from_finding(&finding()?, Path::new("."))?;
        if complete.evidence_bytes == incomplete.evidence_bytes {
            return Err(
                "missing-evidence and complete-evidence items must not project identically"
                    .to_string(),
            );
        }
        Ok(())
    }

    #[test]
    fn resource_uri_parsing_is_strict() {
        assert_eq!(gap_resource_id("ripr://gap/gap:test:1"), Some("gap:test:1"));
        assert_eq!(
            snapshot_resource_id("ripr://snapshot/snapshot:sha256:x"),
            Some("snapshot:sha256:x")
        );
        for other in [
            "ripr://workspace/status",
            "ripr://gap/",
            "ripr://snapshot/",
            "https://example.com/gap/x",
        ] {
            assert_eq!(gap_resource_id(other), None, "{other}");
        }
        assert_eq!(snapshot_resource_id("ripr://gap/x"), None);
    }

    /// #5268 review: the producer gap id embeds a workspace-relative path,
    /// so the advertised `ripr://gap/{id}` resource must parse the suffix
    /// whole — a nested-file Rust (or Python) finding's resource link has to
    /// resolve to the same id its tool calls address. The snapshot grammar
    /// stays single-segment.
    #[test]
    fn gap_resource_route_reads_a_nested_producer_gap_id_whole() {
        let nested = "gap:rust:src/lib.rs:discount:predicate_boundary:predicate:amount==fee";
        assert_eq!(
            gap_resource_id(&format!("ripr://gap/{nested}")),
            Some(nested)
        );
        assert_eq!(
            crate::mcp::repair_card::repair_card_resource_id(&format!(
                "ripr://repair-card/{nested}"
            )),
            Some(nested)
        );
        // A snapshot id never carries a path, so its grammar stays strict.
        assert_eq!(snapshot_resource_id("ripr://snapshot/a/b"), None);
    }

    /// #8736: canonical ids embed raw workspace-relative paths (spaces,
    /// `#`, and non-ASCII are literal), so URI-emitting clients must
    /// percent-encode those characters. The decoder inverts valid `%XX`
    /// escapes (either hex case), keeps stray `%` and short tails literal,
    /// and reports no change when nothing decoded — the resource route
    /// retries the decoded segment only when it differs from the capture.
    #[test]
    fn percent_decoding_resource_id_segments() {
        assert_eq!(
            percent_decode_resource_id("gap%3Arust%3Asrc%2Flib.rs"),
            Some("gap:rust:src/lib.rs".to_string())
        );
        assert_eq!(
            percent_decode_resource_id("gap:rust:my%20file%23name.rs"),
            Some("gap:rust:my file#name.rs".to_string())
        );
        assert_eq!(
            percent_decode_resource_id("%E2%98%83"),
            Some("☃".to_string())
        );
        assert_eq!(
            percent_decode_resource_id("gap%3arust"),
            Some("gap:rust".to_string())
        );
        // A raw stable-path id (`%25` is a literal percent) must decode to
        // the raw spelling, and the server falls back to the raw capture
        // when the decoded form misses.
        assert_eq!(
            percent_decode_resource_id("pricing_%25FF.rs"),
            Some("pricing_%FF.rs".to_string())
        );
        // Nothing to decode: no `%`, no valid escape, or no change.
        assert_eq!(percent_decode_resource_id("gap:rust:src/lib.rs"), None);
        assert_eq!(percent_decode_resource_id("gap:rust:100%.rs"), None);
        assert_eq!(percent_decode_resource_id("gap:rust:trailing%"), None);
        // Invalid UTF-8 after decoding refuses the retry instead of
        // manufacturing an unmatchable id.
        assert_eq!(percent_decode_resource_id("%FF%FE"), None);
    }

    /// #5510: the MCP gap evidence carries the packet-backed Perl rows with
    /// the shared sentence on the direct row only.
    #[cfg(feature = "lang-perl")]
    #[test]
    fn perl_packet_backed_rows_agree_in_gap_evidence() -> Result<(), String> {
        use crate::output::related_test_miss::related_test_miss_reason;
        let finding = crate::analysis::perl_direct_and_advisory_finding()?;
        let [direct, advisory] = finding.related_tests.as_slice() else {
            return Err(format!("expected two rows: {:?}", finding.related_tests));
        };
        let why = related_test_miss_reason(direct, &finding.activation.missing_discriminators)
            .ok_or("the direct row should have a reason")?;
        let item = GapItem::from_finding(&finding, Path::new("/test-root"))?;
        let rows = item.evidence_core["related_tests"]
            .as_array()
            .ok_or("expected related_tests in gap evidence")?;
        let names = rows
            .iter()
            .map(|row| row["name"].as_str().unwrap_or_default())
            .collect::<Vec<_>>();
        assert_eq!(names, [direct.name.as_str(), advisory.name.as_str()]);
        assert_eq!(rows[0]["miss"], "observation_unconfirmed");
        assert_eq!(rows[0]["why"], why.as_str());
        assert!(rows[1]["miss"].is_null() && rows[1]["why"].is_null());
        Ok(())
    }

    #[test]
    fn gap_projection_renders_every_served_file_root_relative() -> Result<(), String> {
        // #5254 item 6: the observed wire leak was a canonicalized finding
        // file served verbatim. Every served file renders relative to the
        // analyzed root: the item, the list summary, and the evidence
        // location through the shared finding-location owner (#5996, with
        // its `./` prefix), related tests and the fix site through the
        // repository-relative renderer. Host-native absolute roots (no
        // filesystem touch) so the test pins the threading on every host;
        // the verbatim/case spelling matrix lives portably with the
        // renderer in `output::path`.
        let root = std::env::temp_dir().join("ripr-gap-path-root");
        let mut finding = finding()?;
        finding.probe.location.file = root.join("src/lib.rs");
        finding.related_tests[0].file = root.join("tests/checkout.rs");

        let item = GapItem::from_finding(&finding, &root)?;
        assert_eq!(item.file, "./src/lib.rs");
        assert_eq!(
            item.list_summary.pointer("/file").and_then(Value::as_str),
            Some("./src/lib.rs")
        );
        assert_eq!(
            item.evidence_core
                .pointer("/location/file")
                .and_then(Value::as_str),
            Some("./src/lib.rs")
        );
        assert_eq!(
            item.evidence_core
                .pointer("/related_tests/0/file")
                .and_then(Value::as_str),
            Some("tests/checkout.rs")
        );
        let fix_site = item
            .repair_readiness
            .fix_site
            .as_ref()
            .ok_or("the strong related test should establish a fix site")?;
        assert_eq!(fix_site.file, "tests/checkout.rs");
        // Belt and braces: the root's unique marker appears nowhere in the
        // served documents.
        for document in [&item.list_summary, &item.evidence_core] {
            let text = serde_json::to_string(document).map_err(|error| error.to_string())?;
            assert!(
                !text.contains("ripr-gap-path-root"),
                "served documents must not leak host paths: {text}"
            );
        }
        Ok(())
    }

    #[test]
    fn gap_projection_keeps_an_out_of_root_file_stable() -> Result<(), String> {
        // A file the root does not contain cannot be expressed relatively;
        // it keeps its full stable spelling rather than an invented location.
        let root = std::env::temp_dir().join("ripr-gap-path-root");
        let elsewhere = std::env::temp_dir().join("ripr-gap-path-elsewhere");
        let mut finding = finding()?;
        finding.probe.location.file = elsewhere.join("lib.rs");

        let item = GapItem::from_finding(&finding, &root)?;
        assert!(
            item.file.contains("ripr-gap-path-elsewhere") && item.file.ends_with("lib.rs"),
            "an out-of-root file must keep its full stable spelling, got: {}",
            item.file
        );
        Ok(())
    }
}

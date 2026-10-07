//! MCP workspace session: the bounded analysis lifecycle adapter.
//!
//! The session is the MCP-side owner of "which snapshot is current". It does
//! not analyze by itself: [`run_check`] delegates to the shared
//! [`crate::app::check_workspace`] authority, and bounded selection delegates
//! to the shared [`crate::lsp::diagnostic_budget`] evaluator. The session only
//! records attempt state, keeps the last completed (last-known-good) snapshot,
//! and projects bounded, progressively disclosed documents for the tools and
//! resources.
//!
//! Read-only boundary (ADR 0022): analysis here is the same static analysis
//! the CLI and LSP run in-process. The session never edits source, never
//! executes verification or mutation commands, never loads project-local
//! provider configuration, and never reports a repair-ready state.

use super::gaps::{self, GAP_LIST_SCHEMA_VERSION, GapItem};
use crate::analysis_outcome::{AnalysisOutcome, AnalysisOutcomeKind};
use crate::lsp::diagnostic_budget::{
    self, DiagnosticBudget, DiagnosticBudgetResult, DiagnosticOverflowReason,
    OmittedDiagnosticReason,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

pub(crate) const SESSION_SCHEMA_VERSION: &str = "ripr-mcp-session-v1";
pub(crate) const REFRESH_SCHEMA_VERSION: &str = "ripr-mcp-refresh-v1";
pub(crate) const SNAPSHOT_SCHEMA_VERSION: &str = "ripr-mcp-snapshot-v1";

pub(crate) const CODE_WORKSPACE_UNAVAILABLE: &str = "workspace_unavailable";
pub(crate) const CODE_ANALYSIS_FAILED: &str = "analysis_failed";
pub(crate) const CODE_UNSUPPORTED_PROFILE: &str = "unsupported_profile";
/// The workspace `ripr.toml` is present but cannot be read or parsed
/// (#6825): the refresh attempt fails closed instead of silently analyzing
/// with built-in defaults. Promoted from the reserved vocabulary when the
/// config path landed.
pub(crate) const CODE_CONFIG_INVALID: &str = "config_invalid";
pub(crate) const CODE_NO_SNAPSHOT: &str = "no_snapshot";
pub(crate) const CODE_ANALYSIS_IN_FLIGHT: &str = "analysis_in_flight";
pub(crate) const CODE_STALE_SNAPSHOT: &str = "stale_snapshot";
pub(crate) const CODE_ITEM_NOT_FOUND: &str = "item_not_found";
pub(crate) const CODE_RESULT_TOO_LARGE: &str = "result_too_large";
/// Reserved typed-failure vocabulary for later slices. This slice fails
/// closed before any of these states can occur; they are named so the wire
/// contract stays stable when the owning slice lands.
pub(crate) const RESERVED_FAILURE_CODES: &[&str] = &[
    "workspace_ambiguous",
    "static_limitation",
    "cancelled",
    "superseded",
];

const MAX_FAILURE_DETAIL_CHARS: usize = 512;

/// The caller-narrowed window over a gap list's selected items (#6021):
/// `offset` indexes the selected items in snapshot order, `limit` caps the
/// returned page. The default window byte-fills one wire-fitting page.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct GapListWindow {
    pub(crate) offset: usize,
    pub(crate) limit: Option<usize>,
}

/// The disclosed page window inside a gap-list document: what the caller
/// asked for (`offset`, `limit` when set), what shipped, and how to
/// continue. A page never lies about the selection: `selected` stays the
/// full stored-selection count while `page.returned` counts this page.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct GapPage {
    offset: usize,
    limit: Option<usize>,
    returned: usize,
    has_more: bool,
    next_offset: Option<usize>,
}

/// Compact serialized length of a document, measured without retaining the
/// encoded bytes.
fn document_bytes(value: &Value) -> Result<usize, AttemptFailure> {
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
    serde_json::to_writer(&mut writer, value).map_err(|error| {
        AttemptFailure::new(
            CODE_ANALYSIS_FAILED,
            format!("serialize gap list: {error}"),
            "retry with ripr_refresh",
        )
    })?;
    Ok(writer.0)
}

/// One typed failure. `detail` is producer wording bounded to
/// [`MAX_FAILURE_DETAIL_CHARS`]; `recovery` names the action that can change
/// the state; `data` carries small structured context (for example the
/// current snapshot identity on a stale request).
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AttemptFailure {
    pub(crate) code: &'static str,
    pub(crate) detail: String,
    pub(crate) recovery: &'static str,
    pub(crate) data: Value,
}

impl AttemptFailure {
    pub(crate) fn new(
        code: &'static str,
        detail: impl Into<String>,
        recovery: &'static str,
    ) -> Self {
        Self {
            code,
            detail: bounded_detail(detail),
            recovery,
            data: json!({}),
        }
    }

    pub(crate) fn with_data(mut self, data: Value) -> Self {
        self.data = data;
        self
    }

    /// The structured failure block shared by tool results and documents.
    pub(crate) fn value(&self) -> Value {
        json!({
            "code": self.code,
            "detail": self.detail,
            "recovery": self.recovery,
            "data": self.data,
        })
    }

    pub(crate) fn document(&self, schema_version: &str) -> Value {
        json!({
            "schema_version": schema_version,
            "failure": self.value(),
        })
    }
}

fn bounded_detail(detail: impl Into<String>) -> String {
    let detail = detail.into();
    let mut chars = detail.chars();
    let bounded: String = chars.by_ref().take(MAX_FAILURE_DETAIL_CHARS).collect();
    if chars.next().is_some() {
        return format!("{bounded}…");
    }
    bounded
}

/// The completed-snapshot authority for one MCP session. Immutable once
/// committed: refresh replaces the whole snapshot, never mutates it.
pub(crate) struct Snapshot {
    pub(crate) snapshot_id: String,
    pub(crate) outcome: AnalysisOutcome,
    pub(crate) items: Vec<GapItem>,
    /// The committed findings the evidence projections read (`ripr_get_gap`
    /// and the repair-card witness matcher both bind from this exact set).
    pub(crate) findings: Vec<crate::domain::Finding>,
    /// The repair-card producer facts bound when this snapshot committed
    /// (RIPR-SPEC-0215). Snapshots committed before the card projection
    /// existed carry `None`; their card reads fail closed with
    /// `no_snapshot` instead of silently missing the card surface.
    pub(crate) card_producers: Option<super::repair_card::SnapshotCardProducers>,
    pub(crate) budget: DiagnosticBudget,
    pub(crate) selection: DiagnosticBudgetResult,
    /// The producer's RIPR-SPEC-0112 working-tree facts bound at commit time
    /// (#5995): the analyzed committed default-branch diff ran while routed
    /// source or test files carried uncommitted edits, so those edits are
    /// outside this snapshot's scope. `uncommitted_edits` includes untracked
    /// files (the overlay keeps them in its dirty set); `untracked` names the
    /// subset neither the committed diff nor `--worktree` can analyze
    /// (#5258), so the served disclosure can name the real remedy. The facts
    /// ride the snapshot identity: two otherwise identical snapshots with
    /// different exclusions are different snapshots.
    pub(crate) scope: ScopeFacts,
}

/// The scope-relevant working-tree facts one snapshot was analyzed against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScopeFacts {
    pub(crate) uncommitted_edits: bool,
    pub(crate) untracked: Vec<String>,
}

impl ScopeFacts {
    fn from_output(output: &crate::app::CheckOutput) -> Self {
        Self {
            uncommitted_edits: output.unanalyzed_working_tree,
            untracked: output.untracked_working_tree_source_paths.clone(),
        }
    }

    fn identity_key(&self) -> Value {
        json!({
            "uncommitted_edits": self.uncommitted_edits,
            "untracked": self.untracked,
        })
    }
}

impl Snapshot {
    /// Bind one completed analysis output into a snapshot. Closed incomplete
    /// outcomes (`unsupported_input`, `analysis_failed`) stay failures and
    /// never become snapshots; partial outcomes commit with their typed
    /// limitations so complete-zero and incomplete-zero stay distinct.
    pub(crate) fn from_output(
        output: &crate::app::CheckOutput,
        root_identity: Option<&str>,
    ) -> Result<Self, AttemptFailure> {
        let outcome = output
            .analysis_outcome
            .as_ref()
            .ok_or_else(|| {
                AttemptFailure::new(
                    CODE_ANALYSIS_FAILED,
                    "the producer returned no typed analysis outcome",
                    "retry with ripr_refresh; if the failure persists, run `ripr check --format json` in the repository",
                )
            })?
            .clone();
        match outcome.kind {
            AnalysisOutcomeKind::UnsupportedInput => {
                return Err(AttemptFailure::new(
                    CODE_UNSUPPORTED_PROFILE,
                    format!(
                        "the workspace input is unsupported: {}",
                        limitation_summary(&outcome)
                    ),
                    "narrow the diff (for example `ripr check --base <revision> --format json`) and retry ripr_refresh",
                ));
            }
            AnalysisOutcomeKind::AnalysisFailed => {
                return Err(AttemptFailure::new(
                    CODE_ANALYSIS_FAILED,
                    format!("the analysis failed: {}", limitation_summary(&outcome)),
                    "retry with ripr_refresh; if the failure persists, run `ripr check --format json` in the repository",
                ));
            }
            AnalysisOutcomeKind::NoScope
            | AnalysisOutcomeKind::NoChangedLines
            | AnalysisOutcomeKind::NoBehavioralCandidates
            | AnalysisOutcomeKind::CompleteNoFindings
            | AnalysisOutcomeKind::CompleteWithFindings
            | AnalysisOutcomeKind::PartialWithLimitations => {}
        }

        let mut items = output
            .findings
            .iter()
            .map(|finding| GapItem::from_finding(finding, &output.root))
            .collect::<Result<Vec<_>, String>>()
            .map_err(|error| {
                AttemptFailure::new(
                    CODE_ANALYSIS_FAILED,
                    format!("project canonical items: {error}"),
                    "retry with ripr_refresh",
                )
            })?;
        items.sort_by(|left, right| left.canonical_id.cmp(&right.canonical_id));

        let scope = ScopeFacts::from_output(output);
        let snapshot_id = snapshot_identity(&outcome, &items, &scope).map_err(|error| {
            AttemptFailure::new(CODE_ANALYSIS_FAILED, error, "retry with ripr_refresh")
        })?;
        let profile_identity = format!(
            "mcp:{}:profile:diff:draft",
            root_identity.unwrap_or("root:unavailable")
        );
        let evidence_identity = evidence_identity(&items).map_err(|error| {
            AttemptFailure::new(CODE_ANALYSIS_FAILED, error, "retry with ripr_refresh")
        })?;
        let budget = DiagnosticBudget::default();
        let selection = diagnostic_budget::evaluate_diagnostic_budget(
            gaps::budget_items(&items),
            &budget,
            &profile_identity,
            &evidence_identity,
        )
        .map_err(|error| {
            AttemptFailure::new(
                CODE_ANALYSIS_FAILED,
                format!("bounded selection failed: {error}"),
                "retry with ripr_refresh",
            )
        })?;

        Ok(Self {
            snapshot_id,
            outcome,
            items,
            findings: output.findings.clone(),
            card_producers: None,
            budget,
            selection,
            scope,
        })
    }

    pub(crate) fn item(&self, id: &str) -> Option<&GapItem> {
        self.items
            .iter()
            .find(|item| item.canonical_id == id || item.finding_id == id)
    }
}

fn limitation_summary(outcome: &AnalysisOutcome) -> String {
    outcome
        .limitations
        .iter()
        .map(|limitation| limitation.kind.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn snapshot_identity(
    outcome: &AnalysisOutcome,
    items: &[GapItem],
    scope: &ScopeFacts,
) -> Result<String, String> {
    let outcome_digest = outcome.semantic_digest()?;
    let item_ids = items
        .iter()
        .map(|item| item.canonical_id.as_str())
        .collect::<Vec<_>>();
    let evidence_digests = items
        .iter()
        .map(|item| item.evidence_sha256.as_str())
        .collect::<Vec<_>>();
    let payload = json!({
        "outcome_digest": outcome_digest,
        "items": item_ids,
        "evidence": evidence_digests,
        // The scope facts change what the served documents disclose, so two
        // otherwise identical snapshots with different exclusions must not
        // share an identity (#5995 review).
        "scope": scope.identity_key(),
    });
    let bytes = serde_json::to_vec(&payload)
        .map_err(|error| format!("serialize snapshot identity: {error}"))?;
    Ok(format!("snapshot:sha256:{}", sha256_hex(&bytes)))
}

fn evidence_identity(items: &[GapItem]) -> Result<String, String> {
    let item_ids = items
        .iter()
        .map(|item| item.canonical_id.as_str())
        .collect::<Vec<_>>();
    let payload = json!({ "items": item_ids });
    let bytes = serde_json::to_vec(&payload)
        .map_err(|error| format!("serialize evidence identity: {error}"))?;
    Ok(format!("evidence:sha256:{}", sha256_hex(&bytes)))
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Freshness is relative to the last completed snapshot. A later failed
/// attempt leaves the retained snapshot unverified for that attempt: it is
/// never labeled current at a refresh that produced no snapshot.
fn freshness_state(session: &WorkspaceSession) -> &'static str {
    match (&session.last_good, &session.last_failure) {
        (Some(_), Some(_)) => "stale_after_failed_attempt",
        (Some(_), None) => "current_at_last_refresh",
        (None, _) => "none",
    }
}

/// Mutable session state. `in_flight` marks a running attempt; `last_good`
/// is the last-known-good completed snapshot; `last_failure` is the most
/// recent terminal failure. A failure never replaces `last_good`.
#[derive(Default)]
pub(crate) struct WorkspaceSession {
    pub(crate) in_flight: bool,
    pub(crate) last_good: Option<Arc<Snapshot>>,
    pub(crate) last_failure: Option<AttemptFailure>,
    /// Session repair transactions created by `ripr_prepare_repair`, keyed by
    /// deterministic attempt identity. In-memory like the snapshot: a restart
    /// drops it, and every transaction binds the snapshot identity it was
    /// prepared against.
    pub(crate) repairs: std::collections::BTreeMap<String, super::repair::RepairTransaction>,
    /// Tombstones for session transactions evicted when a newer snapshot
    /// supersedes them (`attempt_id` → the snapshot identity it was bound
    /// to). Keeps the typed `superseded` failure reachable after the full
    /// packet is pruned, without holding superseded packets in memory for
    /// the lifetime of the session.
    pub(crate) superseded_attempts: std::collections::BTreeMap<String, String>,
    /// Insertion order of `superseded_attempts` keys, oldest first. Attempt
    /// ids are unordered digest hex, so the map alone cannot say which
    /// tombstone is oldest; the queue makes cap eviction drop the oldest
    /// tombstone first and never a recent one (#6291).
    pub(crate) superseded_order: std::collections::VecDeque<String>,
}

impl WorkspaceSession {
    pub(crate) fn attempt_state(&self) -> &'static str {
        if self.in_flight {
            "in_flight"
        } else if self.last_failure.is_some() {
            "failed"
        } else if self.last_good.is_some() {
            "completed"
        } else {
            "no_snapshot"
        }
    }

    pub(crate) fn active_snapshot(
        &self,
        requested: Option<&str>,
    ) -> Result<&Arc<Snapshot>, AttemptFailure> {
        if self.in_flight {
            return Err(AttemptFailure::new(
                CODE_ANALYSIS_IN_FLIGHT,
                "an analysis attempt is running",
                "poll ripr_workspace_status until attempt_state leaves in_flight, then retry",
            ));
        }
        let Some(snapshot) = &self.last_good else {
            return Err(AttemptFailure::new(
                CODE_NO_SNAPSHOT,
                "no completed snapshot exists in this session",
                "call ripr_refresh to run one bounded analysis, then retry",
            ));
        };
        if let Some(requested) = requested
            && requested != snapshot.snapshot_id
        {
            return Err(AttemptFailure::new(
                CODE_STALE_SNAPSHOT,
                format!("snapshot {requested} is not the current completed snapshot"),
                "re-read ripr_workspace_status for the current snapshot identity, then retry",
            )
            .with_data(json!({ "current_snapshot_id": snapshot.snapshot_id })));
        }
        Ok(snapshot)
    }

    /// Bounded working set for the current (or named) snapshot. Selection is
    /// the snapshot's stored shared-budget result; this function never
    /// re-runs ranking. `window` pages over the selected items in snapshot
    /// order (#6021); an unset limit byte-fills the page to
    /// [`super::MAX_TOOL_DOCUMENT_BYTES`] so a listing that outgrows one
    /// wire response degrades to disclosed pages instead of failing after
    /// budget approval.
    pub(crate) fn list_gaps(
        &self,
        requested: Option<&str>,
        window: GapListWindow,
    ) -> Result<Value, AttemptFailure> {
        let snapshot = self.active_snapshot(requested)?;
        let selection = &snapshot.selection;
        let selected_ids = selection
            .selected_ids()
            .collect::<std::collections::BTreeSet<&str>>();
        let selected_items = snapshot
            .items
            .iter()
            .filter(|item| selected_ids.contains(item.canonical_id.as_str()))
            .collect::<Vec<_>>();

        // The document shell carries every disclosure except the page of
        // item summaries; its measured size sets the page budget. The
        // reserve absorbs the small byte difference between this shell's
        // zeroed page fields and the filled ones the caller receives.
        let shell_bytes = {
            let shell = self.gap_list_document(
                snapshot,
                selection,
                requested,
                Vec::new(),
                &GapPage::default(),
            )?;
            document_bytes(&shell)?
        };
        // Reserve covers the array syntax around the summaries and the
        // byte-accounting margin; per-item commas are charged below.
        let reserve = 256usize;
        let page_budget =
            super::MAX_TOOL_DOCUMENT_BYTES.saturating_sub(shell_bytes.saturating_add(reserve));

        let start = window.offset.min(selected_items.len());
        let mut items = Vec::new();
        let mut used = 0usize;
        for item in &selected_items[start..] {
            if items.len() >= window.limit.unwrap_or(usize::MAX) {
                break;
            }
            // One byte per joining comma keeps the accounting exact.
            let cost = item.list_summary_bytes().saturating_add(1);
            // The first summary always ships: a page must make progress
            // even when a single item outgrows the byte budget.
            if !items.is_empty() && used + cost > page_budget {
                break;
            }
            used += cost;
            items.push(item.list_summary.clone());
        }

        // The tool advertises an outputSchema, so a successful result must
        // carry its structured copy (#6021 review): shrink the byte-fitted
        // page until the complete double envelope measures under the bound.
        // The probe carries the exact final-page fields (has_more and
        // next_offset derived from the selection). The complete page is not
        // strictly larger than every shorter prefix — the selection-
        // exhausting page omits the next-page route — so probe it first and
        // keep it whole when it fits; only then bisect the strictly
        // monotone continuation prefixes for the largest fitting page.
        let page_fields = |count: usize| GapPage {
            // The window echoes the requested offset; a past-end request is
            // an empty disclosed page at that offset, not a renumbered one.
            offset: window.offset,
            limit: window.limit,
            returned: count,
            has_more: start + count < selected_items.len(),
            next_offset: (start + count < selected_items.len()).then_some(start + count),
        };
        if !items.is_empty() {
            let structured_fit = |count: usize| -> Result<bool, AttemptFailure> {
                let probe = self.gap_list_document(
                    snapshot,
                    selection,
                    requested,
                    items[..count].to_vec(),
                    &page_fields(count),
                )?;
                crate::mcp::protocol::structured_envelope_overflows(&probe)
                    .map(|overflows| !overflows)
                    .map_err(|error| {
                        AttemptFailure::new(CODE_ANALYSIS_FAILED, error, "retry with ripr_refresh")
                    })
            };
            if !structured_fit(items.len())? {
                let mut low = 1usize;
                let mut high = items.len() - 1;
                let mut fitting = 0usize;
                while low <= high {
                    let mid = (low + high) / 2;
                    if structured_fit(mid)? {
                        fitting = mid;
                        low = mid + 1;
                    } else {
                        high = mid.saturating_sub(1);
                    }
                }
                if fitting == 0 {
                    // Even one summary cannot fit beside the shell in the
                    // structured envelope: no page can carry the advertised
                    // structured result, so fail closed instead of shipping
                    // a text-only success that breaks the outputSchema
                    // contract (#6021 review).
                    return Err(AttemptFailure::new(
                        CODE_RESULT_TOO_LARGE,
                        format!(
                            "gap list cannot serve even one page with its structured result: the non-pageable disclosure alone fills the {}-byte envelope budget",
                            super::MAX_RESPONSE_BYTES
                        ),
                        "read identities through the ripr://snapshot/{snapshot_id} resource and single items through ripr_get_gap",
                    )
                    .with_data(json!({ "current_snapshot_id": snapshot.snapshot_id })));
                }
                items.truncate(fitting);
            }
        }

        let page = page_fields(items.len());

        let document = self.gap_list_document(snapshot, selection, requested, items, &page)?;
        // A listing whose non-pageable shell (for example an omission
        // disclosure alone) outgrows the ceiling cannot be narrowed by
        // paging; fail closed naming the identity route (#6021).
        let bytes = document_bytes(&document)?;
        if bytes > super::MAX_TOOL_DOCUMENT_BYTES {
            return Err(AttemptFailure::new(
                CODE_RESULT_TOO_LARGE,
                format!(
                    "gap list cannot fit even one wire-fitting page: its non-pageable disclosure renders {bytes} bytes against the {}-byte page ceiling",
                    super::MAX_TOOL_DOCUMENT_BYTES
                ),
                "read identities through the ripr://snapshot/{snapshot_id} resource and single items through ripr_get_gap",
            )
            .with_data(json!({ "current_snapshot_id": snapshot.snapshot_id })));
        }
        bounded_document(document)
    }

    /// Assemble the gap-list document around a page of item summaries.
    fn gap_list_document(
        &self,
        snapshot: &Snapshot,
        selection: &DiagnosticBudgetResult,
        requested: Option<&str>,
        items: Vec<Value>,
        page: &GapPage,
    ) -> Result<Value, AttemptFailure> {
        let mut continuation = json!({
            "tool": "ripr_get_gap",
            "resource_template": "ripr://gap/{canonical_id}",
        });
        if let Some(next_offset) = page.next_offset {
            // The generated route pins the snapshot identity: a refresh
            // between pages fails closed with stale_snapshot instead of
            // silently resuming at the old offset over a new selection
            // (#6021 review).
            continuation["next_page"] = json!({
                "tool": "ripr_list_gaps",
                "arguments": {
                    "snapshot_id": snapshot.snapshot_id,
                    "offset": next_offset,
                },
            });
        }
        Ok(json!({
            "schema_version": GAP_LIST_SCHEMA_VERSION,
            "snapshot_id": snapshot.snapshot_id,
            "requested_snapshot_id": requested,
            "snapshot_profile_budget_identity": selection.snapshot_profile_budget_identity,
            "selection_basis_version": selection.selection_basis_version,
            "budget": {
                "max_items_per_workspace_response": snapshot.budget.max_items_per_workspace_response,
                "max_items_per_document": snapshot.budget.max_items_per_document,
                "max_serialized_bytes": snapshot.budget.max_serialized_bytes,
                "max_inline_detail_bytes": snapshot.budget.max_inline_detail_bytes,
            },
            "total": selection.total_canonical_items,
            "eligible": selection.eligible_items,
            "selected": selection.selected.len(),
            "omitted": selection.omitted.len(),
            "selected_bytes": selection.selected_bytes,
            "complete_bytes": selection.complete_bytes,
            "overflowed": selection.overflowed,
            "overflow_reasons": selection
                .overflow_reasons
                .iter()
                .map(|reason| overflow_reason_as_str(*reason))
                .collect::<Vec<_>>(),
            "items": items,
            "page": {
                "offset": page.offset,
                "limit": page.limit,
                "returned": page.returned,
                "has_more": page.has_more,
                "next_offset": page.next_offset,
            },
            "omitted_items": selection
                .omitted
                .iter()
                .map(|item| json!({
                    "canonical_id": item.canonical_id,
                    "reason": omitted_reason_as_str(item.reason),
                }))
                .collect::<Vec<_>>(),
            "continuation": continuation,
            "claim_boundary": "Bounded working-set projection over one completed snapshot. Selection is the shared CLI/LSP budget authority; omitted identities and reasons are disclosed, never silently truncated, and no business-risk ranking is inferred.",
            "limitations": [
                "summaries do not contain evidence detail; read one item with ripr_get_gap or ripr://gap/{canonical_id}",
                "the list is deterministic for its snapshot identity; a refresh replaces the snapshot and its identities",
            ],
        }))
    }

    /// One canonical item's complete bounded evidence.
    pub(crate) fn get_gap(
        &self,
        canonical_id: &str,
        requested: Option<&str>,
    ) -> Result<Value, AttemptFailure> {
        let snapshot = self.active_snapshot(requested)?;
        let Some(item) = snapshot.item(canonical_id) else {
            return Err(AttemptFailure::new(
                CODE_ITEM_NOT_FOUND,
                format!("no canonical item {canonical_id} exists in the current snapshot"),
                "list the current canonical ids with ripr_list_gaps, then retry",
            ));
        };
        let mut document = item.document(&snapshot.snapshot_id);
        // A live session transaction binds the reserved repair-attempt link;
        // without one the link stays the explicit null the projection sets.
        if let Some(attempt_id) =
            self.live_repair_attempt(&snapshot.snapshot_id, &item.canonical_id)
            && let Some(links) = document
                .pointer_mut("/item/links")
                .and_then(Value::as_object_mut)
        {
            links.insert(
                "repair_attempt".to_string(),
                Value::from(format!("ripr://repair-attempt/{attempt_id}")),
            );
            links.remove("repair_attempt_note");
        }
        bounded_document(document)
    }

    /// The `ripr://snapshot/{snapshot_id}` evidence resource: snapshot
    /// identity, typed outcome, the full item index, and the stored selection
    /// summary — not the full evidence graph.
    pub(crate) fn snapshot_document(&self, requested: &str) -> Result<Value, AttemptFailure> {
        let snapshot = self.active_snapshot(Some(requested))?;
        let selection = &snapshot.selection;
        let outcome = serde_json::to_value(&snapshot.outcome).map_err(|error| {
            AttemptFailure::new(
                CODE_ANALYSIS_FAILED,
                format!("serialize analysis outcome: {error}"),
                "retry with ripr_refresh",
            )
        })?;
        let document = json!({
            "schema_version": SNAPSHOT_SCHEMA_VERSION,
            "snapshot_id": snapshot.snapshot_id,
            "outcome": outcome,
            "items": snapshot
                .items
                .iter()
                .map(|item| item.list_summary.clone())
                .collect::<Vec<_>>(),
            "selection": {
                "snapshot_profile_budget_identity": selection.snapshot_profile_budget_identity,
                "selection_basis_version": selection.selection_basis_version,
                "total": selection.total_canonical_items,
                "eligible": selection.eligible_items,
                "selected": selection.selected.len(),
                "omitted": selection.omitted.len(),
                "selected_bytes": selection.selected_bytes,
                "complete_bytes": selection.complete_bytes,
                "overflowed": selection.overflowed,
                "overflow_reasons": selection
                    .overflow_reasons
                    .iter()
                    .map(|reason| overflow_reason_as_str(*reason))
                    .collect::<Vec<_>>(),
            },
            "continuation": {
                "list_tool": "ripr_list_gaps",
                "item_resource_template": "ripr://gap/{canonical_id}",
            },
            "claim_boundary": "Bounded snapshot evidence for one completed analysis. The outcome is typed producer state; a later refresh supersedes this snapshot and its identity.",
            "limitations": [
                "item entries are identities and locations, not evidence; read one item through ripr_get_gap or ripr://gap/{canonical_id}",
            ],
        });
        bounded_document(document)
    }

    /// Session block for `ripr_workspace_status`. Facts only: no evidence
    /// detail, per progressive disclosure.
    pub(crate) fn session_document(&self, profile: &SessionProfile) -> Value {
        let last_completed = self.last_good.as_ref().map(|snapshot| {
            json!({
                "snapshot_id": snapshot.snapshot_id,
                "outcome_kind": snapshot.outcome.kind.as_str(),
                "finding_count": snapshot.outcome.counts.finding_count,
                "total_items": snapshot.selection.total_canonical_items,
            })
        });
        let analysis_outcome = self
            .last_good
            .as_ref()
            .and_then(|snapshot| serde_json::to_value(&snapshot.outcome).ok())
            .unwrap_or(Value::Null);
        let last_known_good = self
            .last_good
            .as_ref()
            .map(|snapshot| json!({ "snapshot_id": snapshot.snapshot_id }));
        let last_failure = self
            .last_failure
            .as_ref()
            .map(|failure| failure.value())
            .unwrap_or(Value::Null);
        json!({
            "schema_version": SESSION_SCHEMA_VERSION,
            "attempt_state": self.attempt_state(),
            "current_attempt": if self.in_flight {
                Value::from("in_flight")
            } else {
                Value::Null
            },
            "current_desired_input": {
                "kind": "workspace_diff",
                "base": "default_branch",
                "mode": "draft",
                "scope": "diff",
            },
            "last_completed_snapshot": last_completed,
            "last_known_good": last_known_good,
            "last_failure": last_failure,
            "scope": self
                .last_good
                .as_ref()
                .and_then(|snapshot| scope_disclosure(snapshot))
                .unwrap_or(Value::Null),
            "freshness": {
                "state": freshness_state(self),
                "note": "the server does not watch the worktree; the snapshot is current as of its last completed ripr_refresh, a later failed attempt leaves it unverified for that attempt, so refresh again after edits",
            },
            "analysis_outcome": analysis_outcome,
            "profile": profile.document(),
            "limitations": profile
                .session_limitation()
                .into_iter()
                .chain([
                    "the session is in-memory: restarting the server drops the snapshot unless a new ripr_refresh commits one",
                ])
                .collect::<Vec<_>>(),
        })
    }
}

/// Guard that clears [`WorkspaceSession::in_flight`] on every terminal path.
/// rmcp awaits a handler future to completion on request cancellation, but
/// transport teardown can still drop the task while the analysis worker
/// runs; without the guard a dropped handler would leave `in_flight` set
/// and fail every later refresh closed with `analysis_in_flight`.
pub(crate) struct InFlightAttempt {
    session: Arc<Mutex<WorkspaceSession>>,
    committed: bool,
}

impl InFlightAttempt {
    pub(crate) fn new(session: Arc<Mutex<WorkspaceSession>>) -> Self {
        Self {
            session,
            committed: false,
        }
    }

    /// The handler committed the attempt outcome itself; `Drop` becomes a
    /// no-op and the normal path owns the state transition.
    pub(crate) fn disarm(&mut self) {
        self.committed = true;
    }
}

impl Drop for InFlightAttempt {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        if let Ok(mut session) = self.session.try_lock() {
            session.in_flight = false;
            return;
        }
        // The lock is momentarily held by a concurrent projection; reset on
        // the runtime instead of leaving the session stuck in flight.
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let session = self.session.clone();
            let _reset = runtime.spawn(async move {
                let mut session = session.lock().await;
                session.in_flight = false;
            });
        }
    }
}

/// The typed scope disclosure for one committed snapshot (#5995), mirroring
/// the LSP session's limits-note wording family: the refresh analyzes only
/// the committed default-branch diff, so when routed source or test files
/// carried uncommitted edits at analysis time (RIPR-SPEC-0112's producer
/// fact), those edits are outside every served document — a dirty-tree
/// `no_scope` with zero findings must never read as all-clear. When
/// untracked files exist, the note never offers bare `--worktree` as the
/// remedy: they are invisible to both the committed diff and `--worktree`
/// (#5258), so the disclosure names staging or an explicit diff instead,
/// the same contract as the human note. `ripr_refresh` takes no diff-source
/// argument, so there is no in-protocol expansion to promise.
fn scope_disclosure(snapshot: &Snapshot) -> Option<Value> {
    let scope = &snapshot.scope;
    if !scope.uncommitted_edits {
        return None;
    }
    const NAMED_PATHS: usize = 3;
    let named = scope
        .untracked
        .iter()
        .take(NAMED_PATHS)
        .cloned()
        .collect::<Vec<_>>();
    let more = scope.untracked.len().saturating_sub(NAMED_PATHS);
    if scope.untracked.is_empty() {
        return Some(json!({
            "analyzed": "committed default-branch diff",
            "uncommitted_edits": "outside this analysis",
            "note": "staged and unstaged tracked edits are outside the analyzed scope of this snapshot; analyze them with `ripr check --worktree --format json` in the repository",
        }));
    }
    let listing = if more > 0 {
        format!("{} and {more} more", named.join(", "))
    } else {
        named.join(", ")
    };
    Some(json!({
        "analyzed": "committed default-branch diff",
        "uncommitted_edits": "outside this analysis",
        "note": "this snapshot reads each file as committed at HEAD; `ripr check --worktree --format json` adds staged and unstaged tracked edits only. Untracked files are invisible to both; stage them (`git add <paths>`, or `git add -N <paths>` intent-to-add makes a new file visible to `--worktree`) and rerun, or pass an explicit `--diff`",
        "untracked_edits": {
            "files": named,
            "total": scope.untracked.len(),
            "more": more,
            "listing_example": listing,
            "remedy": "stage them (`git add <paths>`, or `git add -N <paths>`) and rerun `ripr check --worktree`, or pass an explicit diff",
        },
    }))
}

/// The refresh-time attempt document returned by `ripr_refresh`.
pub(crate) fn refresh_document(session: &WorkspaceSession) -> Value {
    let snapshot = session.last_good.as_ref().map(|snapshot| {
        json!({
            "snapshot_id": snapshot.snapshot_id,
            "outcome_kind": snapshot.outcome.kind.as_str(),
            "finding_count": snapshot.outcome.counts.finding_count,
            "total_items": snapshot.selection.total_canonical_items,
            "list_route": "ripr_list_gaps",
            "snapshot_resource": format!("ripr://snapshot/{}", snapshot.snapshot_id),
        })
    });
    json!({
        "schema_version": REFRESH_SCHEMA_VERSION,
        "attempt": {
            "state": session.attempt_state(),
            "failure": session
                .last_failure
                .as_ref()
                .map(|failure| failure.value())
                .unwrap_or(Value::Null),
        },
        "snapshot": snapshot,
        "scope": session
            .last_good
            .as_ref()
            .and_then(|snapshot| scope_disclosure(snapshot))
            .unwrap_or(Value::Null),
        "last_known_good": session
            .last_good
            .as_ref()
            .map(|snapshot| json!({ "snapshot_id": snapshot.snapshot_id })),
        "claim_boundary": "One bounded static analysis attempt over the workspace diff. The server never edits source, executes verification or mutation commands, or loads project-local provider configuration.",
        "limitations": [
            "an attempt runs to a terminal state; MCP cancellation of the request does not roll back a running attempt and never manufactures a snapshot",
            "a cancelled refresh attempt still commits as a completed snapshot when it finishes; only transport teardown abandons an attempt before it commits (#5254 item 2)",
        ],
    })
}

/// The resolved analysis-configuration posture of the session (#6825): which
/// configuration `ripr_refresh` will run under. The read-only boundary
/// (ADR 0022) is untouched — the server still edits nothing and loads no
/// *provider* configuration; honoring the workspace's analysis
/// configuration is the same `load_for_root` resolution the CLI uses.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SessionConfigPosture {
    /// A workspace `ripr.toml` was read and parsed; `identity` is the
    /// fingerprint of its exact text.
    Loaded { identity: String },
    /// A `ripr.toml` entry is present but could not be read or parsed;
    /// refresh fails closed with `config_invalid`.
    DetectedNotLoaded,
    /// No config resolved; refresh runs on built-in defaults (marker-based
    /// language auto-enable is disclosed through `languages`).
    BuiltInDefaults,
}

/// The analysis profile facts for the session block, resolved from the
/// workspace's own configuration (#6825).
#[derive(Clone, Debug)]
pub(crate) struct SessionProfile {
    mode: &'static str,
    languages: Vec<String>,
    posture: SessionConfigPosture,
}

impl SessionProfile {
    /// Resolve the profile from the analyzed root. `None` (an unavailable
    /// root) keeps the built-in-default profile.
    pub(crate) fn resolve(root: Option<&Path>) -> Self {
        let Some(root) = root else {
            return Self::built_in();
        };
        match crate::config::load_for_root(root) {
            Ok(config) => {
                let languages = config
                    .languages()
                    .enabled()
                    .iter()
                    .map(|language| language.as_str().to_string())
                    .collect();
                let posture = match crate::config::loaded_config_identity(&config) {
                    Some(identity) => SessionConfigPosture::Loaded { identity },
                    None => SessionConfigPosture::BuiltInDefaults,
                };
                Self {
                    mode: "draft",
                    languages,
                    posture,
                }
            }
            // A load failure with no config entry anywhere (for example the
            // marker-based language auto-enable refusing an unavailable
            // language on a feature-restricted build) is a defaults
            // posture; only a present-but-unloadable entry is
            // detected-not-loaded (#6825).
            Err(_) if crate::config::config_discovered_for_root(root) => Self {
                posture: SessionConfigPosture::DetectedNotLoaded,
                ..Self::built_in()
            },
            Err(_) => Self::built_in(),
        }
    }

    /// The built-in-default profile (an unavailable root, or the defaults
    /// fallback when the workspace config cannot load).
    fn built_in() -> Self {
        let languages = crate::config::RiprConfig::default()
            .languages()
            .enabled()
            .iter()
            .map(|language| language.as_str().to_string())
            .collect();
        Self {
            mode: "draft",
            languages,
            posture: SessionConfigPosture::BuiltInDefaults,
        }
    }

    fn document(&self) -> Value {
        let (project_config, support) = match &self.posture {
            SessionConfigPosture::Loaded { .. } => ("loaded", "project_config"),
            SessionConfigPosture::DetectedNotLoaded => ("detected_not_loaded", "built_in_defaults"),
            SessionConfigPosture::BuiltInDefaults => ("built_in_defaults", "built_in_defaults"),
        };
        let mut document = json!({
            "mode": self.mode,
            "languages": self.languages,
            "project_config": project_config,
            "support": support,
        });
        if let SessionConfigPosture::Loaded { identity } = &self.posture {
            document["config_identity"] = Value::from(identity.clone());
        }
        document
    }

    /// The session-block limitation naming the configuration posture, so a
    /// zero finding count is never mistaken for a language gate (#6825).
    fn session_limitation(&self) -> Option<&'static str> {
        match self.posture {
            SessionConfigPosture::Loaded { .. } => Some(
                "this profile resolved ripr.toml at server startup; each ripr_refresh re-resolves it and binds the config identity it used in the snapshot outcome, so a post-startup ripr.toml edit is visible to the next refresh before it is visible here",
            ),
            SessionConfigPosture::DetectedNotLoaded => Some(
                "project-local ripr.toml is detected but could not be loaded; refresh fails closed with config_invalid until it parses",
            ),
            SessionConfigPosture::BuiltInDefaults => {
                Some("no workspace ripr.toml was loaded; refresh runs with built-in defaults")
            }
        }
    }
}

/// Run one bounded analysis through the shared check authority and bind it
/// into a snapshot. This is the only bridge from the session to the
/// producer: read-only static analysis, identical to what `ripr check` and
/// the LSP run in-process. The workspace's own configuration is honored
/// through the same `load_for_root` resolution the CLI uses (#6825): a
/// config file is loaded, and a config-less root keeps built-in defaults
/// (with the zero-config marker-based language auto-enable), so a
/// Python-enabled workspace analyzes identically over MCP and CLI.
pub(crate) fn run_check(
    root: &Path,
    root_identity: Option<&str>,
) -> Result<Snapshot, AttemptFailure> {
    let config = crate::config::load_for_root(root).map_err(|error| {
        AttemptFailure::new(
            CODE_CONFIG_INVALID,
            format!("the workspace configuration could not be loaded: {error}"),
            "address the configuration error above (the detail names the cause), then retry with ripr_refresh",
        )
    })?;
    let input = crate::app::CheckInput {
        root: PathBuf::from(root),
        git_timeout: Some(crate::app::default_cli_git_timeout()),
        ..Default::default()
    };
    let output = crate::app::check_workspace_with_config(input, &config).map_err(|error| {
        AttemptFailure::new(
            CODE_ANALYSIS_FAILED,
            format!("shared check authority failed: {error}"),
            "retry with ripr_refresh; if the failure persists, run `ripr check --format json` in the repository for the full diagnostic",
        )
    })?;
    let mut snapshot = Snapshot::from_output(&output, root_identity)?;
    // Bind the repair-card producers inside the same bounded attempt, after
    // the shared check authority completed: the snapshot commits complete —
    // items, findings, head, and card seams — or not at all (RIPR-SPEC-0215).
    // The card producers consume the same resolved workspace configuration
    // as the findings (#6825 review), so one committed snapshot cannot
    // disagree with itself across the two producers.
    super::repair_card::bind_snapshot_card_producers(root, &config, &mut snapshot)?;
    Ok(snapshot)
}

fn overflow_reason_as_str(reason: DiagnosticOverflowReason) -> &'static str {
    match reason {
        DiagnosticOverflowReason::DocumentItemLimit => "document_item_limit",
        DiagnosticOverflowReason::WorkspaceItemLimit => "workspace_item_limit",
        DiagnosticOverflowReason::SerializedByteLimit => "serialized_byte_limit",
        DiagnosticOverflowReason::InlineDetailLimit => "inline_detail_limit",
    }
}

fn omitted_reason_as_str(reason: OmittedDiagnosticReason) -> &'static str {
    match reason {
        OmittedDiagnosticReason::ProfileFiltered => "profile_filtered",
        OmittedDiagnosticReason::DocumentItemLimit => "document_item_limit",
        OmittedDiagnosticReason::WorkspaceItemLimit => "workspace_item_limit",
        OmittedDiagnosticReason::SerializedByteLimit => "serialized_byte_limit",
    }
}

/// Fail closed when even one bounded document cannot fit the wire cap: an
/// over-cap response would otherwise terminate the transport.
pub(crate) fn bounded_document(document: Value) -> Result<Value, AttemptFailure> {
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
    serde_json::to_writer(&mut writer, &document).map_err(|error| {
        AttemptFailure::new(
            CODE_ANALYSIS_FAILED,
            format!("serialize document: {error}"),
            "retry with ripr_refresh",
        )
    })?;
    if writer.0 > super::MAX_RESPONSE_BYTES {
        return Err(AttemptFailure::new(
            CODE_RESULT_TOO_LARGE,
            format!(
                "document is {} bytes; the MCP response bound is {}",
                writer.0,
                super::MAX_RESPONSE_BYTES
            ),
            "read narrower evidence (one item through ripr_get_gap) instead of widening the response",
        ));
    }
    Ok(document)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis_outcome::{
        AnalysisLimitation, AnalysisLimitationKind, AnalysisOutcomeCounts, AnalysisRecovery,
        AnalysisRecoveryKind, AnalysisStage,
    };
    use std::path::PathBuf;

    fn limitation(kind: AnalysisLimitationKind) -> Result<AnalysisLimitation, String> {
        Ok(AnalysisLimitation::new(
            kind,
            AnalysisStage::DiffParse,
            AnalysisRecovery::new(
                AnalysisRecoveryKind::Retry,
                "inspect the typed limitation before retrying",
            )?,
        ))
    }

    fn outcome(
        kind: AnalysisOutcomeKind,
        findings: u64,
        limitations: Vec<AnalysisLimitation>,
    ) -> Result<AnalysisOutcome, String> {
        AnalysisOutcome::new(
            kind,
            Default::default(),
            AnalysisOutcomeCounts {
                changed_file_count: 1,
                changed_line_count: 2,
                candidate_line_count: 2,
                probe_count: 2,
                finding_count: findings,
            },
            limitations,
        )
    }

    /// A CheckOutput shell carrying only what snapshot binding reads. No
    /// analysis runs in these tests: the session contract is exercised
    /// against typed producer outcomes directly.
    fn output(
        kind: AnalysisOutcomeKind,
        findings: u64,
        limitations: Vec<AnalysisLimitation>,
    ) -> Result<crate::app::CheckOutput, String> {
        Ok(crate::app::CheckOutput {
            schema_version: crate::app::CHECK_OUTPUT_SCHEMA_VERSION.to_string(),
            harness_projections: Vec::new(),
            tool: "ripr".to_string(),
            mode: crate::app::Mode::Draft,
            root: PathBuf::from("."),
            base: None,
            analysis_outcome: Some(outcome(kind, findings, limitations)?),
            summary: crate::domain::Summary::default(),
            findings: Vec::new(),
            preview_language_advisories: Vec::new(),
            language_runs: Vec::new(),
            no_scope_provided: false,
            unanalyzed_working_tree: false,
            untracked_working_tree_source_paths: Vec::new(),
            unlinked_python_tests: None,
            suppression: None,
            partial_scope: None,
            analyzed_revisions: None,
        })
    }

    fn session_with(kind: AnalysisOutcomeKind, findings: u64) -> Result<WorkspaceSession, String> {
        let output = output(kind, findings, Vec::new())?;
        let snapshot = Snapshot::from_output(&output, Some("root:sha256:test"))
            .map_err(|failure| failure.detail)?;
        Ok(WorkspaceSession {
            in_flight: false,
            last_good: Some(Arc::new(snapshot)),
            last_failure: None,
            repairs: std::collections::BTreeMap::new(),
            superseded_attempts: std::collections::BTreeMap::new(),
            superseded_order: std::collections::VecDeque::new(),
        })
    }

    fn expect_code(result: Result<Value, AttemptFailure>, code: &str) -> Result<(), String> {
        match result {
            Ok(value) => Err(format!("expected {code}, got success: {value}")),
            Err(failure) if failure.code == code => Ok(()),
            Err(failure) => Err(format!("expected {code}, got {}", failure.code)),
        }
    }

    #[test]
    fn complete_zero_and_incomplete_zero_stay_distinct() -> Result<(), String> {
        let complete = session_with(AnalysisOutcomeKind::CompleteNoFindings, 0)?;
        let limited = output(
            AnalysisOutcomeKind::PartialWithLimitations,
            0,
            vec![limitation(AnalysisLimitationKind::ProducerFailure)?],
        )?;
        let limited_snapshot = Snapshot::from_output(&limited, Some("root:sha256:test"))
            .map_err(|failure| failure.detail)?;
        let incomplete = WorkspaceSession {
            in_flight: false,
            last_good: Some(Arc::new(limited_snapshot)),
            last_failure: None,
            repairs: std::collections::BTreeMap::new(),
            superseded_attempts: std::collections::BTreeMap::new(),
            superseded_order: std::collections::VecDeque::new(),
        };
        let _ = complete
            .list_gaps(None, GapListWindow::default())
            .map_err(|failure| failure.detail)?;
        let incomplete_doc = incomplete
            .list_gaps(None, GapListWindow::default())
            .map_err(|failure| failure.detail)?;
        let complete_snapshot = complete
            .last_good
            .as_ref()
            .ok_or_else(|| "complete session lost its snapshot".to_string())?;
        let incomplete_snapshot = incomplete
            .last_good
            .as_ref()
            .ok_or_else(|| "incomplete session lost its snapshot".to_string())?;
        if complete_snapshot.snapshot_id == incomplete_snapshot.snapshot_id {
            return Err(
                "complete-zero and incomplete-zero must not share a snapshot identity".to_string(),
            );
        }
        if complete_snapshot.outcome.kind == incomplete_snapshot.outcome.kind {
            return Err(
                "complete-zero and incomplete-zero must keep distinct outcome kinds".to_string(),
            );
        }
        let incomplete_list = incomplete_doc
            .pointer("/items")
            .and_then(Value::as_array)
            .ok_or_else(|| "incomplete-zero list lost its items array".to_string())?;
        if !incomplete_list.is_empty() {
            return Err("incomplete-zero must not project phantom items".to_string());
        }
        Ok(())
    }

    #[test]
    fn unsupported_input_never_becomes_a_snapshot() -> Result<(), String> {
        let unsupported = output(
            AnalysisOutcomeKind::UnsupportedInput,
            0,
            vec![limitation(AnalysisLimitationKind::CombinedHunkUnsupported)?],
        )?;
        let failure = Snapshot::from_output(&unsupported, None)
            .err()
            .ok_or_else(|| "unsupported input must fail closed".to_string())?;
        if failure.code != CODE_UNSUPPORTED_PROFILE {
            return Err(format!("unexpected failure code: {}", failure.code));
        }
        Ok(())
    }

    #[test]
    fn list_get_snapshot_before_any_refresh_fail_closed() -> Result<(), String> {
        let session = WorkspaceSession::default();
        expect_code(
            session.list_gaps(None, GapListWindow::default()),
            CODE_NO_SNAPSHOT,
        )?;
        expect_code(session.get_gap("gap:any", None), CODE_NO_SNAPSHOT)?;
        expect_code(
            session.snapshot_document("snapshot:sha256:none"),
            CODE_NO_SNAPSHOT,
        )?;
        let document = session.session_document(&SessionProfile::built_in());
        if document.pointer("/attempt_state").and_then(Value::as_str) != Some("no_snapshot") {
            return Err(format!("fresh session must be no_snapshot: {document}"));
        }
        Ok(())
    }

    #[test]
    fn in_flight_session_reports_analysis_in_flight() -> Result<(), String> {
        let session = WorkspaceSession {
            in_flight: true,
            ..Default::default()
        };
        expect_code(
            session.list_gaps(None, GapListWindow::default()),
            CODE_ANALYSIS_IN_FLIGHT,
        )?;
        expect_code(session.get_gap("gap:any", None), CODE_ANALYSIS_IN_FLIGHT)?;
        expect_code(
            session.snapshot_document("snapshot:sha256:any"),
            CODE_ANALYSIS_IN_FLIGHT,
        )?;
        let document = session.session_document(&SessionProfile::built_in());
        if document.pointer("/attempt_state").and_then(Value::as_str) != Some("in_flight") {
            return Err(format!("in-flight session state drifted: {document}"));
        }
        Ok(())
    }

    #[test]
    fn stale_snapshot_identity_fails_closed_with_current_identity() -> Result<(), String> {
        let session = session_with(AnalysisOutcomeKind::CompleteNoFindings, 0)?;
        let current = session
            .last_good
            .as_ref()
            .ok_or_else(|| "missing snapshot".to_string())?
            .snapshot_id
            .clone();
        match session.list_gaps(Some("snapshot:sha256:old"), GapListWindow::default()) {
            Ok(value) => Err(format!("stale snapshot must fail closed: {value}")),
            Err(failure) if failure.code != CODE_STALE_SNAPSHOT => {
                Err(format!("unexpected failure code: {}", failure.code))
            }
            Err(failure) => {
                if failure
                    .data
                    .pointer("/current_snapshot_id")
                    .and_then(Value::as_str)
                    != Some(current.as_str())
                {
                    return Err(format!(
                        "stale failure lost the current identity: {}",
                        failure.data
                    ));
                }
                Ok(())
            }
        }
    }

    /// A snapshot carrying `count` distinct canonical items, built from the
    /// shared gaps test finding with per-index identities. No analysis runs:
    /// the session contract is exercised against typed producer output.
    fn session_with_findings(count: usize) -> Result<WorkspaceSession, String> {
        let findings = (0..count)
            .map(|index| {
                let mut finding = gaps::test_finding()?;
                finding.id = format!("finding:test:{index}");
                if let Some(gap) = finding.canonical_gap.as_mut() {
                    gap.id = format!("gap:test:{index}");
                }
                finding.probe.id = crate::domain::ProbeId(format!("probe:test:{index}"));
                // Distinct files: the shared budget caps items per document,
                // and this fixture needs a workspace-scale selection.
                finding.probe.location =
                    crate::domain::SourceLocation::new(format!("src/module{index}.rs"), 12, 5);
                Ok(finding)
            })
            .collect::<Result<Vec<_>, String>>()?;
        let output = crate::app::CheckOutput {
            schema_version: crate::app::CHECK_OUTPUT_SCHEMA_VERSION.to_string(),
            harness_projections: Vec::new(),
            tool: "ripr".to_string(),
            mode: crate::app::Mode::Draft,
            root: PathBuf::from("."),
            base: None,
            analysis_outcome: Some(outcome(
                AnalysisOutcomeKind::CompleteWithFindings,
                count as u64,
                Vec::new(),
            )?),
            summary: crate::domain::Summary::default(),
            findings,
            preview_language_advisories: Vec::new(),
            language_runs: Vec::new(),
            no_scope_provided: false,
            unanalyzed_working_tree: false,
            untracked_working_tree_source_paths: Vec::new(),
            unlinked_python_tests: None,
            suppression: None,
            partial_scope: None,
        };
        let snapshot = Snapshot::from_output(&output, Some("root:sha256:test"))
            .map_err(|failure| failure.detail)?;
        Ok(WorkspaceSession {
            in_flight: false,
            last_good: Some(Arc::new(snapshot)),
            last_failure: None,
            repairs: std::collections::BTreeMap::new(),
            superseded_attempts: std::collections::BTreeMap::new(),
            superseded_order: std::collections::VecDeque::new(),
        })
    }

    /// #6021: an aggregate listing too large for one wire response degrades
    /// to disclosed pages. The old behavior returned the whole budget-
    /// approved document and let the tool envelope fail `result_too_large`
    /// after approval — the approved-then-dead sequence. Now the default
    /// call byte-fills one wire-fitting page, discloses the window, and
    /// walking `page.next_offset` covers the selection exactly once.
    #[test]
    fn oversized_aggregate_listing_pages_instead_of_dying_after_budget_approval()
    -> Result<(), String> {
        let session = session_with_findings(700)?;
        let document = session
            .list_gaps(None, GapListWindow::default())
            .map_err(|failure| failure.detail)?;

        // The page itself must fit the wire envelope end to end, with the
        // advertised structured copy intact: the tool declares an
        // outputSchema, so a paged success may not drop structuredContent
        // (#6021 review).
        let envelope = crate::mcp::protocol::tool_result(document.clone())
            .map_err(|error| format!("paged listing must ship: {error}"))?;
        let envelope_bytes = serde_json::to_vec(&envelope).map_err(|error| error.to_string())?;
        if envelope_bytes.len() > crate::mcp::MAX_RESPONSE_BYTES {
            return Err(format!(
                "the paged envelope must fit the wire bound, got {} bytes",
                envelope_bytes.len()
            ));
        }
        if envelope.get("structuredContent").is_none() {
            return Err(
                "a paged listing must keep its structured result; the page trim must size the \
                 double envelope, not fall back to text-only"
                    .to_string(),
            );
        }
        // A byte-filled page must actually sit at the tool document ceiling.
        let document_bytes = serde_json::to_vec(&document).map_err(|error| error.to_string())?;
        if document_bytes.len() > crate::mcp::MAX_TOOL_DOCUMENT_BYTES {
            return Err(format!(
                "the default page must respect the tool document ceiling, got {} bytes",
                document_bytes.len()
            ));
        }

        let selected = document
            .pointer("/selected")
            .and_then(Value::as_u64)
            .ok_or_else(|| "list lost its selected count".to_string())?;
        let returned = document
            .pointer("/page/returned")
            .and_then(Value::as_u64)
            .ok_or_else(|| "list lost its page window".to_string())?;
        if returned == 0 || returned >= selected {
            return Err(format!(
                "a byte-filled page must be partial: returned {returned} of {selected}"
            ));
        }
        if document.pointer("/page/has_more").and_then(Value::as_bool) != Some(true) {
            return Err("a partial page must disclose has_more".to_string());
        }
        if document
            .pointer("/page/next_offset")
            .and_then(Value::as_u64)
            != Some(returned)
        {
            return Err("next_offset must resume after the returned page".to_string());
        }
        if document
            .pointer("/items")
            .and_then(Value::as_array)
            .map(|items| items.len())
            != Some(returned as usize)
        {
            return Err("items must carry exactly the returned page".to_string());
        }
        if document
            .pointer("/continuation/next_page/tool")
            .and_then(Value::as_str)
            != Some("ripr_list_gaps")
        {
            return Err("a partial page must name the next-page route".to_string());
        }
        // The generated next-page route pins the snapshot identity, so a
        // refresh between pages fails closed with stale_snapshot instead of
        // silently mixing snapshots (#6021 review).
        let snapshot_id = document
            .pointer("/snapshot_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "list lost its snapshot identity".to_string())?
            .to_string();
        if document
            .pointer("/continuation/next_page/arguments/snapshot_id")
            .and_then(Value::as_str)
            != Some(snapshot_id.as_str())
        {
            return Err(format!(
                "next_page must pin the snapshot identity {snapshot_id}: {document}"
            ));
        }

        // Walking next_offset covers the selection exactly once, in order,
        // ending in a disclosed final page.
        let mut seen = std::collections::BTreeSet::new();
        let mut pages = 0usize;
        let mut offset = 0usize;
        loop {
            let document = session
                .list_gaps(
                    None,
                    GapListWindow {
                        offset,
                        limit: None,
                    },
                )
                .map_err(|failure| failure.detail)?;
            let ids = document
                .pointer("/items")
                .and_then(Value::as_array)
                .ok_or_else(|| "page lost its items".to_string())?;
            if ids.is_empty() {
                return Err(format!("page at offset {offset} returned no items"));
            }
            for id in ids {
                let id = id
                    .pointer("/canonical_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "item summary lost its identity".to_string())?;
                if !seen.insert(id.to_string()) {
                    return Err(format!("identity {id} appeared on two pages"));
                }
            }
            pages += 1;
            match document.pointer("/page/has_more").and_then(Value::as_bool) {
                Some(true) => {
                    offset = document
                        .pointer("/page/next_offset")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| "has_more without next_offset".to_string())?
                        as usize;
                }
                Some(false) => break,
                other => return Err(format!("page lost has_more: {other:?}")),
            }
            if pages > selected as usize {
                return Err("paging must terminate".to_string());
            }
        }
        if seen.len() != selected as usize {
            return Err(format!(
                "walked {} identities but the selection holds {selected}",
                seen.len()
            ));
        }

        // An explicit limit caps the page and is disclosed.
        let limited = session
            .list_gaps(
                None,
                GapListWindow {
                    offset: 0,
                    limit: Some(1),
                },
            )
            .map_err(|failure| failure.detail)?;
        if limited.pointer("/page/limit").and_then(Value::as_u64) != Some(1)
            || limited.pointer("/page/returned").and_then(Value::as_u64) != Some(1)
            || limited.pointer("/page/has_more").and_then(Value::as_bool) != Some(true)
        {
            return Err(format!(
                "an explicit limit must cap and disclose: {limited}"
            ));
        }
        // An offset past the selection is an empty disclosed page, not an
        // error, and the window echoes the requested offset rather than the
        // clamped slice start (#6021 review).
        let requested_offset = selected as usize + 5;
        let beyond = session
            .list_gaps(
                None,
                GapListWindow {
                    offset: requested_offset,
                    limit: None,
                },
            )
            .map_err(|failure| failure.detail)?;
        if beyond
            .pointer("/items")
            .and_then(Value::as_array)
            .map(|items| !items.is_empty())
            != Some(false)
            || beyond.pointer("/page/has_more").and_then(Value::as_bool) != Some(false)
            || beyond.pointer("/page/next_offset") != Some(&Value::Null)
            || beyond.pointer("/page/offset").and_then(Value::as_u64)
                != Some(requested_offset as u64)
        {
            return Err(format!(
                "an offset past the selection must be an empty final page echoing the request: {beyond}"
            ));
        }
        Ok(())
    }

    /// #6021: below the wire ceiling the default listing still ships the
    /// whole selection in one page — the small-workspace contract is
    /// unchanged apart from the added page disclosure.
    #[test]
    fn fitting_listing_ships_whole_with_a_closed_page_window() -> Result<(), String> {
        let session = session_with_findings(3)?;
        let document = session
            .list_gaps(None, GapListWindow::default())
            .map_err(|failure| failure.detail)?;
        if document.pointer("/selected").and_then(Value::as_u64) != Some(3)
            || document
                .pointer("/items")
                .and_then(Value::as_array)
                .map(|items| items.len())
                != Some(3)
        {
            return Err(format!("small selection must ship whole: {document}"));
        }
        if document.pointer("/page/has_more").and_then(Value::as_bool) != Some(false)
            || document.pointer("/page/next_offset") != Some(&Value::Null)
            || document.pointer("/page/returned").and_then(Value::as_u64) != Some(3)
        {
            return Err(format!(
                "a complete listing must close its window: {document}"
            ));
        }
        let envelope = crate::mcp::protocol::tool_result(document)
            .map_err(|error| format!("small listing must ship: {error}"))?;
        if envelope.get("structuredContent").is_none() {
            return Err("a fitting listing must keep structuredContent".to_string());
        }
        Ok(())
    }

    #[test]
    fn snapshot_resource_binds_identity_and_typed_outcome() -> Result<(), String> {
        let session = session_with(AnalysisOutcomeKind::CompleteNoFindings, 0)?;
        let current = session
            .last_good
            .as_ref()
            .ok_or_else(|| "missing snapshot".to_string())?
            .snapshot_id
            .clone();
        let document = session
            .snapshot_document(&current)
            .map_err(|failure| failure.detail)?;
        if document.pointer("/schema_version").and_then(Value::as_str)
            != Some(SNAPSHOT_SCHEMA_VERSION)
        {
            return Err(format!("snapshot resource lost its schema: {document}"));
        }
        if document.pointer("/snapshot_id").and_then(Value::as_str) != Some(current.as_str()) {
            return Err("snapshot resource lost its identity".to_string());
        }
        if document.pointer("/outcome/kind").and_then(Value::as_str) != Some("complete_no_findings")
        {
            return Err(format!(
                "snapshot resource lost the typed outcome: {document}"
            ));
        }
        Ok(())
    }

    #[test]
    fn unknown_item_fails_closed_and_never_fabricates_evidence() -> Result<(), String> {
        let session = session_with(AnalysisOutcomeKind::CompleteNoFindings, 0)?;
        expect_code(session.get_gap("gap:missing", None), CODE_ITEM_NOT_FOUND)?;
        Ok(())
    }

    #[test]
    fn session_document_projects_attempt_state_and_built_in_profile() -> Result<(), String> {
        let session = session_with(AnalysisOutcomeKind::CompleteNoFindings, 0)?;
        let document = session.session_document(&SessionProfile::built_in());
        if document.pointer("/attempt_state").and_then(Value::as_str) != Some("completed") {
            return Err(format!("completed session state drifted: {document}"));
        }
        if document
            .pointer("/profile/project_config")
            .and_then(Value::as_str)
            != Some("built_in_defaults")
        {
            return Err(format!(
                "built-in profile must disclose its defaults posture: {document}"
            ));
        }
        if document.pointer("/profile/support").and_then(Value::as_str) != Some("built_in_defaults")
        {
            return Err(format!(
                "built-in profile lost its support fact: {document}"
            ));
        }
        let limitations = document
            .pointer("/limitations")
            .and_then(Value::as_array)
            .ok_or_else(|| "session lost its limitations".to_string())?;
        let text = serde_json::to_string(limitations).map_err(|error| error.to_string())?;
        if !text.contains("no workspace ripr.toml was loaded") {
            return Err(format!(
                "a defaults session must disclose the missing config: {text}"
            ));
        }
        if document
            .pointer("/last_completed_snapshot/snapshot_id")
            .is_none()
        {
            return Err("session lost its last completed snapshot identity".to_string());
        }
        if document.pointer("/analysis_outcome/kind").is_none() {
            return Err("session lost its typed analysis outcome".to_string());
        }
        let profile = SessionProfile::built_in();
        if profile.languages.is_empty() {
            return Err("built-in profile must name at least one language".to_string());
        }
        Ok(())
    }

    /// #6825: the session profile resolves the workspace's own
    /// configuration, so a Python-enabled workspace discloses `loaded` with
    /// its config identity and the enabled language — never a rust-only
    /// built-in default.
    #[test]
    fn session_profile_resolves_the_workspace_configuration() -> Result<(), String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-mcp-profile-resolve-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;

        // No config: built-in defaults (no python markers in an empty root).
        let defaults = SessionProfile::resolve(Some(&root));
        if defaults.languages != vec!["rust".to_string()]
            || defaults.posture != SessionConfigPosture::BuiltInDefaults
        {
            return Err(format!(
                "a config-less empty root must keep built-in defaults: {defaults:?}"
            ));
        }
        let document =
            crate::mcp::workspace::WorkspaceSession::default().session_document(&defaults);
        let limitations = document
            .pointer("/limitations")
            .and_then(Value::as_array)
            .ok_or_else(|| "session lost its limitations".to_string())?;
        let text = serde_json::to_string(limitations).map_err(|error| error.to_string())?;
        if !text.contains("no workspace ripr.toml was loaded") {
            return Err(format!("defaults posture must be disclosed: {text}"));
        }

        // A python-enabled workspace: loaded posture with the config
        // identity and the enabled language. Python-only (#4252): a build
        // without `lang-python` refuses the enabled language at load, so
        // the loaded posture is not observable there.
        #[cfg(feature = "lang-python")]
        {
            std::fs::write(
                root.join("ripr.toml"),
                "[languages]\nenabled = [\"python\"]\n",
            )
            .map_err(|error| error.to_string())?;
            let loaded = SessionProfile::resolve(Some(&root));
            match &loaded.posture {
                SessionConfigPosture::Loaded { identity }
                    if identity.starts_with("fnv1a64:") && loaded.languages == vec!["python"] => {}
                other => {
                    return Err(format!(
                        "a python-enabled workspace must project loaded with its language: {other:?}"
                    ));
                }
            }
            let document =
                crate::mcp::workspace::WorkspaceSession::default().session_document(&loaded);
            if document.pointer("/profile/config_identity").is_none() {
                return Err(format!(
                    "a loaded profile must publish its config identity: {document}"
                ));
            }
            let limitations = document
                .pointer("/limitations")
                .and_then(Value::as_array)
                .ok_or_else(|| "session lost its limitations".to_string())?;
            let text = serde_json::to_string(limitations).map_err(|error| error.to_string())?;
            if text.contains("could not be loaded") || !text.contains("re-resolves") {
                return Err(format!(
                    "a loaded profile must carry only the startup-freshness disclosure: {text}"
                ));
            }
        }

        // A present but unparseable config: detected-not-loaded, refresh
        // fails closed with config_invalid.
        std::fs::write(root.join("ripr.toml"), "not valid toml =\n")
            .map_err(|error| error.to_string())?;
        let detected = SessionProfile::resolve(Some(&root));
        if detected.posture != SessionConfigPosture::DetectedNotLoaded
            || detected.languages != vec!["rust".to_string()]
        {
            return Err(format!(
                "an unparseable ripr.toml must project detected-not-loaded: {detected:?}"
            ));
        }
        let document =
            crate::mcp::workspace::WorkspaceSession::default().session_document(&detected);
        let limitations = document
            .pointer("/limitations")
            .and_then(Value::as_array)
            .ok_or_else(|| "session lost its limitations".to_string())?;
        let text = serde_json::to_string(limitations).map_err(|error| error.to_string())?;
        if !text.contains("config_invalid") {
            return Err(format!(
                "detected-not-loaded must name the refresh failure: {text}"
            ));
        }

        std::fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
        Ok(())
    }

    #[test]
    fn failure_detail_is_bounded_and_documents_carry_schema() -> Result<(), String> {
        let long = "x".repeat(MAX_FAILURE_DETAIL_CHARS * 2);
        let failure = AttemptFailure::new(CODE_ANALYSIS_FAILED, long, "retry");
        if failure.detail.chars().count() > MAX_FAILURE_DETAIL_CHARS + 1 {
            return Err(format!(
                "failure detail was not bounded: {}",
                failure.detail.len()
            ));
        }
        let document = failure.document(REFRESH_SCHEMA_VERSION);
        if document.pointer("/schema_version").and_then(Value::as_str)
            != Some(REFRESH_SCHEMA_VERSION)
            || document.pointer("/failure/code").and_then(Value::as_str)
                != Some(CODE_ANALYSIS_FAILED)
        {
            return Err(format!("failure document lost its contract: {document}"));
        }
        Ok(())
    }

    #[test]
    fn refresh_limitations_distinguish_cancel_commit_from_teardown_abandon() -> Result<(), String> {
        // A cancel notification does not stop a running attempt: it still
        // commits as completed. Only transport teardown abandons an attempt
        // before commit (#5254 item 2).
        let session = session_with(AnalysisOutcomeKind::CompleteNoFindings, 0)?;
        let document = refresh_document(&session);
        let limitations = document
            .pointer("/limitations")
            .and_then(Value::as_array)
            .ok_or_else(|| "refresh document lost its limitations".to_string())?;
        let text = serde_json::to_string(limitations).map_err(|error| error.to_string())?;
        if !text.contains("still commits as a completed snapshot")
            || !text.contains("only transport teardown abandons an attempt before it commits")
        {
            return Err(format!("cancel/commit limitations drifted: {text}"));
        }
        if text.contains("is never committed as a completed snapshot") {
            return Err(format!("false never-committed claim remained: {text}"));
        }
        Ok(())
    }

    #[test]
    fn refresh_document_reports_failure_without_dropping_last_known_good() -> Result<(), String> {
        let mut session = session_with(AnalysisOutcomeKind::CompleteNoFindings, 0)?;
        let failure = AttemptFailure::new(CODE_ANALYSIS_FAILED, "producer hiccup", "retry");
        session.last_failure = Some(failure);
        let document = refresh_document(&session);
        if document.pointer("/attempt/state").and_then(Value::as_str) != Some("failed") {
            return Err(format!("failed attempt state drifted: {document}"));
        }
        if document
            .pointer("/attempt/failure/code")
            .and_then(Value::as_str)
            != Some(CODE_ANALYSIS_FAILED)
        {
            return Err("failed attempt lost its typed failure".to_string());
        }
        if document.pointer("/last_known_good/snapshot_id").is_none() {
            return Err("a failed refresh must not drop the last-known-good snapshot".to_string());
        }
        if document.pointer("/snapshot/snapshot_id").is_none() {
            return Err("the committed snapshot summary must survive a later failure".to_string());
        }
        Ok(())
    }

    /// #5995: the refresh analyzes only the committed default-branch diff.
    /// On a dirty tracked file it reports `no_scope` with zero findings
    /// while `ripr check --worktree` and an LSP session both report the
    /// finding — so the refresh result and the workspace status must carry
    /// the producer's unanalyzed-working-tree fact as a typed scope
    /// disclosure naming the `--worktree` route, and must stay silent on a
    /// clean tracked tree. Without it, `no_scope` reads as all-clear.
    #[test]
    fn dirty_tree_refresh_and_status_disclose_the_committed_diff_scope() -> Result<(), String> {
        let mut dirty = output(AnalysisOutcomeKind::CompleteNoFindings, 0, Vec::new())?;
        // The issue's shape verbatim: `no_scope` with every count zero.
        dirty.analysis_outcome = Some(AnalysisOutcome::new(
            AnalysisOutcomeKind::NoScope,
            Default::default(),
            AnalysisOutcomeCounts::default(),
            Vec::new(),
        )?);
        dirty.unanalyzed_working_tree = true;
        let snapshot =
            Snapshot::from_output(&dirty, Some("root:sha256:test")).map_err(|f| f.detail)?;
        assert!(
            snapshot.scope.uncommitted_edits,
            "the producer fact must bind onto the snapshot"
        );
        let session = WorkspaceSession {
            in_flight: false,
            last_good: Some(Arc::new(snapshot)),
            last_failure: None,
            repairs: std::collections::BTreeMap::new(),
            superseded_attempts: std::collections::BTreeMap::new(),
            superseded_order: std::collections::VecDeque::new(),
        };

        let refresh = refresh_document(&session);
        if refresh.pointer("/scope/analyzed").and_then(Value::as_str)
            != Some("committed default-branch diff")
        {
            return Err(format!(
                "refresh must disclose the analyzed scope: {refresh}"
            ));
        }
        if refresh
            .pointer("/scope/uncommitted_edits")
            .and_then(Value::as_str)
            != Some("outside this analysis")
        {
            return Err(format!(
                "refresh must disclose the excluded edits: {refresh}"
            ));
        }
        let note = refresh
            .pointer("/scope/note")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("scope disclosure must name the repair route: {refresh}"))?;
        if !note.contains("--worktree") {
            return Err(format!(
                "scope note must name `ripr check --worktree`: {note}"
            ));
        }

        let status = session.session_document(&SessionProfile::built_in());
        if status.pointer("/scope/analyzed").and_then(Value::as_str)
            != Some("committed default-branch diff")
        {
            return Err(format!("status must disclose the analyzed scope: {status}"));
        }
        if !status
            .pointer("/scope/note")
            .and_then(Value::as_str)
            .is_some_and(|note| note.contains("--worktree"))
        {
            return Err(format!(
                "status scope note must name the worktree route: {status}"
            ));
        }

        // A clean tracked tree carries no exclusion to disclose.
        let clean = session_with(AnalysisOutcomeKind::CompleteNoFindings, 0)?;
        if !refresh_document(&clean)["scope"].is_null() {
            return Err("a clean tree must not invent a scope exclusion".to_string());
        }
        if !clean.session_document(&SessionProfile::built_in())["scope"].is_null() {
            return Err("a clean tree must not invent a scope exclusion".to_string());
        }

        // #5995 review: an untracked-only tree must not receive the bare
        // `--worktree` remedy — untracked files are invisible to it (#5258).
        // The disclosure names staging or an explicit diff instead.
        let mut untracked_only = output(AnalysisOutcomeKind::CompleteNoFindings, 0, Vec::new())?;
        untracked_only.analysis_outcome = dirty.analysis_outcome.clone();
        untracked_only.unanalyzed_working_tree = true;
        untracked_only.untracked_working_tree_source_paths =
            vec!["src/new.rs".to_string(), "src/other_new.rs".to_string()];
        let untracked_snapshot = Snapshot::from_output(&untracked_only, Some("root:sha256:test"))
            .map_err(|f| f.detail)?;
        let untracked_session = WorkspaceSession {
            in_flight: false,
            last_good: Some(Arc::new(untracked_snapshot)),
            last_failure: None,
            repairs: std::collections::BTreeMap::new(),
            superseded_attempts: std::collections::BTreeMap::new(),
            superseded_order: std::collections::VecDeque::new(),
        };
        let untracked_refresh = refresh_document(&untracked_session);
        let note = untracked_refresh
            .pointer("/scope/note")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                format!("untracked disclosure must carry a note: {untracked_refresh}")
            })?;
        if !note.contains("staged and unstaged tracked edits only") {
            return Err(format!(
                "an untracked tree must bound the worktree remedy: {note}"
            ));
        }
        if !note.contains("stage them") {
            return Err(format!(
                "an untracked tree must name the staging remedy: {note}"
            ));
        }
        let files = untracked_refresh
            .pointer("/scope/untracked_edits/files")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                format!("untracked disclosure must name the files: {untracked_refresh}")
            })?;
        if files.len() != 2 {
            return Err(format!(
                "the disclosure must name both untracked files: {files:?}"
            ));
        }
        if untracked_refresh
            .pointer("/scope/untracked_edits/total")
            .and_then(Value::as_u64)
            != Some(2)
        {
            return Err(format!(
                "untracked disclosure must carry the total: {untracked_refresh}"
            ));
        }

        Ok(())
    }

    /// #5995 review: the scope facts change what the served documents
    /// disclose, so two otherwise identical snapshots with different
    /// exclusions must not share a snapshot identity.
    #[test]
    fn snapshot_identity_distinguishes_excluded_edit_facts() -> Result<(), String> {
        let no_scope_outcome = AnalysisOutcome::new(
            AnalysisOutcomeKind::NoScope,
            Default::default(),
            AnalysisOutcomeCounts::default(),
            Vec::new(),
        )?;
        let mut clean = output(AnalysisOutcomeKind::CompleteNoFindings, 0, Vec::new())?;
        clean.analysis_outcome = Some(no_scope_outcome.clone());
        let mut dirty = clean.clone();
        dirty.unanalyzed_working_tree = true;
        let clean_snapshot =
            Snapshot::from_output(&clean, Some("root:sha256:test")).map_err(|f| f.detail)?;
        let dirty_snapshot =
            Snapshot::from_output(&dirty, Some("root:sha256:test")).map_err(|f| f.detail)?;
        if clean_snapshot.snapshot_id == dirty_snapshot.snapshot_id {
            return Err(
                "two snapshots differing only in the excluded-edit facts must not share an id"
                    .to_string(),
            );
        }
        Ok(())
    }

    #[test]
    fn portable_snapshot_identity_ignores_the_concrete_root() -> Result<(), String> {
        let first = output(AnalysisOutcomeKind::CompleteNoFindings, 0, Vec::new())?;
        let second = output(AnalysisOutcomeKind::CompleteNoFindings, 0, Vec::new())?;
        let one = Snapshot::from_output(&first, Some("root:sha256:one")).map_err(|f| f.detail)?;
        let two = Snapshot::from_output(&second, Some("root:sha256:two")).map_err(|f| f.detail)?;
        if one.snapshot_id != two.snapshot_id {
            return Err(
                "equivalent roots must preserve one portable snapshot identity".to_string(),
            );
        }
        Ok(())
    }

    #[test]
    fn portable_snapshot_identity_survives_distinct_absolute_checkouts() -> Result<(), String> {
        // #5254 item 6: the same finding content committed from two checkouts
        // at different absolute roots must share one snapshot identity, since
        // every served file renders root-relative. Absolute host paths in
        // evidence would fork the identity per checkout. Host-native roots
        // (no filesystem touch) so the pin holds on every host.
        let one_root = std::env::temp_dir().join("ripr-portable-one");
        let two_root = std::env::temp_dir().join("ripr-portable-two");
        let mut first = output(AnalysisOutcomeKind::CompleteWithFindings, 1, Vec::new())?;
        first.root = one_root.clone();
        let mut first_finding = gaps::test_finding()?;
        first_finding.probe.location.file = one_root.join("src/lib.rs");
        first_finding.related_tests[0].file = one_root.join("tests/checkout.rs");
        first.findings.push(first_finding);

        let mut second = output(AnalysisOutcomeKind::CompleteWithFindings, 1, Vec::new())?;
        second.root = two_root.clone();
        let mut second_finding = gaps::test_finding()?;
        second_finding.probe.location.file = two_root.join("src/lib.rs");
        second_finding.related_tests[0].file = two_root.join("tests/checkout.rs");
        second.findings.push(second_finding);

        let one = Snapshot::from_output(&first, Some("root:sha256:test")).map_err(|f| f.detail)?;
        let two = Snapshot::from_output(&second, Some("root:sha256:test")).map_err(|f| f.detail)?;
        if one.snapshot_id != two.snapshot_id {
            return Err(format!(
                "distinct checkouts must share one portable snapshot identity: {} vs {}",
                one.snapshot_id, two.snapshot_id
            ));
        }
        Ok(())
    }

    #[test]
    fn snapshot_identity_binds_same_length_evidence_with_the_original_digest() -> Result<(), String>
    {
        let mut before = output(AnalysisOutcomeKind::CompleteWithFindings, 1, Vec::new())?;
        let mut finding = gaps::test_finding()?;
        finding.recommended_next_step = Some("assert boundary a".to_string());
        before.findings.push(finding);
        let before_snapshot =
            Snapshot::from_output(&before, Some("root:sha256:test")).map_err(|f| f.detail)?;

        let mut after = output(AnalysisOutcomeKind::CompleteWithFindings, 1, Vec::new())?;
        let mut finding = gaps::test_finding()?;
        finding.recommended_next_step = Some("assert boundary b".to_string());
        after.findings.push(finding);
        let after_snapshot =
            Snapshot::from_output(&after, Some("root:sha256:test")).map_err(|f| f.detail)?;

        let before_id = before_snapshot.snapshot_id.clone();
        let after_id = after_snapshot.snapshot_id.clone();
        let mut lengths = Vec::new();
        for snapshot in [before_snapshot, after_snapshot] {
            assert_eq!(
                snapshot.items.len(),
                1,
                "the snapshot must contain evidence"
            );
            let mut original_items = snapshot
                .findings
                .iter()
                // Same root the `output()` shell commits with, so the
                // re-projection reproduces the committed bytes exactly.
                .map(|finding| GapItem::from_finding(finding, Path::new(".")))
                .collect::<Result<Vec<_>, _>>()?;
            for item in &mut original_items {
                let bytes = serde_json::to_vec(&item.evidence_core).map_err(|e| e.to_string())?;
                item.evidence_bytes = bytes.len();
                item.evidence_sha256 = sha256_hex(&bytes);
                lengths.push(bytes.len());
            }
            let original_id =
                snapshot_identity(&snapshot.outcome, &original_items, &snapshot.scope)?;
            assert_eq!(snapshot.snapshot_id, original_id);

            let session = WorkspaceSession {
                in_flight: false,
                last_good: Some(Arc::new(snapshot)),
                last_failure: None,
                repairs: std::collections::BTreeMap::new(),
                superseded_attempts: std::collections::BTreeMap::new(),
                superseded_order: std::collections::VecDeque::new(),
            };
            let item = original_items
                .first()
                .ok_or_else(|| "the original item oracle must not be empty".to_string())?;
            let document = session
                .get_gap(&item.canonical_id, Some(&original_id))
                .map_err(|f| f.detail)?;
            assert_eq!(document, item.document(&original_id));
        }
        assert_eq!(lengths.len(), 2);
        assert_eq!(
            lengths[0], lengths[1],
            "the changed evidence has equal length"
        );
        assert_ne!(before_id, after_id);
        Ok(())
    }

    #[test]
    fn reserved_failure_vocabulary_stays_named() -> Result<(), String> {
        for code in RESERVED_FAILURE_CODES {
            if code.is_empty() || code.contains(' ') {
                return Err(format!("reserved code is not a wire token: {code}"));
            }
        }
        Ok(())
    }
}

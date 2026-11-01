//! Test-grip evidence per RIPR-SPEC-0005, v1.
//!
//! For each `RepoSeam`, build per-stage evidence (reach / activate /
//! propagate / observe / discriminate) using the existing `RustIndex`
//! facts. This is **not** classification: the output is a per-stage
//! evidence record, not a `SeamGripClass`. The classification PR
//! (`analysis/repo-ripr-classification-v1`) consumes these records.
//!
//! Determinism: `evidence_for_seams` sorts by `seam_id`. Within each
//! evidence record, `related_tests` are deduped and ranked by relation
//! confidence, relation reason, oracle strength, activation overlap,
//! then stable file/name/line tie-breakers.

mod boolean_boundary_pin;
mod call_budget;
mod owner_result_binding;
mod panic_outcome;
mod reach_limit;
mod related_tests;
pub(crate) mod shared_grips;
mod snapshot_owner_binding;
pub(crate) use snapshot_owner_binding::{BindingReach, local_is_owner_result_before, text_calls};
mod value_contradiction;

use boolean_boundary_pin::BooleanBoundaryPin;
pub(crate) use value_contradiction::CONTRADICTION_DISCLOSURE;
use value_contradiction::{
    ExactValueVerdict, boolean_assertion_verdict_for_test, contradiction_summary,
    exact_value_assertion_verdict_for_test,
};
pub(in crate::analysis) use value_contradiction::{
    InvisibilityReason, PointerReturnChange, change_is_invisible_to_every_assertion,
};

pub(crate) use related_tests::CompactGripContext;
use related_tests::{
    CompactTest, assertion_target_tokens, call_text_contains_named_call, find_related_tests_ranked,
    find_related_tests_with_context, required_discriminator_text, sort_related_tests_for_seam,
    strip_comments_and_strings, test_assertion_mentions_any_target_token,
};

use super::classify::{
    assertion_observes_collection_argument, assertion_observes_direct_collection_for_probe,
    call_text_may_call_free_function, direct_collection_mutation_receiver, is_ignored,
    owner_call_text, predicate_line_arms_all_agree, reach_evidence as classify_reach_evidence,
    related_tests_reach_owner_indirectly, relation_establishes_reach, test_calls_free_function,
};
use super::facts::{CallFact, FunctionImplContext};
use super::new_test_target::{self, NewTestTargetAdmission};
use super::resource_cost::trace_latency_phase;
use super::rust_index::{
    self, FunctionSummary, OracleFact, RustIndex, TestSummary, extract_call_facts,
    extract_identifier_tokens,
};
use super::seams::{ExpectedSink, RepoSeam, SeamId, SeamKind};
use crate::analysis::cancellation;
use crate::{
    domain::{
        Confidence, MissingDiscriminatorFact, OracleKind, OracleStrength, StageEvidence,
        StageState, SymbolId, ValueContext,
    },
    value_origins::LocatedValueFact as ValueFact,
};
// Re-export so callers that import from this module continue to compile.
pub(crate) use crate::domain::{RelationConfidence, RelationReason};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::HashMap;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Per-seam test-grip evidence record.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct TestGripEvidence {
    pub(crate) seam_id: SeamId,
    /// Shared records: equal grips across seams are one allocation (#5341).
    pub(crate) related_tests: Vec<Arc<RelatedTestGrip>>,
    pub(crate) reach: StageEvidence,
    pub(crate) activate: StageEvidence,
    pub(crate) propagate: StageEvidence,
    pub(crate) observe: StageEvidence,
    pub(crate) discriminate: StageEvidence,
    pub(crate) observed_values: Vec<ValueFact>,
    pub(crate) missing_discriminators: Vec<MissingDiscriminatorFact>,
    /// #7007: how many related tests carry a statically contradicted
    /// exact-value assertion, counted by the producer over the FULL related
    /// set. This is the completeness authority for the contradiction gate:
    /// the evidence record's `related_tests` array is a capped projection
    /// and each entry names only its best oracle, so absence of a disclosure
    /// there cannot establish absence from the evidence set. No serde
    /// default: a stored entry predating this field must miss (the cache
    /// generation bump) rather than deserialize as a silent zero.
    pub(crate) statically_contradicted_related_tests: usize,
    /// Producer-owned Integration or InlineUnit proposal, or the typed
    /// blocker that kept the target `Missing`. Compact evidence leaves this
    /// empty so the compact classified-seam cache does not need a generation
    /// bump.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) new_test_target: Option<NewTestTargetAdmission>,
}

impl TestGripEvidence {
    /// Whether any stage summary names the typed static `limit`: the evidence
    /// record's own marker that a named limitation, not a missing test,
    /// explains the stage (#10703).
    pub(crate) fn names_static_limit(&self, limit: crate::domain::StaticLimitKind) -> bool {
        [
            &self.reach,
            &self.activate,
            &self.propagate,
            &self.observe,
            &self.discriminate,
        ]
        .iter()
        .any(|stage| stage.summary.contains(limit.as_str()))
    }
}

const COMPACT_RELATED_TEST_LIMIT: usize = 12;
const EVIDENCE_PROGRESS_CHUNK: usize = 500;
const HELPER_OWNER_CALL_GRAPH_MAX_HOPS: usize = 3;

/// Per-related-test grip facts attached to a `TestGripEvidence`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RelatedTestGrip {
    pub(crate) test_name: String,
    pub(crate) file: PathBuf,
    pub(crate) line: usize,
    pub(crate) test_target: Option<TestTargetEvidence>,
    pub(crate) oracle_kind: OracleKind,
    pub(crate) oracle_strength: OracleStrength,
    pub(crate) evidence_summary: String,
    pub(crate) relation_reason: RelationReason,
    pub(crate) relation_confidence: RelationConfidence,
}

/// Hashes the identifying fields only; `Eq` still compares every field, so
/// records that differ elsewhere collide and stay distinct.
impl std::hash::Hash for RelatedTestGrip {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.test_name.hash(state);
        self.file.hash(state);
        self.line.hash(state);
        self.evidence_summary.hash(state);
        std::mem::discriminant(&self.relation_reason).hash(state);
    }
}

/// Producer-owned identity for an existing Rust test target.
///
/// This is populated from the Rust index's test function facts. Renderers may
/// display it, but may not reconstruct it from a path, name, or line tuple.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TestTargetEvidence {
    symbol_id: SymbolId,
    file: PathBuf,
    line: usize,
    test_kind: TestKind,
    relation: RelationReason,
    provenance: TestTargetProvenance,
    workspace_identity: String,
    currentness: TestTargetCurrentness,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TestTargetCurrentness {
    /// Only indexed, on-disk byte-current targets are emitted as evidence;
    /// stale, missing, or invalid targets are rejected before serialization.
    Current,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TestKind {
    InlineUnit,
    Integration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TestTargetProvenance {
    RustIndexFunction,
    #[cfg(test)]
    FixtureOnly,
}

impl TestTargetEvidence {
    pub(crate) fn from_index(
        symbol_id: SymbolId,
        file: PathBuf,
        line: usize,
        test_kind: TestKind,
        relation: RelationReason,
        workspace_identity: String,
    ) -> Self {
        Self {
            symbol_id,
            file,
            line,
            test_kind,
            relation,
            provenance: TestTargetProvenance::RustIndexFunction,
            workspace_identity,
            currentness: TestTargetCurrentness::Current,
        }
    }

    pub(crate) fn symbol_id(&self) -> &SymbolId {
        &self.symbol_id
    }

    /// Projection accessors for the RepairCard builder (#4663): the card
    /// copies these facts verbatim and never re-derives them.
    pub(crate) fn relation(&self) -> RelationReason {
        self.relation
    }

    pub(crate) fn test_kind(&self) -> TestKind {
        self.test_kind
    }

    pub(crate) fn workspace_identity(&self) -> &str {
        &self.workspace_identity
    }

    pub(crate) fn file(&self) -> &Path {
        &self.file
    }

    pub(crate) fn line(&self) -> usize {
        self.line
    }
}

#[cfg(test)]
impl TestTargetEvidence {
    pub(crate) fn fixture(name: &str, file: &std::path::Path, line: usize) -> Self {
        Self {
            symbol_id: SymbolId(format!("fixture:test:{}:{}:{}", file.display(), line, name)),
            file: file.to_path_buf(),
            line,
            // Integration-vs-inline kind comes from the typed test-file
            // authority, not a local path re-derivation (#3534).
            test_kind: if crate::analysis::rust_index::is_test_file(file) {
                TestKind::Integration
            } else {
                TestKind::InlineUnit
            },
            relation: RelationReason::DirectOwnerCall,
            provenance: TestTargetProvenance::FixtureOnly,
            workspace_identity: "fixture".to_string(),
            currentness: TestTargetCurrentness::Current,
        }
    }

    pub(crate) fn provenance(&self) -> TestTargetProvenance {
        self.provenance
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OracleSemantics {
    pub(crate) observes: String,
    pub(crate) missing: String,
    pub(crate) upgrade_suggestion: Option<String>,
}

/// Build evidence records for a slice of seams. Output is sorted by
/// `seam_id` so two runs over the same input produce identical bytes.
///
/// A cancellation checkpoint can stop this non-fallible helper early, so a
/// caller that runs under a cancellation context must checkpoint immediately
/// after this function returns before using or publishing the vector.
pub(crate) fn evidence_for_seams(seams: &[RepoSeam], index: &RustIndex) -> Vec<TestGripEvidence> {
    EvidencePass::new(index).evidence_for(seams)
}

/// One test context shared by several [`EvidencePass::evidence_for`]
/// calls, so a caller that evaluates seams in stages builds the test
/// context once. Its caches are keyed memoization, so a seam's evidence
/// does not depend on which seams the pass evaluated before it.
/// Fewest seams in a stage for which the whole-index tables are built up
/// front.
const WARM_MIN_SEAMS: usize = 1_000;

pub(crate) struct EvidencePass<'index> {
    context: Option<CompactGripContext<'index>>,
    /// The whole-index tables are paid once per pass, not per stage.
    shared_tables_warmed: std::sync::Once,
}

impl<'index> EvidencePass<'index> {
    pub(crate) fn new(index: &'index RustIndex) -> Self {
        let context_started = Instant::now();
        trace_latency_phase("evidence_context", "start", Duration::ZERO);
        let context = CompactGripContext::try_new(index).ok();
        trace_latency_phase(
            "evidence_context",
            &format!(
                "tests_{}",
                context.as_ref().map_or(0, |context| context.tests.len())
            ),
            context_started.elapsed(),
        );
        Self {
            context,
            shared_tables_warmed: std::sync::Once::new(),
        }
    }

    /// Drop memoized owner/AST detail at a window boundary. Immutable test
    /// indexes and file-bounded scalar/line facts stay reusable.
    pub(crate) fn clear_window_memos(&self) {
        if let Some(context) = &self.context {
            context.clear_window_memos();
        }
    }

    /// Evidence for `seams`, sorted by `seam_id`; empty when the context
    /// could not be built. Carries the same cancellation contract as
    /// [`evidence_for_seams`].
    pub(crate) fn evidence_for(&self, seams: &[RepoSeam]) -> Vec<TestGripEvidence> {
        let Some(context) = self.context.as_ref() else {
            return Vec::new();
        };
        if seams.is_empty() {
            // An empty stage must not pay the whole-index warm below.
            return Vec::new();
        }
        let evidence_started = Instant::now();
        // Seams are independent and the context's memos are keyed, so the
        // pass runs on the rayon pool. Workers inherit this thread's
        // cancellation token and committed-source overlay; a cancelled
        // worker drops its seam, and the caller's checkpoint discards the
        // partial vector exactly as it did for the serial loop.
        let token = cancellation::current_token();
        let overlay = crate::analysis::committed_source::current_overlay();
        // #9211: a stage large enough to need the whole-index tables pays
        // for them here, before any seam is queued, instead of the first
        // worker to ask building one in line while its siblings wait. A first
        // review stage of a few dozen seams reads none of them, and the warm
        // costs tens of seconds on a workspace the size of ripr-swarm.
        if seams.len() >= WARM_MIN_SEAMS {
            self.warm_shared_tables(context, token.as_ref(), &overlay);
        }
        let processed = AtomicUsize::new(0);
        let mut out: Vec<TestGripEvidence> = seams
            .par_iter()
            .filter_map(|seam| {
                cancellation::with_optional_token(token.as_ref(), || {
                    crate::analysis::committed_source::with_overlay(overlay.clone(), || {
                        cancellation::checkpoint().ok()?;
                        let evidence = evidence_for_seam_with_context(seam, context);
                        let processed = processed.fetch_add(1, Ordering::Relaxed) + 1;
                        if processed.is_multiple_of(EVIDENCE_PROGRESS_CHUNK)
                            || processed == seams.len()
                        {
                            trace_latency_phase(
                                "evidence_for_seams_progress",
                                &format!("processed_{processed}_of_{}", seams.len()),
                                evidence_started.elapsed(),
                            );
                        }
                        Some(evidence)
                    })
                })
            })
            .collect();
        out.sort_by(|a, b| a.seam_id.as_str().cmp(b.seam_id.as_str()));
        out
    }

    /// Pay the macro-site scan, the unseen-reach names and the type-mention
    /// index once per pass, on the caller, where the scans fan out over the
    /// pool.
    fn warm_shared_tables(
        &self,
        context: &CompactGripContext<'_>,
        token: Option<&cancellation::AnalysisCancellationToken>,
        overlay: &Option<std::sync::Arc<crate::analysis::committed_source::CommittedSourceOverlay>>,
    ) {
        self.shared_tables_warmed.call_once(|| {
            cancellation::with_optional_token(token, || {
                crate::analysis::committed_source::with_overlay(overlay.clone(), || {
                    // A cancelled pass returns no evidence, so it stops paying
                    // for tables nobody will read.
                    if cancellation::checkpoint().is_err() {
                        return;
                    }
                    let started = Instant::now();
                    context.prime_pin_syntax_scan();
                    trace_latency_phase("evidence_macro_scan", "primed", started.elapsed());
                    if cancellation::checkpoint().is_err() {
                        return;
                    }
                    let started = Instant::now();
                    context.unseen_reach_names();
                    trace_latency_phase("evidence_unseen_reach", "built", started.elapsed());
                    if cancellation::checkpoint().is_err() {
                        return;
                    }
                    let started = Instant::now();
                    context.type_mentions();
                    trace_latency_phase("evidence_type_mentions", "built", started.elapsed());
                });
            });
        });
    }
}

/// Build evidence for a single seam.
#[cfg(test)]
pub(crate) fn evidence_for_seam(seam: &RepoSeam, index: &RustIndex) -> TestGripEvidence {
    let context = CompactGripContext::new(index);
    evidence_for_seam_with_context(seam, &context)
}

/// #12060: the related tests as the stage evidence should read them. A match
/// arm names an enum variant other functions also return, so a test related
/// to the owner only by file or module cannot confirm the arm through an
/// assertion that calls a different indexed function with no path to the
/// owner (`assert_eq!(reduce(..), Reduction::Over(..))` for an arm of
/// `consume`). Repo relation also ties such a test by assertion affinity,
/// because the assertion names the arm's variant, so that tie is refused
/// the same way. `ripr check` refuses the same assertion per assertion
/// (RIPR-SPEC-0094, #11552); the repo seam must agree.
fn credited_related_tests<'a>(
    seam: &RepoSeam,
    context: &CompactGripContext<'_>,
    owner_fn: Option<&FunctionSummary>,
    related: &[(&'a TestSummary, RelationReason)],
) -> Vec<std::borrow::Cow<'a, TestSummary>> {
    let owner_reach = (seam.kind() == SeamKind::MatchArm)
        .then(|| owner_fn.map(|owner| context.transitive_reach.owner_reach(&owner.name)))
        .flatten();
    related
        .iter()
        .map(|(test, reason)| {
            let Some(owner_reach) = owner_reach.as_ref() else {
                return std::borrow::Cow::Borrowed(*test);
            };
            if !(reason.is_proximity_only() || *reason == RelationReason::AssertionTargetAffinity)
                || !test
                    .assertions
                    .iter()
                    .any(|oracle| owner_reach.assertion_pins_other_function(&oracle.text))
            {
                return std::borrow::Cow::Borrowed(*test);
            }
            let mut credited = (*test).clone();
            credited
                .assertions
                .retain(|oracle| !owner_reach.assertion_pins_other_function(&oracle.text));
            std::borrow::Cow::Owned(credited)
        })
        .collect()
}

fn evidence_for_seam_with_context(
    seam: &RepoSeam,
    context: &CompactGripContext<'_>,
) -> TestGripEvidence {
    let mut related_with_reason = find_related_tests_with_context(seam, context);
    let owner_fn = context.owner_function(seam.file(), seam.display_line());
    let best_oracles =
        sort_related_tests_for_seam(seam, context, owner_fn, &mut related_with_reason);
    let related_indexed: Vec<&CompactTest<'_>> = related_with_reason
        .iter()
        .map(|(indexed, _reason)| *indexed)
        .collect();

    let related: Vec<&TestSummary> = related_indexed.iter().map(|indexed| indexed.test).collect();

    let related_reasons: Vec<(&TestSummary, RelationReason)> = related_with_reason
        .iter()
        .map(|(indexed, reason)| (indexed.test, *reason))
        .collect();
    let reach = reach_evidence(seam, &related_reasons, owner_fn, context);
    let (activate, observed_values, missing_discriminators) =
        activate_evidence(seam, &related_indexed, context, owner_fn);
    let pin_syntax = context.pin_syntax();
    let credited_views = credited_related_tests(seam, context, owner_fn, &related_reasons);
    let credited: Vec<&TestSummary> = credited_views.iter().map(AsRef::as_ref).collect();
    let propagate = propagate_evidence(seam, &credited, owner_fn, context.index, pin_syntax);
    let observe = observe_evidence(&credited);
    let discriminate = discriminate_evidence(seam, &credited, owner_fn, context.index, pin_syntax);

    // One admission per test file for this seam: the verdict depends only on
    // the (test file, seam file) pair, and a file's tests share it (#5361).
    let mut validated_targets: HashMap<&Path, bool> = HashMap::new();
    let related_tests: Vec<Arc<RelatedTestGrip>> = related_with_reason
        .iter()
        .zip(best_oracles)
        .map(|((indexed, reason), oracle)| {
            context.share_grip(related_test_grip(
                seam,
                indexed.test,
                *reason,
                context,
                oracle,
                &mut validated_targets,
            ))
        })
        .collect();
    let new_test_target = new_test_target_admission(seam, context);
    // Counted over the full related set, not the rendered record projection:
    // this count is the contradiction gate's completeness authority (#7007).
    let statically_contradicted_related_tests = related
        .iter()
        .filter(|test| test_has_contradicted_assertion(test, owner_fn))
        .count();

    // For a changed match arm: which related tests run the arm through their
    // one call of the owner, passing the arm's own literal in the argument
    // the arm's match reads (aligned with `related_tests`). Empty when the arm
    // is not a plain literal arm of the owner's top-level match on a bare
    // parameter, so the start withholds.
    let arm_literal_tests: Vec<bool> = owner_fn
        .filter(|_| seam.kind() == SeamKind::MatchArm)
        .and_then(|owner| arm_scrutinee_argument(seam, owner).map(|arm| (owner, arm)))
        .map(|(owner, (parameter, literal))| {
            related_with_reason
                .iter()
                .map(|(indexed, _)| {
                    test_runs_arm_in_its_one_owner_call(indexed.test, owner, parameter, &literal)
                })
                .collect()
        })
        .unwrap_or_default();
    let mut missing_discriminators = missing_discriminators;
    let path_established = [&reach, &activate, &propagate]
        .iter()
        .all(|stage| stage.state == StageState::Yes);
    if let Some(fact) = return_value_missing_discriminator(
        seam,
        AlignedRelatedTests {
            grips: &related_tests,
            summaries: &related,
            passes_arm_literal: &arm_literal_tests,
        },
        path_established.then_some(&discriminate),
        statically_contradicted_related_tests,
        owner_fn,
        context.index,
    ) {
        missing_discriminators.push(fact);
    }

    let mut evidence = TestGripEvidence {
        seam_id: seam.id().clone(),
        related_tests,
        reach,
        activate,
        propagate,
        observe,
        discriminate,
        observed_values,
        missing_discriminators,
        statically_contradicted_related_tests,
        new_test_target,
    };
    // #8600: read over the full related set, the same set the stages read.
    let saturated = call_budget::any_saturated(related.iter().copied());
    call_budget::withhold(&mut evidence, saturated);
    // #10703: the same named limit `check` applies to a `#[should_panic]`
    // test whose panic may be the changed code's.
    let panicking = panic_outcome::deciding_tests(
        related_with_reason
            .iter()
            .filter(|(_, reason)| *reason == RelationReason::DirectOwnerCall)
            .map(|(indexed, _)| {
                (
                    indexed.test.name.as_str(),
                    indexed.test.file.as_path(),
                    indexed.test.start_line,
                )
            }),
        owner_fn,
        seam.display_line(),
        context.index,
    );
    panic_outcome::withhold(seam, &mut evidence, &panicking);
    evidence
}

/// The producer-owned `return_value` missing-discriminator fact (#10701): a
/// changed plain return expression is repairable only when a related test
/// calls the owner directly with an edit target, reach, activation and
/// propagation hold, the oracle is weak or absent, and no assertion is
/// statically contradicted. Anything else emits nothing (fail-closed).
const RETURN_VALUE_START_REASON: &str = "no related test asserts the exact changed return value";
const ERROR_VARIANT_START_REASON: &str = "no related test asserts the exact changed error variant";
/// A changed match arm under the same gates, and only when a related test
/// passes the arm's own literal pattern, so the test is known to select the
/// changed arm rather than a sibling one.
const MATCH_ARM_START_REASON: &str =
    "no related test asserts the exact result of the changed match arm";

/// The related tests of one seam, aligned by position: each test's grip, its
/// summary, and whether it passes the changed arm's literal as the matched
/// argument (always `false` off a match arm).
struct AlignedRelatedTests<'a> {
    grips: &'a [Arc<RelatedTestGrip>],
    summaries: &'a [&'a TestSummary],
    passes_arm_literal: &'a [bool],
}

fn return_value_missing_discriminator(
    seam: &RepoSeam,
    related: AlignedRelatedTests<'_>,
    discriminate: Option<&StageEvidence>,
    statically_contradicted_related_tests: usize,
    owner_fn: Option<&FunctionSummary>,
    index: &RustIndex,
) -> Option<MissingDiscriminatorFact> {
    let AlignedRelatedTests {
        grips: related_tests,
        summaries: related_summaries,
        passes_arm_literal: arm_literal_tests,
    } = related;
    if !matches!(
        seam.kind(),
        SeamKind::ReturnValue | SeamKind::ErrorVariant | SeamKind::MatchArm
    ) || statically_contradicted_related_tests > 0
    {
        return None;
    }
    // `check` withholds the gap when a test reaches the owner as a function
    // value or inside an executor helper; repair must agree.
    let owner_fn = owner_fn?;
    let related_identities: Vec<(&str, &Path, usize)> = related_tests
        .iter()
        .map(|test| (test.test_name.as_str(), test.file.as_path(), test.line))
        .collect();
    if related_tests_reach_owner_indirectly(
        seam.file(),
        seam.display_line(),
        &owner_fn.name,
        &related_identities,
        index,
    ) {
        return None;
    }
    // `discriminate` is `None` unless reach, activation and propagation are
    // established; a statically limited change must not start a repair.
    if !matches!(
        discriminate.map(|stage| &stage.state),
        Some(StageState::Weak | StageState::No)
    ) {
        return None;
    }
    // `check` judges a struct literal per changed field and the seam does not
    // know which changed, so the start is withheld.
    if seam.kind() == SeamKind::ReturnValue && is_struct_literal_expression(seam.expression()) {
        return None;
    }
    let (description, reason) = match seam.required_discriminator() {
        super::seams::RequiredDiscriminator::ReturnValue { description } => {
            (description, RETURN_VALUE_START_REASON)
        }
        super::seams::RequiredDiscriminator::ErrorVariant { variant } => {
            (variant, ERROR_VARIANT_START_REASON)
        }
        super::seams::RequiredDiscriminator::MatchArmTaken { arm }
            if seam.kind() == SeamKind::MatchArm =>
        {
            (arm, MATCH_ARM_START_REASON)
        }
        _ => return None,
    };
    if description.trim().is_empty() {
        return None;
    }
    // A related test that asserts an exact error variant other than the new
    // one is contradicted by the change (or not about it): adding a variant
    // assertion beside it is not the repair, so the start is withheld.
    if reason == ERROR_VARIANT_START_REASON
        && related_summaries.iter().any(|test| {
            test.assertions.iter().any(|oracle| {
                oracle.kind == OracleKind::ExactErrorVariant
                    && !mentions_path(&oracle.text, description.as_str())
            })
        })
    {
        return None;
    }
    // A lone equality between an owner call and an owner-free value
    // (`assert!(f(x) == lit)`, or the terminal Err-return guard twin) already
    // pins the changed value, so repair must not start; syntactic, errs
    // toward withholding.
    if related_summaries
        .iter()
        .any(|test| test_pins_owner_call_by_equality(test, &owner_fn.name, index))
    {
        return None;
    }
    // `#[ignore]` never runs and `#[should_panic]` passes by panicking, so a
    // strengthened one is no repair: such a related owner caller withholds.
    if related_summaries
        .iter()
        .any(|test| is_ignored(test) || is_should_panic(test))
    {
        return None;
    }
    // An assertion cannot fix a test whose failure is swallowed: a non-plain
    // `Drop` type may exit the process while unwinding, so `check` already
    // demotes an exact assertion. An unreadable workspace withholds.
    if names_non_plain_drop_type(owner_fn, seam.expression(), index) {
        return None;
    }
    // A start needs an assertion that observes the owner's result (the call or
    // a local bound from it); `let _ = f(2); assert!(1 > 0);` never reads it.
    if !related_summaries
        .iter()
        .any(|test| test_assertion_observes_owner(test, owner_fn, index))
    {
        return None;
    }
    let direct_call_with_target = if seam.kind() == SeamKind::MatchArm {
        // A match-arm start needs ONE related test that does everything at
        // once: passes the arm's literal in the argument the arm's match
        // reads, calls the owner directly with an edit target under `check`'s
        // own relation rules (no test-local shadow type, #6951), and asserts
        // on the result. Gates satisfied by different tests prove nothing
        // about the arm. (A struct-field change keeps its own producer route
        // in `activate_evidence`.)
        // The admitted assertions are trusted by name, so a workspace that
        // rebinds `assert!`/`assert_eq!`/`assert_ne!` (a `macro_rules!` or a
        // `use .. as assert`) may never evaluate the call: refuse, as the
        // check-side evidence reader does.
        impl_identity_is_unambiguous(owner_fn, index)
            && free_owner_name_is_unique(owner_fn, index)
            && !crate::analysis::classify::macro_names_rebound(
                index,
                &["assert", "assert_eq", "assert_ne"],
            )
            && related_tests
                .iter()
                .zip(related_summaries.iter())
                .zip(arm_literal_tests.iter())
                .any(|((test, summary), passes_literal)| {
                    *passes_literal
                        && test.relation_reason == RelationReason::DirectOwnerCall
                        && test.test_target.is_some()
                        && crate::analysis::classify::owner_call_is_direct(summary, owner_fn, index)
                        && test_assertion_observes_owner(summary, owner_fn, index)
                })
    } else {
        related_tests.iter().any(|test| {
            test.relation_reason == RelationReason::DirectOwnerCall && test.test_target.is_some()
        })
    };
    direct_call_with_target.then(|| MissingDiscriminatorFact {
        value: description.clone(),
        reason: reason.to_string(),
        flow_sink: None,
    })
}

/// Whether `test` expects a panic, so passing means panicking (any
/// `should_panic` form, with or without an expected message).
fn is_should_panic(test: &TestSummary) -> bool {
    test.attrs.iter().any(|attr| {
        let compact: String = attr.chars().filter(|ch| !ch.is_whitespace()).collect();
        compact.contains("should_panic")
    })
}

#[cfg(test)]
thread_local! {
    /// Times the masked-source memo was built on this thread (#10701 work pin).
    static MASK_BUILD_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Whether the owner's signature or the changed expression may name a type
/// with a non-plain `Drop` impl. Fails closed: `Self` is read as the impl's
/// self type (withheld when not established); a type alias withholds; a
/// `use .. as Name` rename is followed; a workspace struct, enum or union is
/// followed into the types it names; a return type that is one of the owner's
/// generic parameters withholds; a workspace holding a macro or outside-crate
/// item calls no type plain. A name no workspace source declares is a
/// standard-library type, whose `Drop` cannot exit the process.
fn names_non_plain_drop_type(owner: &FunctionSummary, expression: &str, index: &RustIndex) -> bool {
    let body: &str = &owner.body;
    let signature = body.find('{').map_or(body, |open| &body[..open]);
    if returns_generic_or_opaque_type(owner, signature, index) {
        return true;
    }
    // Masked once per index, read once per type name.
    let masked = index.call_identity_memos.masked_sources(|| {
        #[cfg(test)]
        MASK_BUILD_COUNT.with(|count| count.set(count.get() + 1));
        index
            .files()
            .iter()
            .map(|(_, facts)| crate::analysis::extract::mask_comments_and_strings(&facts.source))
            .collect()
    });
    let uppercase_words = |text: &str| -> Vec<String> {
        text.split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
            .filter(|word| word.starts_with(|ch: char| ch.is_ascii_uppercase()))
            .map(str::to_string)
            .collect()
    };
    let mut pending = uppercase_words(&format!("{signature}\n{expression}"));
    let mut seen: BTreeSet<String> = BTreeSet::new();
    while let Some(word) = pending.pop() {
        let word = if word == "Self" {
            match &owner.impl_context {
                FunctionImplContext::Impl { self_type } => self_type.clone(),
                _ => return true,
            }
        } else {
            word
        };
        if !seen.insert(word.clone()) {
            continue;
        }
        let (aliased, next) = index.call_identity_memos.type_word_facts(&word, || {
            let aliased = masked
                .iter()
                .any(|source| source_declares_type_alias_masked(source, &word));
            let mut next = Vec::new();
            for source in masked {
                next.extend(renamed_from(source, &word));
                next.extend(uppercase_words(&type_definition_region(source, &word)));
            }
            (aliased, next)
        });
        if aliased || !crate::analysis::classify::drop_is_plain_in_closed_sources(index, &word) {
            return true;
        }
        pending.extend(next);
    }
    false
}

/// Whether the owner's return type is one the caller (or an implementor)
/// chooses, so the start cannot prove it plain: it names a generic parameter of
/// the function or of the enclosing `impl`/`trait` header, is `Self` inside a
/// generic `impl`, or is opaque (`impl Trait`, `dyn Trait`). Unbalanced generics
/// withhold.
fn returns_generic_or_opaque_type(
    owner: &FunctionSummary,
    signature: &str,
    index: &RustIndex,
) -> bool {
    let word = |ch: char| ch.is_alphanumeric() || ch == '_';
    let Some(fn_at) = signature.find("fn ") else {
        return false;
    };
    let after = &signature[fn_at + 3..];
    let name_end = after.find(|ch: char| !word(ch)).unwrap_or(after.len());
    let rest = after[name_end..].trim_start();
    let mut names: Vec<String> = Vec::new();
    let rest = if rest.starts_with('<') {
        let Some((params, end)) = generic_parameter_names(rest) else {
            return true;
        };
        names.extend(params);
        &rest[end..]
    } else {
        rest
    };
    // The return type starts at the first `->` after the parameter list.
    let Some(open) = rest.find('(') else {
        return false;
    };
    let mut depth = 0usize;
    let mut close = None;
    for (offset, ch) in rest[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    close = Some(open + offset + 1);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(close) = close else {
        return true;
    };
    let Some(ret) = rest[close..].trim_start().strip_prefix("->") else {
        return false;
    };
    let ret = ret
        .split(|ch: char| !word(ch))
        .take_while(|token| *token != "where")
        .collect::<Vec<_>>();
    let impl_generics = enclosing_header_generics(owner, index);
    let Some(impl_generics) = impl_generics else {
        return true;
    };
    let in_generic_impl = !impl_generics.is_empty();
    names.extend(impl_generics);
    ret.iter().any(|token| {
        names.iter().any(|name| name.as_str() == *token)
            || matches!(*token, "impl" | "dyn")
            || (in_generic_impl && *token == "Self")
    })
}

/// The generic parameter names of every `impl`/`trait` header before the owner
/// in its file, or `None` when unreadable (withhold). Over-reads: a preceding
/// header need not enclose the owner.
fn enclosing_header_generics(owner: &FunctionSummary, index: &RustIndex) -> Option<Vec<String>> {
    let Some(facts) = index.files().get(&owner.file) else {
        return Some(Vec::new());
    };
    let masked = crate::analysis::extract::mask_comments_and_strings(&facts.source);
    // Earlier lines plus the owner's line up to its `fn` (same-line header).
    let lines: Vec<&str> = masked.lines().collect();
    let mut before = lines
        .iter()
        .take(owner.start_line.saturating_sub(1))
        .copied()
        .collect::<Vec<_>>()
        .join("\n");
    if let Some(own) = lines.get(owner.start_line.saturating_sub(1)) {
        // Cut at the owner's own `fn <name>`, not an earlier `fn` of the line.
        let own_fn = format!("fn {}", owner.name);
        let upto = own
            .find(&own_fn)
            .or_else(|| own.find("fn "))
            .map_or(*own, |at| &own[..at]);
        before.push('\n');
        before.push_str(upto);
    }
    let part = |ch: Option<char>| ch.is_some_and(|ch| ch.is_alphanumeric() || ch == '_');
    let mut names = Vec::new();
    for keyword in ["impl", "trait"] {
        for (at, word) in before.match_indices(keyword) {
            if part(before[..at].chars().next_back())
                || part(before[at + word.len()..].chars().next())
            {
                continue;
            }
            let header = before[at + keyword.len()..].trim_start();
            if header.starts_with('<') {
                names.extend(generic_parameter_names(header)?.0);
            } else if keyword == "trait" {
                // `trait Name<T>`: skip the name, then read any generics.
                let name_end = header.find(|ch: char| !(ch.is_alphanumeric() || ch == '_'));
                let rest = name_end.map_or("", |end| header[end..].trim_start());
                if rest.starts_with('<') {
                    names.extend(generic_parameter_names(rest)?.0);
                }
            }
        }
    }
    Some(names)
}

/// The parameter names of the generics list at the start of `text` and the
/// offset past its closing `>`, or `None` when unbalanced. A `->` in a bound
/// does not close it.
fn generic_parameter_names(text: &str) -> Option<(Vec<String>, usize)> {
    let word = |ch: char| ch.is_alphanumeric() || ch == '_';
    let mut depth = 0usize;
    let mut params: Vec<&str> = Vec::new();
    let mut start = 1;
    let mut end = None;
    let mut previous = ' ';
    for (offset, ch) in text.char_indices() {
        match ch {
            '<' | '(' | '[' => depth += 1,
            '>' if previous == '-' => {}
            '>' | ')' | ']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    params.push(&text[start..offset]);
                    end = Some(offset + 1);
                    break;
                }
            }
            ',' if depth == 1 => {
                params.push(&text[start..offset]);
                start = offset + 1;
            }
            _ => {}
        }
        previous = ch;
    }
    let end = end?;
    let names = params
        .iter()
        .filter_map(|param| {
            let param = param.trim();
            let param = param.strip_prefix("const ").unwrap_or(param).trim_start();
            if param.starts_with('\'') {
                return None;
            }
            param
                .split(|ch: char| !word(ch))
                .next()
                .filter(|name| !name.is_empty())
                .map(str::to_string)
        })
        .collect();
    Some((names, end))
}

/// The names `target` is imported as through `use .. Original as target`.
fn renamed_from(masked: &str, target: &str) -> Vec<String> {
    let part = |ch: Option<char>| ch.is_some_and(|ch| ch.is_alphanumeric() || ch == '_');
    let mut originals = Vec::new();
    for (at, _) in masked.match_indices(target) {
        if part(masked[..at].chars().next_back())
            || part(masked[at + target.len()..].chars().next())
        {
            continue;
        }
        let before = masked[..at].trim_end();
        let Some(before) = before.strip_suffix("as") else {
            continue;
        };
        if part(before.chars().next_back()) {
            continue;
        }
        let before = before.trim_end();
        let start = before
            .rfind(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
            .map_or(0, |found| found + 1);
        let mut original = &before[start..];
        if original == "self" {
            // `use Type::{Other, self as Name}`: the original is the segment
            // before the group holding `self`.
            let mut depth = 0usize;
            let mut open = None;
            for (offset, ch) in before[..start].char_indices().rev() {
                match ch {
                    '}' => depth += 1,
                    '{' if depth == 0 => {
                        open = Some(offset);
                        break;
                    }
                    '{' => depth -= 1,
                    _ => {}
                }
            }
            let head = open.map_or("", |open| before[..open].trim_end());
            let head = head.strip_suffix("::").unwrap_or(head);
            let begin = head
                .rfind(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
                .map_or(0, |found| found + 1);
            original = &head[begin..];
        }
        if !original.is_empty() {
            originals.push(original.to_string());
        }
    }
    originals
}

/// The text of every `struct`, `enum` or `union` named `name` (name through the
/// closing `}` or `;`), joined. Same-named types in different modules are all
/// read, since the path that picks one is not resolved.
fn type_definition_region(masked: &str, name: &str) -> String {
    let part = |ch: Option<char>| ch.is_some_and(|ch| ch.is_alphanumeric() || ch == '_');
    let mut regions = String::new();
    for keyword in ["struct", "enum", "union"] {
        for (at, word) in masked.match_indices(keyword) {
            let rest = &masked[at + word.len()..];
            if part(masked[..at].chars().next_back()) || !rest.starts_with(char::is_whitespace) {
                continue;
            }
            let rest = rest.trim_start();
            let ident: String = rest
                .chars()
                .take_while(|ch| ch.is_alphanumeric() || *ch == '_')
                .collect();
            if ident != name {
                continue;
            }
            let mut depth = 0usize;
            let mut region_end = rest.len();
            for (offset, ch) in rest.char_indices() {
                match ch {
                    '(' | '{' | '[' => depth += 1,
                    ')' | ']' => depth = depth.saturating_sub(1),
                    '}' => {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            region_end = offset + 1;
                            break;
                        }
                    }
                    ';' if depth == 0 => {
                        region_end = offset + 1;
                        break;
                    }
                    _ => {}
                }
            }
            regions.push_str(&rest[ident.len()..region_end]);
            regions.push('\n');
        }
    }
    regions
}

/// Token-level read of `type <name>` in comment- and string-masked source.
#[cfg(test)]
fn source_declares_type_alias(source: &str, name: &str) -> bool {
    source_declares_type_alias_masked(
        &crate::analysis::extract::mask_comments_and_strings(source),
        name,
    )
}

/// [`source_declares_type_alias`] over already-masked source text.
fn source_declares_type_alias_masked(masked: &str, name: &str) -> bool {
    let part = |ch: char| ch.is_alphanumeric() || ch == '_';
    masked.match_indices("type").any(|(at, word)| {
        let before = masked[..at].chars().next_back();
        let rest = &masked[at + word.len()..];
        if before.is_some_and(part) || !rest.starts_with(char::is_whitespace) {
            return false;
        }
        let rest = rest.trim_start();
        let ident: String = rest.chars().take_while(|ch| part(*ch)).collect();
        ident == name
    })
}

/// Whether an impl owner's self type (and trait, for a trait impl) is declared
/// exactly once in the workspace and never renamed or aliased, so no test can
/// be calling a same-named shadow type instead (#6950 / #6951 / #7067). A free
/// function passes; an owner id whose impl type cannot be read withholds.
fn impl_identity_is_unambiguous(owner: &FunctionSummary, index: &RustIndex) -> bool {
    if matches!(owner.impl_context, FunctionImplContext::Free) {
        return true;
    }
    let Some(self_type) = crate::analysis::classify::impl_self_type_name(&owner.id.0) else {
        return false;
    };
    let mut names: Vec<(String, &[&str])> = vec![(self_type, &["struct", "enum", "union"])];
    if let Some(trait_name) = crate::analysis::classify::owner_trait_name(&owner.id.0) {
        names.push((trait_name, &["trait"]));
    }
    names.iter().all(|(name, keywords)| {
        let mut declarations = 0usize;
        for facts in index.files().values() {
            if !facts.source.contains(name.as_str()) {
                continue;
            }
            let masked = crate::analysis::extract::mask_comments_and_strings(&facts.source);
            if source_declares_type_alias_masked(&masked, name)
                || !renamed_from(&masked, name).is_empty()
            {
                return false;
            }
            declarations += count_item_declarations(&masked, name, keywords);
        }
        declarations == 1
    })
}

/// Whether a free-function owner is the only free function of its name in
/// the workspace. A same-named free function in another module (`b::rate`
/// beside `a::rate`) can be what a bare `rate(..)` call reaches through a
/// file-level import, which the call reader does not resolve, so the start
/// withholds. A method owner passes: its receiver and impl type are checked
/// by [`impl_identity_is_unambiguous`] and the direct-call relation.
fn free_owner_name_is_unique(owner: &FunctionSummary, index: &RustIndex) -> bool {
    !matches!(owner.impl_context, FunctionImplContext::Free)
        || index
            .functions()
            .iter()
            .filter(|function| {
                function.name == owner.name
                    && matches!(function.impl_context, FunctionImplContext::Free)
            })
            .count()
            == 1
}

/// How many `<keyword> <name>` item declarations masked source holds.
fn count_item_declarations(masked: &str, name: &str, keywords: &[&str]) -> usize {
    let part = |ch: Option<char>| ch.is_some_and(|ch| ch.is_alphanumeric() || ch == '_');
    keywords
        .iter()
        .map(|keyword| {
            masked
                .match_indices(keyword)
                .filter(|(at, word)| {
                    let rest = &masked[at + word.len()..];
                    if part(masked[..*at].chars().next_back())
                        || !rest.starts_with(char::is_whitespace)
                    {
                        return false;
                    }
                    let ident: String = rest
                        .trim_start()
                        .chars()
                        .take_while(|ch| ch.is_alphanumeric() || *ch == '_')
                        .collect();
                    ident == *name
                })
                .count()
        })
        .sum()
}

/// Whether `pattern` is one plain literal: a string or char literal with no
/// escape or inner quote, or a decimal integer. Anything else withholds.
fn is_plain_literal_pattern(pattern: &str) -> bool {
    let pattern = pattern.trim();
    let is_string = pattern.len() >= 2
        && pattern.starts_with('"')
        && pattern.ends_with('"')
        && !pattern[1..pattern.len() - 1].contains(['"', '\\']);
    let is_char = pattern.len() >= 3
        && pattern.starts_with('\'')
        && pattern.ends_with('\'')
        && !pattern[1..pattern.len() - 1].contains(['\'', '\\']);
    let is_integer = {
        let digits = pattern.strip_prefix('-').unwrap_or(pattern);
        !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit())
    };
    is_string || is_char || is_integer
}

/// For a changed match arm, the owner parameter index the arm's match reads
/// and the arm's literal, only when the shape makes "a test passes this
/// literal in that argument" mean "the test runs this arm": the owner's whole
/// body is one `match <parameter> { .. }` on a bare parameter, the changed arm
/// is a top-level arm of it whose pattern is one plain literal, and every
/// earlier arm is plain literals only (no guard, binding, wildcard, range or
/// path that could take the input first). Anything else is `None`.
fn arm_scrutinee_argument(seam: &RepoSeam, owner: &FunctionSummary) -> Option<(usize, String)> {
    let target = seam.expression().trim().trim_end_matches("=>").trim();
    if !is_plain_literal_pattern(target) {
        return None;
    }
    let (scrutinee, arms) = whole_body_match_arms(owner)?;
    let (_, parameters) = arm_owner_signature(owner)?;
    let parameter = parameters.iter().position(|name| *name == scrutinee)?;
    for (pattern, line) in arms {
        if line == seam.display_line() && pattern.trim() == target {
            return Some((parameter, target.to_string()));
        }
        // An earlier arm naming the same literal (alone or in a `|`
        // alternation) takes that input first, so the changed arm never runs.
        if !pattern.split('|').all(is_plain_literal_pattern)
            || pattern
                .split('|')
                .any(|alternative| same_literal_value(alternative, target))
        {
            return None;
        }
    }
    None
}

/// Whether two plain literal patterns can match the same input: equal text,
/// or two integers of equal value however written (`01`, `-0`, `0`). An
/// integer too large to read counts as equal, so the caller refuses.
fn same_literal_value(left: &str, right: &str) -> bool {
    let (left, right) = (left.trim(), right.trim());
    let integer = |text: &str| {
        let digits = text.strip_prefix('-').unwrap_or(text);
        !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit())
    };
    if left == right {
        return true;
    }
    if !(integer(left) && integer(right)) {
        return false;
    }
    match (left.parse::<i128>(), right.parse::<i128>()) {
        (Ok(left), Ok(right)) => left == right,
        _ => true,
    }
}

/// Whether `test` runs the changed arm through its one call of `owner`: the
/// call's argument at `parameter` (as the owner's parameter list counts it,
/// without `self`) is the arm's literal written directly in the call. A
/// local, constant or computed argument refuses, since a later `let` or
/// assignment may give the call another value; every other shape refuses
/// too: see [`arm_owner_call_argument`].
fn test_runs_arm_in_its_one_owner_call(
    test: &TestSummary,
    owner: &FunctionSummary,
    parameter: usize,
    literal: &str,
) -> bool {
    arm_owner_call_argument(test, owner, parameter).is_some_and(|argument| argument == literal)
}

/// The argument text at `parameter` of the ONE call of `owner` in `test`, so
/// the result an assertion reads can only be the result of that call. `None`
/// (refuse) when a credited helper also calls the owner, or when the test
/// body is not one of the shapes [`arm_call_argument_in_body`] admits.
fn arm_owner_call_argument(
    test: &TestSummary,
    owner: &FunctionSummary,
    parameter: usize,
) -> Option<String> {
    let name = owner.name.as_str();
    if name.is_empty() || test.calls.iter().filter(|call| call.name == name).count() > 1 {
        return None;
    }
    let (takes_self, _) = arm_owner_signature(owner)?;
    arm_call_argument_in_body(&test.body, name, takes_self, parameter)
}

/// The owner's parameter list read strictly from its first line: whether it
/// takes a `self` receiver, and the other parameters' names in order. `None`
/// (refuse) unless the line holds `fn <name>(..)` with no generic parameters
/// on the function itself (an enclosing `impl<T>` keeps positions), and
/// every parameter is `name: Type` (or `mut name: Type`) whose type is plain
/// words, paths, references and lifetimes: a `<`, bracket, tuple or pattern
/// could hold commas that would shift the parameter positions, and a
/// visibility like `pub(crate)` is skipped by anchoring on the name.
fn arm_owner_signature(owner: &FunctionSummary) -> Option<(bool, Vec<String>)> {
    let signature = owner.body.lines().next()?;
    let part = |ch: Option<char>| ch.is_some_and(|ch| ch.is_alphanumeric() || ch == '_');
    let header = format!("fn {}", owner.name);
    let at = signature
        .match_indices(header.as_str())
        .find(|(at, _)| {
            !part(signature[..*at].chars().next_back())
                && !part(signature[at + header.len()..].chars().next())
        })
        .map(|(at, _)| at + header.len())?;
    let rest = signature[at..].trim_start().strip_prefix('(')?;
    let list = &rest[..rest.find(')')?];
    let mut takes_self = false;
    let mut names = Vec::new();
    for (position, parameter) in list.split(',').enumerate() {
        let parameter = parameter.trim();
        if parameter.is_empty() {
            continue;
        }
        let (name, kind) = match parameter.split_once(':') {
            Some((name, kind)) => (name.trim(), Some(kind.trim())),
            None => (parameter, None),
        };
        let receiver = name.trim_start_matches('&').trim_start();
        let receiver = match receiver.strip_prefix('\'') {
            Some(lifetime) => lifetime
                .trim_start_matches(|ch: char| ch.is_alphanumeric() || ch == '_')
                .trim_start(),
            None => receiver,
        };
        let receiver = receiver.strip_prefix("mut ").unwrap_or(receiver).trim();
        if receiver == "self" {
            if position != 0 || kind.is_some() {
                return None;
            }
            takes_self = true;
            continue;
        }
        let name = name.strip_prefix("mut ").unwrap_or(name).trim();
        let kind = kind?;
        let plain_type = !kind.is_empty()
            && kind
                .chars()
                .all(|ch| ch.is_alphanumeric() || ch.is_whitespace() || "_&':".contains(ch));
        if !plain_type
            || name.is_empty()
            || name.starts_with(|ch: char| ch.is_ascii_digit())
            || !name.chars().all(|ch| ch.is_alphanumeric() || ch == '_')
        {
            return None;
        }
        names.push(name.to_string());
    }
    Some((takes_self, names))
}

/// The assertion macros whose condition operands an arm call may sit in, and
/// how many leading arguments are operands (the rest is the failure message,
/// which runs only when the assertion fails).
/// The `debug_assert` forms are left out: they are compiled out of release
/// test builds.
const ARM_CALL_ASSERTIONS: [(&str, usize); 3] = [("assert", 1), ("assert_eq", 2), ("assert_ne", 2)];

/// Words that let a statement end the test early or skip code.
const ARM_EARLY_EXIT_WORDS: [&str; 10] = [
    "return",
    "break",
    "continue",
    "else",
    "loop",
    "panic",
    "unreachable",
    "todo",
    "unimplemented",
    "exit",
];

/// The pure core of [`arm_owner_call_argument`] over one test body. An
/// allowlist: the owner's name is written exactly once in the body, as a call
/// whose arguments line up with the owner's parameters (a `self` method only
/// as `value.method(..)`, any other owner never so) and are all simple tokens
/// (a literal, a name, a field or path, optionally borrowed). That call is
/// the whole right side of a top-level `let name = ..;`, or sits in the
/// condition operands of a top-level `assert!`-family statement, with only
/// plain operand text around it (no closure, block, branch, short-circuit,
/// `?` or extra argument). No attribute appears in the body, and no other
/// top-level statement can end the test early. Anything else is `None`.
fn arm_call_argument_in_body(
    body: &str,
    name: &str,
    takes_self: bool,
    parameter: usize,
) -> Option<String> {
    let masked = crate::analysis::extract::mask_comments_and_strings(body);
    let part = |ch: Option<char>| ch.is_some_and(|ch| ch.is_alphanumeric() || ch == '_');
    let mut mentions = masked.match_indices(name).filter(|(at, _)| {
        !part(masked[..*at].chars().next_back()) && !part(masked[at + name.len()..].chars().next())
    });
    let (at, _) = mentions.next()?;
    if mentions.next().is_some() || masked[..at].trim_end().ends_with('.') != takes_self {
        return None;
    }
    let after = at + name.len();
    let open = after + (masked[after..].len() - masked[after..].trim_start().len());
    if !masked[open..].starts_with('(') {
        return None;
    }
    let close = matching_close(&masked, open)?;
    let (statements, body_open) = top_level_statements(&masked)?;
    // An outer `#[cfg(..)]` or inner `#![cfg(..)]` (or any attribute) inside
    // the body may compile the call, an assertion or the whole test out.
    if masked[body_open..].contains('#') {
        return None;
    }
    let (start, end) = *statements
        .iter()
        .find(|(start, end)| *start <= at && close < *end)?;
    let others_may_exit = statements
        .iter()
        .filter(|span| span.0 != start)
        .any(|(from, to)| {
            let text = &masked[*from..*to];
            text.contains('?')
                || ARM_EARLY_EXIT_WORDS
                    .iter()
                    .any(|word| mentions_identifier(text, word))
        });
    if others_may_exit || !call_statement_admits(&masked, start, end, at, close) {
        return None;
    }
    let mut arguments = Vec::new();
    let mut from = open + 1;
    while let Some(comma) = find_top_level(&masked, from, close, ",") {
        arguments.push(body.get(from..comma)?.trim());
        from = comma + 1;
    }
    let last = body.get(from..close)?.trim();
    if !last.is_empty() {
        arguments.push(last);
    }
    if !arguments
        .iter()
        .all(|argument| is_simple_call_argument(argument))
    {
        return None;
    }
    arguments
        .get(parameter)
        .map(|argument| argument.to_string())
}

/// The top-level statement spans (split at `;` only) of a masked test body,
/// inside its own braces when it starts with a `fn` header, and the offset
/// the statements start at. `None` when the brackets do not balance.
fn top_level_statements(masked: &str) -> Option<(Vec<(usize, usize)>, usize)> {
    let header = masked.find('{').map_or("", |brace| &masked[..brace]);
    let base = usize::from(mentions_identifier(header, "fn"));
    let body_open = if base == 1 { masked.find('{')? + 1 } else { 0 };
    let mut statements = Vec::new();
    let mut stack: Vec<char> = Vec::new();
    let mut start = body_open;
    let mut cursor = body_open;
    while cursor < masked.len() {
        cursor = skip_literal(masked, cursor);
        let Some(ch) = masked[cursor..].chars().next() else {
            break;
        };
        match ch {
            '(' | '[' | '{' => stack.push(ch),
            ')' | ']' | '}' => {
                let Some(_) = stack.pop() else {
                    // The body's own closing brace.
                    statements.push((start, cursor));
                    return (base == 1).then_some((statements, body_open));
                };
            }
            ';' if stack.is_empty() => {
                statements.push((start, cursor));
                start = cursor + 1;
            }
            _ => {}
        }
        cursor += ch.len_utf8();
    }
    (base == 0 && stack.is_empty()).then(|| {
        statements.push((start, masked.len()));
        (statements, body_open)
    })
}

/// Whether the statement `start..end` holding the owner call `at..=close` is
/// one admitted shape: `let <name>[: <type>] = <operand>` or an assertion
/// macro from [`ARM_CALL_ASSERTIONS`] whose condition operand holds the call,
/// with the operand around the call plain (see [`is_plain_operand_text`]).
fn call_statement_admits(masked: &str, start: usize, end: usize, at: usize, close: usize) -> bool {
    let statement = &masked[start..end];
    let lead = statement.len() - statement.trim_start().len();
    let text = statement.trim();
    let text_start = start + lead;
    let text_end = text_start + text.len();
    let operand = if let Some(rest) = text.strip_prefix("let ") {
        let Some(eq) = rest.find('=') else {
            return false;
        };
        let pattern = &rest[..eq];
        if rest[eq + 1..].starts_with('=')
            || !pattern.chars().all(|ch| {
                ch.is_alphanumeric() || ch == '_' || ch == ':' || ch == '&' || ch.is_whitespace()
            })
        {
            return false;
        }
        (text_start + 4 + eq + 1, text_end)
    } else {
        let Some((_, operands)) = ARM_CALL_ASSERTIONS.iter().find(|(name, _)| {
            text.strip_prefix(name)
                .and_then(|rest| rest.strip_prefix('!'))
                .is_some_and(|rest| rest.trim_start().starts_with('('))
        }) else {
            return false;
        };
        let Some(open_rel) = text.find('(') else {
            return false;
        };
        let open = text_start + open_rel;
        if matching_close(masked, open) != Some(text_end - 1) {
            return false;
        }
        // The operand that holds the call must be one of the leading
        // condition operands, never a message argument.
        let mut from = open + 1;
        let mut index = 0usize;
        loop {
            let to = find_top_level(masked, from, text_end - 1, ",").unwrap_or(text_end - 1);
            if from <= at && close < to {
                if index >= *operands {
                    return false;
                }
                break (from, to);
            }
            if to >= text_end - 1 {
                return false;
            }
            from = to + 1;
            index += 1;
        }
    };
    let (from, to) = operand;
    from <= at
        && close < to
        && is_plain_operand_text(&masked[from..at])
        && is_plain_operand_text(&masked[close + 1..to])
}

/// Operand text around the owner call that always evaluates the call: names,
/// literals, paths, field and method access, arithmetic and comparison, but
/// no closure bar, short-circuit `&&`, `?`, block, index, extra argument or
/// early-exit word.
fn is_plain_operand_text(text: &str) -> bool {
    text.chars()
        .all(|ch| ch.is_alphanumeric() || ch.is_whitespace() || "_.<>=!+-*/%()&:\"'".contains(ch))
        && !text.contains("&&")
        && !invokes_macro(text)
        && !["return", "break", "continue", "async", "move", "await"]
            .iter()
            .any(|word| mentions_identifier(text, word))
}

/// Whether `text` invokes a macro (`name!(`, `name![`, `name!{`, with any
/// whitespace around the `!`): a macro
/// around the owner call may drop its argument or never evaluate it.
fn invokes_macro(text: &str) -> bool {
    // Whitespace (or a masked comment) may sit on either side of the `!`:
    // `stringify ! (..)` and `keep /**/ !(..)` are macro calls too.
    text.match_indices('!').any(|(at, _)| {
        text[..at]
            .trim_end()
            .chars()
            .next_back()
            .is_some_and(|ch| ch.is_alphanumeric() || ch == '_')
            && text[at + 1..].trim_start().starts_with(['(', '[', '{'])
    })
}

/// A call argument that is one simple token: a plain literal, or a name,
/// field or path (`a`, `a.b`, `a::B`), optionally borrowed (`&`, `&mut`).
/// A closure, generic, block, call or operator is not simple, and its commas
/// could shift the argument count.
fn is_simple_call_argument(argument: &str) -> bool {
    let token = argument.trim().trim_start_matches('&').trim_start();
    let token = token.strip_prefix("mut ").unwrap_or(token).trim();
    is_plain_literal_pattern(token)
        || (token.starts_with(|ch: char| ch.is_alphabetic() || ch == '_')
            && token
                .chars()
                .all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '.' || ch == ':'))
}

/// The scrutinee and top-level arms (pattern text, 1-based file line) of an
/// owner whose body is exactly one `match <identifier> { .. }`. Comments,
/// strings and chars are masked while scanning; any shape it cannot read is
/// `None`.
fn whole_body_match_arms(owner: &FunctionSummary) -> Option<(String, Vec<(String, usize)>)> {
    // The structure is read on masked text, so a quote or bracket inside a
    // comment or string cannot hide an arm; pattern text is read from the
    // original at the same offsets (masking keeps the byte layout).
    let original = owner.body.as_str();
    let masked = crate::analysis::extract::mask_comments_and_strings(original);
    let body = masked.as_str();
    let params_open = body.find('(')?;
    let body_open = params_open + body[params_open..].find('{')?;
    let body_close = body.rfind('}')?;
    if body_close <= body_open {
        return None;
    }
    let inner = &body[body_open + 1..body_close];
    let lead = inner.len() - inner.trim_start().len();
    let rest = inner.trim_start().strip_prefix("match ")?;
    let arms_open_rel = rest.find('{')?;
    let scrutinee = rest[..arms_open_rel].trim();
    if scrutinee.is_empty()
        || !scrutinee
            .chars()
            .all(|ch| ch.is_alphanumeric() || ch == '_')
    {
        return None;
    }
    // Absolute offset of the arms block's `{` in `body`.
    let arms_open = body_open + 1 + lead + "match ".len() + arms_open_rel;
    let arms_close = matching_close(body, arms_open)?;
    if !body[arms_close + 1..body_close].trim().is_empty() {
        return None;
    }
    let line_of = |offset: usize| owner.start_line + body[..offset].matches('\n').count();
    let mut arms = Vec::new();
    let mut at = arms_open + 1;
    loop {
        // Skip separators only where the original has them too: a masked
        // string pattern is blank in `body` but is the arm's pattern.
        let separator = |text: &str| text.starts_with(char::is_whitespace) || text.starts_with(',');
        while at < arms_close && separator(&body[at..]) && separator(original.get(at..)?) {
            at += body[at..].chars().next()?.len_utf8();
        }
        if at >= arms_close {
            break;
        }
        let arrow = find_top_level(body, at, arms_close, "=>")?;
        arms.push((original.get(at..arrow)?.to_string(), line_of(at)));
        let mut end = arrow + 2;
        while let Some(ch) = body[end..].chars().next().filter(|ch| ch.is_whitespace()) {
            end += ch.len_utf8();
        }
        end = if body[end..].starts_with('{') {
            matching_close(body, end)? + 1
        } else {
            find_top_level(body, end, arms_close, ",").unwrap_or(arms_close)
        };
        at = end;
    }
    Some((scrutinee.to_string(), arms))
}

/// The offset of the bracket closing the one at `open`, skipping string and
/// char literals.
fn matching_close(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut at = open;
    while at < text.len() {
        at = skip_literal(text, at);
        let ch = text[at..].chars().next()?;
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
        at += ch.len_utf8();
    }
    None
}

/// The first offset in `from..to` where `needle` starts outside any bracket
/// or literal.
fn find_top_level(text: &str, from: usize, to: usize, needle: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut at = from;
    while at < to {
        at = skip_literal(text, at);
        if at >= to {
            return None;
        }
        if depth == 0 && text[at..].starts_with(needle) {
            return Some(at);
        }
        let ch = text[at..].chars().next()?;
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.checked_sub(1)?,
            _ => {}
        }
        at += ch.len_utf8();
    }
    None
}

/// Past a string or char literal starting at `at` (with escapes), else `at`.
/// A lifetime-like `'a` is not a char literal and is left in place.
fn skip_literal(text: &str, at: usize) -> usize {
    let bytes = text.as_bytes();
    match bytes.get(at) {
        Some(b'"') => {
            let mut index = at + 1;
            while index < bytes.len() {
                match bytes[index] {
                    b'\\' => index += 2,
                    b'"' => return skip_literal(text, index + 1),
                    _ => index += 1,
                }
            }
            text.len()
        }
        Some(b'\'') => {
            let closing = if bytes.get(at + 1) == Some(&b'\\') {
                bytes
                    .get(at + 3..)
                    .and_then(|tail| tail.iter().position(|b| *b == b'\''))
                    .map(|p| at + 3 + p)
            } else if bytes.get(at + 2) == Some(&b'\'') {
                Some(at + 2)
            } else {
                None
            };
            closing.map_or(at, |close| skip_literal(text, close + 1))
        }
        _ => at,
    }
}

/// Whether `expression` is a struct literal, not a block, `if`, `match` or loop.
fn is_struct_literal_expression(expression: &str) -> bool {
    let trimmed = expression.trim();
    let trimmed = trimmed.strip_prefix("return ").unwrap_or(trimmed).trim();
    let Some(open) = trimmed.find('{') else {
        return false;
    };
    let path = trimmed[..open].trim_end();
    let name = path.rsplit("::").next().unwrap_or(path);
    trimmed.ends_with('}')
        && !path.is_empty()
        && path
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == ':')
        && name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_uppercase())
}

/// Whether one of `test`'s assertions pins the value of a call to `owner_name`
/// or of a local derived from it: a lone top-level equality (RIPR-SPEC-0197 /
/// RIPR-SPEC-0154 shapes) or, more broadly, `.eq(`, a negated `!=` or a closed
/// `>=`/`<=` range over such a value. The breadth only withholds a start.
fn test_pins_owner_call_by_equality(
    test: &TestSummary,
    owner_name: &str,
    index: &RustIndex,
) -> bool {
    let source = index
        .files()
        .get(&test.file)
        .map(|facts| facts.source.clone());
    let locals = owner_result_locals(&test.body, owner_name);
    assertions_pin_owner_by_equality(
        test.assertions.iter().map(|oracle| oracle.text.as_str()),
        owner_name,
        &locals,
        source.as_deref(),
    )
}

/// Whether some assertion of `test` mentions a call of `owner_name` or a local
/// bound from one. Syntactic and over-approximate: it only decides whether a
/// start may be offered.
fn test_assertion_observes_owner(
    test: &TestSummary,
    owner: &FunctionSummary,
    index: &RustIndex,
) -> bool {
    let calls_owner =
        |text: &str| crate::analysis::classify::text_calls_owner(index, owner, test, text);
    let locals = owner_result_locals_with(&test.body, &calls_owner, true);
    test.assertions.iter().any(|oracle| {
        // Comments and string literals (including `assert!` messages) are not
        // observations.
        let masked = crate::analysis::extract::mask_comments_and_strings(&oracle.text);
        // Failure-message arguments run only when the assertion fails, so a
        // result named only there is not observed by the assertion.
        let operands = assertion_condition_operands(&masked);
        calls_owner(operands)
            || locals
                .iter()
                .any(|local| mentions_identifier(operands, local))
    })
}

/// The leading condition operands of a masked `assert!`, `assert_eq!` or
/// `assert_ne!` (see [`ARM_CALL_ASSERTIONS`]), without the failure-message
/// arguments. Any other text, or text that does not parse, is returned whole.
fn assertion_condition_operands(masked: &str) -> &str {
    let text = masked.trim_start();
    for (name, operands) in ARM_CALL_ASSERTIONS {
        let Some(rest) = text.strip_prefix(name) else {
            continue;
        };
        let Some(rest) = rest.trim_start().strip_prefix('!') else {
            continue;
        };
        let open = text.len() - rest.len() + (rest.len() - rest.trim_start().len());
        if !text[open..].starts_with('(') {
            continue;
        }
        let Some(close) = matching_close(text, open) else {
            return masked;
        };
        let mut end = close;
        let mut from = open + 1;
        for _ in 0..operands {
            match find_top_level(text, from, close, ",") {
                Some(comma) => {
                    end = comma;
                    from = comma + 1;
                }
                None => {
                    end = close;
                    break;
                }
            }
        }
        return &text[open + 1..end];
    }
    masked
}

/// Names bound by a `let` whose initializer calls `owner_name`
/// (`let v = f(2);`, `let (a, b) = f(2);`) or mentions a bound name
/// (`let w = v;`), so a comparison of such a local pins the owner's value as a
/// call does. Reads the masked body. Not followed: a field write, a mutation,
/// a closure, a loop variable or a function parameter.
fn owner_result_locals(body: &str, owner_name: &str) -> Vec<String> {
    owner_result_locals_with(body, &|text| mentions_owner_call(text, owner_name), false)
}

/// [`owner_result_locals`] with the caller's reading of "this initializer
/// calls the owner". With `rebinding_drops` the `let`s are read in order and a
/// later `let` (or bare `let v;`) rebinding a name to a non-owner expression
/// drops it, so a shadowed name grants no observation; the pin reader keeps
/// the additive reading, whose over-approximation only withholds.
/// Scope-blind: an inner-block `let`, `if let` or loop variable is read as a
/// flat rebinding in order.
fn owner_result_locals_with(
    body: &str,
    calls_owner: &dyn Fn(&str) -> bool,
    rebinding_drops: bool,
) -> Vec<String> {
    let masked = crate::analysis::extract::mask_comments_and_strings(body);
    let mut lets: Vec<(Vec<String>, &str)> = Vec::new();
    for statement in masked.split(';') {
        let Some(at) = statement.find("let ") else {
            continue;
        };
        if at > 0
            && statement[..at]
                .chars()
                .next_back()
                .is_some_and(|ch| ch.is_alphanumeric() || ch == '_')
        {
            continue;
        }
        let rest = &statement[at + 4..];
        let (pattern, init) = match rest.split_once('=') {
            Some(split) => split,
            None if rebinding_drops => (rest, ""),
            None => continue,
        };
        // The pattern ends at a type annotation's single `:` (not `::`).
        let bytes = pattern.as_bytes();
        let end = (0..bytes.len())
            .find(|&i| {
                bytes[i] == b':'
                    && bytes.get(i + 1) != Some(&b':')
                    && (i == 0 || bytes[i - 1] != b':')
            })
            .unwrap_or(bytes.len());
        let names: Vec<String> = pattern[..end]
            .split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
            .filter(|word| {
                word.starts_with(|ch: char| ch.is_lowercase() || ch == '_')
                    && *word != "_"
                    && !matches!(*word, "mut" | "ref")
            })
            .map(str::to_string)
            .collect();
        if !names.is_empty() {
            lets.push((names, init));
        }
    }
    let mut locals: Vec<String> = Vec::new();
    if rebinding_drops {
        for (names, init) in &lets {
            if calls_owner(init) || locals.iter().any(|local| mentions_identifier(init, local)) {
                for name in names {
                    if !locals.contains(name) {
                        locals.push(name.clone());
                    }
                }
            } else {
                locals.retain(|local| !names.contains(local));
            }
        }
        return locals;
    }
    loop {
        let before = locals.len();
        for (names, init) in &lets {
            if calls_owner(init) || locals.iter().any(|local| mentions_identifier(init, local)) {
                for name in names {
                    if !locals.contains(name) {
                        locals.push(name.clone());
                    }
                }
            }
        }
        if locals.len() == before {
            return locals;
        }
    }
}

/// Whether `text` spells `path` (`E::Bad`, `Bad`) with no identifier
/// character on either side, so `E::B` is not found in `E::Bad`. A leading
/// `::` is allowed (`crate::E::B`).
fn mentions_path(text: &str, path: &str) -> bool {
    let part = |ch: Option<char>| ch.is_some_and(|ch| ch.is_alphanumeric() || ch == '_');
    !path.is_empty()
        && text.match_indices(path).any(|(at, _)| {
            !part(text[..at].chars().next_back()) && !part(text[at + path.len()..].chars().next())
        })
}

/// Whether `text` calls `name` as a whole identifier: no identifier character
/// before it (a path or method receiver is fine) and `(` or a turbofish after
/// it, so `undbl(` and `dbl_x(` are not calls of `dbl`.
fn mentions_owner_call(text: &str, name: &str) -> bool {
    let part = |ch: Option<char>| ch.is_some_and(|ch| ch.is_alphanumeric() || ch == '_');
    !name.is_empty()
        && text.match_indices(name).any(|(at, _)| {
            let after = text[at + name.len()..].trim_start();
            !part(text[..at].chars().next_back())
                && (after.starts_with('(') || after.starts_with("::<"))
        })
}

/// Whether `operand` mentions `name` as a whole identifier.
fn mentions_identifier(operand: &str, name: &str) -> bool {
    operand.match_indices(name).any(|(at, _)| {
        let before = operand[..at].chars().next_back();
        let after = operand[at + name.len()..].chars().next();
        let part = |ch: Option<char>| ch.is_some_and(|ch| ch.is_alphanumeric() || ch == '_');
        !part(before) && !part(after)
    })
}

/// The pure core of [`test_pins_owner_call_by_equality`]. A terminal
/// Err-return guard is a pin only through the prelude `Err`: a file that
/// declares `Err` as a value (`fn Err`, `const Err`, ...) makes the guard an
/// ordinary statement (RIPR-SPEC-0197), read by the same authority the
/// owner-pin credit uses. A file fact that cannot be read withholds: the
/// guard is kept as a pin, so no start is offered on unknown ground.
fn assertions_pin_owner_by_equality<'a>(
    assertions: impl IntoIterator<Item = &'a str>,
    owner_name: &str,
    locals: &[String],
    source: Option<&str>,
) -> bool {
    let call = format!("{owner_name}(");
    // Evaluated only when a guard assertion is seen; an unreadable file
    // (`None`) is not shadowed, so the guard stays a pin and withholds.
    let err_shadowed = || {
        source.is_some_and(|source| {
            crate::analysis::syntax::source_declares_value_name(source, "Err")
        })
    };
    let mentions =
        |text: &str| text.contains(&call) || locals.iter().any(|l| mentions_identifier(text, l));
    assertions.into_iter().any(|text| {
        let guard = crate::analysis::extract::err_return_guard_twin(text).is_some();
        if guard && err_shadowed() {
            return false;
        }
        crate::analysis::classify::equality_condition_operands(text)
            .is_some_and(|operands| operands.iter().any(|operand| mentions(operand)))
            || (!guard && comparison_pins_value(text, &mentions))
    })
}

/// A broader, syntactic read of an assertion that pins a value derived from
/// the owner call without a lone top-level `==`: `x.eq(&6)`, a negated
/// inequality `!(x != 6)`, or a closed range `x >= 6 && x <= 6`. A plain
/// `!=`, `.ne(`, `assert_ne!`, a one-sided bound, a comparison nested in
/// another expression (`f(x) != 0 == true`) or any `||` is a weak check and
/// not a pin. Over-reading these shapes only withholds a repair start.
fn comparison_pins_value(text: &str, mentions: &dyn Fn(&str) -> bool) -> bool {
    let masked = crate::analysis::extract::mask_comments_and_strings(text);
    if !mentions(&masked) || masked.contains("||") {
        return false;
    }
    masked.contains(".eq(")
        || (masked.contains("!=")
            && masked.match_indices("!(").any(|(at, _)| {
                // `!(` after an identifier is a macro bang (`assert!(`), not a negation.
                !masked[..at]
                    .chars()
                    .next_back()
                    .is_some_and(|ch| ch.is_alphanumeric() || ch == '_')
            }))
        || (masked.contains(">=") && masked.contains("<=") && masked.contains("&&"))
}

#[cfg(test)]
mod equality_pin_tests {
    use super::{mentions_identifier, owner_result_locals};

    fn pins<'a>(
        assertions: impl IntoIterator<Item = &'a str>,
        owner_name: &str,
        source: Option<&str>,
    ) -> bool {
        super::assertions_pin_owner_by_equality(assertions, owner_name, &[], source)
    }

    #[test]
    fn an_equality_on_a_local_bound_from_the_owner_call_pins() {
        let body = "let v = weight(4);\n    assert!(v == 12);";
        let locals = owner_result_locals(body, "weight");
        assert_eq!(locals, ["v"]);
        let pinned = |assertion: &str, locals: &[String]| {
            super::assertions_pin_owner_by_equality([assertion], "weight", locals, Some(""))
        };
        assert!(pinned("assert!(v == 12);", &locals));
        assert!(!pinned("assert!(v > 0);", &locals), "weak stays unpinned");
        assert!(!pinned("assert!(vv == 12);", &locals), "whole identifiers");
        assert!(!pinned("assert!(v == 12);", &[]), "no local, no pin");
    }

    #[test]
    fn assertion_condition_operands_exclude_the_failure_message() {
        use super::assertion_condition_operands as operands;
        assert_eq!(operands("assert!(r < 100, \"  \", r);"), "r < 100");
        assert_eq!(operands("assert!(f(a, b) < 3);"), "f(a, b) < 3");
        assert_eq!(operands("assert_eq!(f(a), r, \"\", w);"), "f(a), r");
        assert_eq!(operands("assert_ne!(x, y);"), "x, y");
        assert_eq!(operands("check(r, w);"), "check(r, w);");
    }

    #[test]
    fn derived_locals_and_destructures_are_followed_to_a_fixed_point() {
        let locals = |body: &str| owner_result_locals(body, "weight");
        assert_eq!(locals("let v = weight(4); let w = v;"), ["v", "w"]);
        assert_eq!(locals("let w = v; let v = weight(4);"), ["v", "w"]);
        assert_eq!(locals("let (a, mut b) = weight(4);"), ["a", "b"]);
        assert_eq!(locals("let Some(x) = weight(4) else { return };"), ["x"]);
        assert!(locals("let w = v; let u = other(4);").is_empty());
    }

    #[test]
    fn broader_comparison_shapes_pin_but_a_weak_bound_does_not() {
        let locals = ["v".to_string()];
        let pins = |assertion: &str| {
            super::assertions_pin_owner_by_equality([assertion], "weight", &locals, Some(""))
        };
        for pinned in [
            "assert!(v.eq(&6));",
            "assert!(!(v != 6));",
            "assert!(v >= 6 && v <= 6);",
            "assert!(weight(4).eq(&6));",
        ] {
            assert!(pins(pinned), "{pinned}");
        }
        for weak in [
            "assert!(v > 0);",
            "assert!(v >= 1);",
            "assert!(v < 100);",
            "assert!(other == 6);",
            "assert!(w >= 6 && w <= 6);",
            "assert!(v > 0, \"v == 6\");",
            "assert!(v != 0);",
            "assert!(v.ne(&6));",
            "assert_ne!(v, 0);",
            "assert_eq!(v != 0, true);",
            "assert!(v.eq(&6) || lenient);",
            "assert!(v >= 6 || v <= 6);",
        ] {
            assert!(!pins(weak), "{weak}");
        }
    }

    #[test]
    fn variant_paths_match_whole_identifiers_only() {
        use super::mentions_path as mentions;
        assert!(mentions("matches!(r, Err(E::Bad))", "E::Bad"));
        assert!(mentions("matches!(r, Err(crate::E::Bad))", "E::Bad"));
        assert!(mentions("matches!(r, Err(Bad))", "Bad"));
        assert!(!mentions("matches!(r, Err(E::Bad))", "E::B"));
        assert!(!mentions("matches!(r, Err(E::Bad))", "B"));
        assert!(!mentions("matches!(r, Err(E::B))", "E::Bad"));
        assert!(!mentions("matches!(r, Err(XE::B))", "E::B"));
    }

    #[test]
    fn type_aliases_are_read_as_tokens_not_substrings() {
        use super::source_declares_type_alias as declares;
        for spelling in [
            "type Out = Loud;",
            "pub type Out = Loud;",
            "pub(crate) type Out<T> = Loud<T>;",
            "type\nOut = Loud;",
            "type   /* c */\n\tOut\n= Loud;",
            "impl X for Y {\n    type Out = Loud;\n}",
        ] {
            assert!(declares(spelling, "Out"), "{spelling}");
        }
        for other in [
            "type Output = Loud;",
            "type Outer = Loud;",
            "// type Out = Loud;",
            "const S: &str = \"type Out\";",
            "fn mytype Out() {}",
            "let t: Out = Loud;",
        ] {
            assert!(!declares(other, "Out"), "{other}");
        }
    }

    #[test]
    fn locals_come_only_from_lets_that_call_the_owner() {
        let locals = |body: &str| owner_result_locals(body, "weight");
        assert_eq!(locals("let mut v: u32 = weight(4);"), ["v"]);
        assert_eq!(locals("let r = weight(4).unwrap();"), ["r"]);
        assert!(locals("let v = other(4);").is_empty());
        assert!(locals("let v = 4; assert!(weight(v) > 0);").is_empty());
        assert!(locals("// let v = weight(4);").is_empty());
        assert!(mentions_identifier("v + 1", "v"));
        assert!(!mentions_identifier("value + 1", "v"));
    }

    const GUARD: &str = "if weight(4) != 12 { return Err(\"mismatch\"); }";
    const PLAIN: &str = "assert!(weight(4) == 12);";

    #[test]
    fn plain_and_guard_equalities_pin_the_owner() {
        assert!(pins([PLAIN], "weight", Some("fn t() {}")));
        assert!(pins([GUARD], "weight", Some("fn t() {}")));
        assert!(
            pins([GUARD], "weight", None),
            "an unreadable file withholds"
        );
    }

    #[test]
    fn a_declared_value_err_unpins_only_the_guard() {
        let shadowed = "fn Err(_: &str) -> Result<(), String> { Ok(()) }";
        assert!(!pins([GUARD], "weight", Some(shadowed)));
        assert!(pins([PLAIN], "weight", Some(shadowed)));
    }

    #[test]
    fn lookalike_names_comments_and_strings_do_not_shadow_err() {
        for source in [
            "fn Error_case() {}",
            "fn t() { let Errors = 1; }",
            "// fn Err(_: &str) {}",
            "const S: &str = \"fn Err\";",
        ] {
            assert!(pins([GUARD], "weight", Some(source)), "{source}");
        }
    }

    #[test]
    fn struct_literals_are_told_from_blocks_and_calls() {
        use super::is_struct_literal_expression as lit;
        assert!(lit("Config {\n    a: 1,\n    b,\n}"));
        assert!(lit("return crate::Config { a: 1 }"));
        assert!(!lit("if a { 1 } else { 2 }"));
        assert!(!lit("match a { _ => 1 }"));
        assert!(!lit("{ 1 }"));
        assert!(!lit("x * 3"));
        assert!(!lit("make(Config { a: 1 })"));
    }

    #[test]
    fn other_owner_calls_and_non_equalities_do_not_pin() {
        assert!(!pins([PLAIN], "other", Some("")));
        assert!(!pins(["assert!(weight(4) > 0);"], "weight", Some("")));
    }
}

fn new_test_target_admission(
    seam: &RepoSeam,
    context: &CompactGripContext<'_>,
) -> Option<NewTestTargetAdmission> {
    match seam.kind() {
        SeamKind::PredicateBoundary
        | SeamKind::ErrorVariant
        | SeamKind::ReturnValue
        | SeamKind::FieldConstruction
        | SeamKind::MatchArm => Some(new_test_target::admit_new_test_target(
            seam,
            context.index,
            context.owner_function(seam.file(), seam.display_line()),
            &context.inline_unit_layouts,
        )),
        SeamKind::SideEffect | SeamKind::CallPresence => None,
    }
}

/// Build compact evidence for a single seam. The returned
/// `TestGripEvidence` preserves the stage states used by classification,
/// but intentionally omits related-test detail and observed-value
/// payloads because repo badges only need per-class counts.
pub(crate) fn compact_evidence_for_seam(
    seam: &RepoSeam,
    context: &CompactGripContext<'_>,
) -> TestGripEvidence {
    let ranked = find_related_tests_ranked(seam, context);
    let related_reasons: Vec<(&TestSummary, RelationReason)> = ranked
        .iter()
        .map(|(indexed, reason)| (indexed.test, *reason))
        .collect();
    let related_indexed: Vec<&CompactTest<'_>> = ranked
        .iter()
        .take(COMPACT_RELATED_TEST_LIMIT)
        .map(|(indexed, _reason)| *indexed)
        .collect();
    let related: Vec<&TestSummary> = related_indexed.iter().map(|indexed| indexed.test).collect();
    let owner_fn = context.owner_function(seam.file(), seam.display_line());

    let reach = reach_evidence(seam, &related_reasons, owner_fn, context);
    let (activate, missing_discriminators) =
        compact_activate_evidence(seam, &related_indexed, context, owner_fn);
    let pin_syntax = context.pin_syntax();
    let limited_reasons: Vec<(&TestSummary, RelationReason)> = ranked
        .iter()
        .take(COMPACT_RELATED_TEST_LIMIT)
        .map(|(indexed, reason)| (indexed.test, *reason))
        .collect();
    let credited_views = credited_related_tests(seam, context, owner_fn, &limited_reasons);
    let credited: Vec<&TestSummary> = credited_views.iter().map(AsRef::as_ref).collect();
    let propagate = propagate_evidence(seam, &credited, owner_fn, context.index, pin_syntax);
    let observe = observe_evidence(&credited);
    let discriminate = discriminate_evidence(seam, &credited, owner_fn, context.index, pin_syntax);
    let statically_contradicted_related_tests = related
        .iter()
        .filter(|test| test_has_contradicted_assertion(test, owner_fn))
        .count();

    let mut evidence = TestGripEvidence {
        seam_id: seam.id().clone(),
        related_tests: Vec::new(),
        reach,
        activate,
        propagate,
        observe,
        discriminate,
        observed_values: Vec::new(),
        missing_discriminators,
        statically_contradicted_related_tests,
        new_test_target: None,
    };
    // #8600: read over every ranked related test, before the compact cap.
    let saturated = call_budget::any_saturated(ranked.iter().map(|(indexed, _)| indexed.test));
    call_budget::withhold(&mut evidence, saturated);
    // #10703: the same named limit `check` applies to a `#[should_panic]`
    // test whose panic may be the changed code's.
    let panicking = panic_outcome::deciding_tests(
        ranked
            .iter()
            .filter(|(_, reason)| *reason == RelationReason::DirectOwnerCall)
            .map(|(indexed, _)| {
                (
                    indexed.test.name.as_str(),
                    indexed.test.file.as_path(),
                    indexed.test.start_line,
                )
            }),
        owner_fn,
        seam.display_line(),
        context.index,
    );
    panic_outcome::withhold(seam, &mut evidence, &panicking);
    evidence
}

/// Reach evidence: does a related test call the seam's owner?
///
/// Only relations that tie a test to running the owner count
/// ([`relation_establishes_reach`], the rule diff-mode reach uses). A test
/// related only by sharing the owner's file or module, by asserting a token
/// the seam's expression names, by naming the owner in its own name or an
/// import path, or by calling a fixture-shaped helper may never run the
/// owner: crediting it let an uncalled `as_kb` borrow a sibling test's
/// exact-value oracle and read `reach: yes` (#5335). When every related
/// test is such a relation, the shared diff-mode reach decides: `weak` when
/// something could still run the owner unseen (a doctest or caller names
/// it, a trait impl, a test macro), otherwise `no`. A transitive (bounded
/// or past five hops), macro or trait-dispatch witness from repo reach keeps
/// it `weak` too.
/// Either way the seam can no longer read `strongly_gripped`; the related
/// tests stay listed as suggestions.
fn reach_evidence(
    seam: &RepoSeam,
    related: &[(&TestSummary, RelationReason)],
    owner_fn: Option<&FunctionSummary>,
    context: &CompactGripContext<'_>,
) -> StageEvidence {
    if let Some(summary) = reach_limit::outside_module_tree_summary(seam, context) {
        return StageEvidence::new(StageState::Opaque, Confidence::Low, summary);
    }
    if related.is_empty() {
        return reach_limit::reach_without_related_tests(seam.owner(), owner_fn, context);
    }
    let reaches = related
        .iter()
        .any(|(_, reason)| relation_establishes_reach(*reason, owner_fn.is_some()));
    if !reaches {
        let shared = classify_reach_evidence(related, owner_fn, || {
            owner_fn.is_none_or(|owner| context.unseen_reach_names().may_reach_unseen(owner))
        });
        if shared.state != StageState::No {
            return shared;
        }
        let unreached = reach_limit::reach_without_related_tests(seam.owner(), owner_fn, context);
        if unreached.state == StageState::No {
            return shared;
        }
        // An unresolved path from a test toward the owner exists, so these
        // nearby tests may run it after all: weak reach, never full reach.
        return StageEvidence::new(
            StageState::Weak,
            Confidence::Low,
            format!("{} {}", shared.summary, unreached.summary),
        );
    }
    // Name only the tests whose relation establishes reach, in ranked
    // order: a proximity or affinity test ranked ahead of the reaching one
    // must not be named as reaching the owner (#5335 review round 2).
    let names: Vec<&str> = related
        .iter()
        .filter(|(_, reason)| relation_establishes_reach(*reason, owner_fn.is_some()))
        .take(3)
        .map(|(test, _)| test.name.as_str())
        .collect();
    StageEvidence::new(
        StageState::Yes,
        Confidence::Medium,
        format!(
            "Related tests appear to reach `{}`: {}",
            seam.owner(),
            names.join(", ")
        ),
    )
}

/// Activation evidence.
///
/// Returns `(stage, observed_values, missing_discriminators)`. The
/// observed values come from the seam's owner-call argument lists
/// across all related tests. The missing-discriminator set is the
/// per-kind required value or shape minus what we observed.
fn activate_evidence(
    seam: &RepoSeam,
    related: &[&CompactTest<'_>],
    context: &CompactGripContext<'_>,
    owner_fn: Option<&FunctionSummary>,
) -> (StageEvidence, Vec<ValueFact>, Vec<MissingDiscriminatorFact>) {
    let index = context.index;
    let owner_name = owner_fn.map(|f| f.name.as_str()).unwrap_or("");
    let mut observed: Vec<ValueFact> = Vec::new();
    // #7007: a related test whose equality assertion statically contradicts
    // the owner's folded behavior fails at baseline and pins the mutant's
    // value, so its call-site values and boundary-coverage arguments
    // describe the inverted repair attempt, not the seam's activation
    // contract. Its activation credit is withheld here the way #6026
    // already withholds its oracle credit; a consistent or not-evaluable
    // test keeps today's credit, so the fold can only withhold.
    let activation_credited: Vec<&CompactTest<'_>> = related
        .iter()
        .copied()
        .filter(|indexed| !test_has_contradicted_assertion(indexed.test, owner_fn))
        .collect();
    let withheld_activation_tests = related.len() - activation_credited.len();
    let observed_argument_selection = owner_fn
        .filter(|owner| !owner.name.is_empty())
        .map(|owner| observed_argument_selection(seam, owner));

    if let (Some(selection), Some(owner_fn)) = (&observed_argument_selection, owner_fn) {
        for indexed in activation_credited.iter() {
            observed.extend(observed_value_facts_for_test(
                seam, indexed, context, owner_fn, selection,
            ));
        }
    }
    sort_value_facts(&mut observed);

    let field_assignment_value_unresolved = observed.is_empty()
        && observed_argument_selection
            .as_ref()
            .zip(owner_fn)
            .is_some_and(|(selection, owner_fn)| {
                related.iter().any(|indexed| {
                    field_assignment_value_unresolved_for_test(
                        seam, indexed, index, owner_fn, selection,
                    )
                })
            });

    // A boundary operand naming a constant (`amount >= DISCOUNT_THRESHOLD`)
    // is compared through the shared named-constant lookup: a visible
    // same-file integer value matches observed values, and a test argument
    // naming the constant itself matches by identity.
    let boundary_constant =
        owner_fn.and_then(|owner_fn| boundary_constant_operand(seam, index, owner_fn));
    let boundary_constant_observed = boundary_constant.as_ref().is_some_and(|constant| {
        constant.lookup.value().is_some_and(|value| {
            observed
                .iter()
                .any(|fact| comparable_value(&fact.value) == comparable_value(value))
        }) || (constant.lookup.is_declared_once()
            && activation_credited
                .iter()
                .any(|indexed| test_passes_boundary_constant(indexed, owner_fn, index, constant)))
    });
    let boundary_equality_observed = boundary_constant_observed
        || owner_fn.is_some_and(|owner_fn| {
            seam.kind() == SeamKind::PredicateBoundary
                && activation_credited.iter().any(|indexed| {
                    boundary_equality_overlap_score(seam, indexed, context, owner_fn) > 0
                })
        });
    // A constant whose value ripr cannot see and whose declaration it
    // cannot pin to the owner's file can never be matched by a test, so
    // it routes to the unresolved-operand limitation instead of an
    // actionable "add the missing discriminator" packet.
    let boundary_constant_unresolved = !boundary_equality_observed
        && boundary_constant
            .as_ref()
            .is_some_and(|constant| !constant.lookup.is_declared_once());
    // A local boundary with no input-free value (#4228) is the same
    // dead end: `check` drops its repair ask, so grip must not grade it.
    let boundary = owner_fn.and_then(|owner_fn| predicate_boundary_operand(seam, owner_fn));
    let boundary_local_unresolved = !boundary_equality_observed
        && boundary
            .as_ref()
            .is_some_and(PredicateBoundaryOperand::local_unresolved);
    let boundary_activation_operands_unresolved = boundary_constant_unresolved
        || boundary_local_unresolved
        || observed_argument_selection
            .as_ref()
            .is_some_and(|selection| {
                matches!(
                    selection,
                    ObservedArgumentSelection::UnresolvedBoundaryOperands
                ) || (selection.requires_projection()
                    && observed.is_empty()
                    && !boundary_equality_observed)
            });
    let mut missing = missing_discriminators_for(
        seam,
        &observed,
        boundary_activation_operands_unresolved,
        boundary_equality_observed,
        boundary_constant.as_ref(),
        boundary.as_ref(),
    );
    let direct_value_insensitive_owner_call = !owner_name.is_empty()
        && !requires_concrete_activation_values(seam)
        && owner_fn.is_some_and(|owner_fn| {
            activation_credited
                .iter()
                .any(|indexed| has_direct_owner_call(indexed, owner_fn))
        });
    let target_affinity_tokens =
        (!requires_concrete_activation_values(seam)).then(|| assertion_target_tokens(seam));
    let helper_value_insensitive_owner_call = !owner_name.is_empty()
        && !requires_concrete_activation_values(seam)
        && owner_fn.is_some_and(|owner_fn| {
            let free_owner = owner_fn.impl_context == FunctionImplContext::Free;
            activation_credited.iter().any(|indexed| {
                if free_owner {
                    indexed
                        .free_spelling_helper_owner_call_names
                        .contains(owner_name)
                        || (indexed
                            .free_spelling_target_affinity_owner_call_names
                            .contains(owner_name)
                            && has_owner_call_via_target_affinity(
                                indexed,
                                owner_name,
                                target_affinity_tokens.as_ref(),
                            ))
                } else {
                    has_owner_call_via_one_hop_helper(indexed, owner_name)
                        || has_owner_call_via_target_affinity(
                            indexed,
                            owner_name,
                            target_affinity_tokens.as_ref(),
                        )
                }
            })
        });
    let ambiguous_constructor_field_owner = !owner_name.is_empty()
        && related
            .iter()
            .any(|indexed| has_ambiguous_constructor_field_owner(indexed, seam, owner_name));

    let state = if related.is_empty() {
        StageState::No
    } else if ambiguous_constructor_field_owner
        || boundary_constant_unresolved
        || boundary_local_unresolved
    {
        StageState::Unknown
    } else if !observed.is_empty()
        || direct_value_insensitive_owner_call
        || helper_value_insensitive_owner_call
    {
        StageState::Yes
    } else {
        // Reach exists but no concrete value seen — most often a helper
        // call that hides the activation, or an integration test.
        StageState::Unknown
    };
    let stage = StageEvidence::new(
        state,
        if !boundary_constant_unresolved
            && !boundary_local_unresolved
            && (!observed.is_empty()
                || direct_value_insensitive_owner_call
                || helper_value_insensitive_owner_call)
        {
            Confidence::Medium
        } else {
            Confidence::Low
        },
        if ambiguous_constructor_field_owner {
            format!(
                "constructor_field_owner_ambiguous: exact field observer found, but same-crate caller linkage to owner `{owner_name}` is ambiguous"
            )
        } else if boundary_constant_unresolved && !related.is_empty() {
            let constant = boundary_constant.as_ref().map_or(("", ""), |constant| {
                (constant.name.as_str(), constant.lookup.limitation())
            });
            format!(
                "Boundary constant `{}` has no statically visible value for seam `{}` ({}); ripr cannot tell whether a related test hits the equality boundary, so it does not emit an actionable repair packet",
                constant.0,
                seam.expression()
                    .lines()
                    .next()
                    .unwrap_or(seam.expression()),
                constant.1
            )
        } else if boundary_local_unresolved && !related.is_empty() {
            boundary_activation_operands_unresolved_summary(seam, owner_fn)
        } else if !observed.is_empty() {
            format!(
                "Observed {} concrete activation value(s) for seam `{}`",
                observed.len(),
                seam.expression()
                    .lines()
                    .next()
                    .unwrap_or(seam.expression())
            )
        } else if direct_value_insensitive_owner_call {
            format!(
                "Observed direct owner call for value-insensitive seam `{}`",
                seam.expression()
                    .lines()
                    .next()
                    .unwrap_or(seam.expression())
            )
        } else if helper_value_insensitive_owner_call {
            format!(
                "Observed helper owner call for value-insensitive seam `{}`",
                seam.expression()
                    .lines()
                    .next()
                    .unwrap_or(seam.expression())
            )
        } else if field_assignment_value_unresolved {
            format!(
                "Field assignment value is unresolved for seam `{}`; direct field writes only support unconditional source-ordered assignments of literals and same-file literal constants with bounded +/- integer offsets and no intervening mutable borrow",
                seam.expression()
                    .lines()
                    .next()
                    .unwrap_or(seam.expression())
            )
        } else if boundary_activation_operands_unresolved && !related.is_empty() {
            boundary_activation_operands_unresolved_summary(seam, owner_fn)
        } else if withheld_activation_tests > 0 {
            // Every credited activation path is empty while at least one
            // related test was withheld, so the stage discloses the
            // withholding instead of reading as if nothing ran (#7007).
            format!(
                "Activation credit withheld from {withheld_activation_tests} related test(s) for seam `{}`: their equality assertion statically contradicts the owner's folded behavior",
                seam.expression()
                    .lines()
                    .next()
                    .unwrap_or(seam.expression())
            )
        } else if requires_concrete_activation_values(seam) {
            format!(
                "No concrete activation values observed for seam `{}`",
                seam.expression()
                    .lines()
                    .next()
                    .unwrap_or(seam.expression())
            )
        } else {
            format!(
                "No direct owner call observed for value-insensitive seam `{}`",
                seam.expression()
                    .lines()
                    .next()
                    .unwrap_or(seam.expression())
            )
        },
    );
    missing.extend(owner_result_binding::missing_field_value_facts(
        seam,
        related,
        context,
        owner_fn,
        &stage.state,
    ));
    (stage, observed, missing)
}

fn requires_concrete_activation_values(seam: &RepoSeam) -> bool {
    seam.kind() == SeamKind::PredicateBoundary && comparison_operands(seam.expression()).is_some()
}

enum ObservedArgumentSelection {
    AllArguments,
    ArgumentOperands(Vec<ObservedArgumentOperand>),
    UnresolvedBoundaryOperands,
}

impl ObservedArgumentSelection {
    fn requires_projection(&self) -> bool {
        matches!(
            self,
            ObservedArgumentSelection::ArgumentOperands(operands)
                if operands
                    .iter()
                    .any(|operand| operand.projection.is_some())
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ObservedArgumentOperand {
    index: usize,
    projection: Option<String>,
}

fn observed_value_facts_for_test(
    seam: &RepoSeam,
    indexed: &CompactTest<'_>,
    context: &CompactGripContext<'_>,
    owner_fn: &FunctionSummary,
    observed_argument_selection: &ObservedArgumentSelection,
) -> Vec<ValueFact> {
    let index = context.index;
    let owner_name = owner_fn.name.as_str();
    let mut observed: Vec<ValueFact> = Vec::new();
    if matches!(
        observed_argument_selection,
        ObservedArgumentSelection::UnresolvedBoundaryOperands
    ) {
        return observed;
    }
    // Per-test resolution facts (let bindings, rstest cases, table
    // rows, same-file consts) are built lazily and then reused across
    // all owner calls in this test. Per `analysis/value-extraction-v2`.
    let value_facts = indexed.value_facts(index);
    let env = super::value_resolution::ValueEnv::new(seam, value_facts);
    for call in indexed.test.body_calls() {
        if call.name != owner_name {
            continue;
        }
        let Some(args) =
            owner_call_text(&call.text, owner_fn).and_then(|text| call_arguments(text, owner_name))
        else {
            continue;
        };
        for (arg_index, arg) in args.into_iter().enumerate() {
            let argument_operand = match observed_argument_selection {
                ObservedArgumentSelection::ArgumentOperands(operands) => operands
                    .iter()
                    .find(|operand| operand.index == arg_index)
                    .cloned(),
                ObservedArgumentSelection::AllArguments => Some(ObservedArgumentOperand {
                    index: arg_index,
                    projection: None,
                }),
                ObservedArgumentSelection::UnresolvedBoundaryOperands => continue,
            };
            let Some(argument_operand) = argument_operand else {
                continue;
            };
            let arg = projected_argument_expression(&arg, argument_operand.projection.as_deref());
            if suffixed_decimal_integer_value(&arg).is_some()
                && !super::classify::bound_free_owner_calls(
                    index,
                    owner_fn,
                    indexed.test,
                    &call.text,
                    context.pin_syntax(),
                )
            {
                continue;
            }
            let mut emitted = false;
            // Direct literal first (matches pre-v2 behavior).
            for value in scalar_values(&arg) {
                observed.push(ValueFact {
                    file: None,
                    line: call.line,
                    text: call.text.to_string(),
                    value,
                    context: ValueContext::FunctionArgument,
                });
                emitted = true;
            }
            if emitted {
                continue;
            }
            // value-extraction-v2: try to resolve the arg through the
            // priority chain (let / rstest case / table row /
            // same-file const / Some/Ok/Err).
            for (value, context) in env.resolve_at_call(&arg, call.line, &call.name, &call.text) {
                observed.push(ValueFact {
                    file: None,
                    line: call.line,
                    text: call.text.to_string(),
                    value,
                    context,
                });
            }
        }
    }
    // Builder-method values (e.g.,
    // `Quote::new().amount(100).threshold(100)`) - collected
    // separately because they don't fit the per-arg shape. These only
    // count when method names align with seam tokens; the env enforces
    // that filter.
    observed.extend(env.builder_facts(&indexed.test.body));
    for fact in &mut observed {
        fact.file = Some(indexed.test.file.clone());
    }
    observed
}

fn field_assignment_value_unresolved_for_test(
    seam: &RepoSeam,
    indexed: &CompactTest<'_>,
    index: &RustIndex,
    owner_fn: &FunctionSummary,
    selection: &ObservedArgumentSelection,
) -> bool {
    let owner_name = owner_fn.name.as_str();
    let ObservedArgumentSelection::ArgumentOperands(operands) = selection else {
        return false;
    };
    let value_facts = indexed.value_facts(index);
    let env = super::value_resolution::ValueEnv::new(seam, value_facts);
    indexed.test.calls.iter().any(|call| {
        if call.name != owner_name {
            return false;
        }
        let Some(args) =
            owner_call_text(&call.text, owner_fn).and_then(|text| call_arguments(text, owner_name))
        else {
            return false;
        };
        operands.iter().any(|operand| {
            let Some(arg) = args.get(operand.index) else {
                return false;
            };
            let arg = projected_argument_expression(arg, operand.projection.as_deref());
            env.field_assignment_value_unresolved_at_call(&arg, call.line, &call.name, &call.text)
        })
    })
}

fn has_direct_owner_call(indexed: &CompactTest<'_>, owner_fn: &FunctionSummary) -> bool {
    indexed.test.calls.iter().any(|call| {
        call_may_reach_owner(call, owner_fn)
            && call_text_contains_named_call(&call.text, &owner_fn.name)
    })
}

/// Whether a captured call named like the owner can be a call of it. A free
/// owner is reached only through a bare or module-qualified spelling:
/// `ByteSize::kb(100)` and `size.kb()` call something else, so their
/// arguments are not the free `kb`'s activation values (#6713).
fn call_may_reach_owner(call: &CallFact, owner_fn: &FunctionSummary) -> bool {
    call.name == owner_fn.name
        && (owner_fn.impl_context != FunctionImplContext::Free
            || call_text_may_call_free_function(&call.text, &owner_fn.name))
}

fn has_owner_call_via_one_hop_helper(indexed: &CompactTest<'_>, owner_name: &str) -> bool {
    indexed.helper_owner_call_names.contains(owner_name)
}

fn has_owner_call_via_target_affinity(
    indexed: &CompactTest<'_>,
    owner_name: &str,
    target_tokens: Option<&BTreeSet<String>>,
) -> bool {
    indexed
        .target_affinity_owner_call_names
        .contains(owner_name)
        && target_tokens
            .is_some_and(|tokens| test_assertion_mentions_any_target_token(indexed, tokens))
}

fn has_ambiguous_constructor_field_owner(
    indexed: &CompactTest<'_>,
    seam: &RepoSeam,
    owner_name: &str,
) -> bool {
    seam.kind() == SeamKind::FieldConstruction
        && indexed
            .ambiguous_target_affinity_owner_call_names
            .contains(owner_name)
        && indexed
            .test
            .assertions
            .iter()
            .any(|oracle| field_construction_oracle_matches_seam_field(seam, &oracle.text))
}

/// `owner_fn` is the seam's resolved owner.
fn observed_argument_selection(
    seam: &RepoSeam,
    owner_fn: &FunctionSummary,
) -> ObservedArgumentSelection {
    if seam.kind() != SeamKind::PredicateBoundary {
        return ObservedArgumentSelection::AllArguments;
    }
    let Some((left, right)) = comparison_operands(seam.expression()) else {
        return ObservedArgumentSelection::AllArguments;
    };
    let parameters = function_parameters(owner_fn);
    if let Some(left_operand) = boundary_operand_argument(owner_fn, &parameters, &left) {
        return ObservedArgumentSelection::ArgumentOperands(vec![left_operand]);
    }
    if let Some(right_operand) = boundary_operand_argument(owner_fn, &parameters, &right)
        && left_is_boundary_value(owner_fn, &left, seam.display_line())
    {
        return ObservedArgumentSelection::ArgumentOperands(vec![right_operand]);
    }
    ObservedArgumentSelection::UnresolvedBoundaryOperands
}

fn boundary_activation_operands_unresolved_summary(
    seam: &RepoSeam,
    owner_fn: Option<&FunctionSummary>,
) -> String {
    let expression = seam
        .expression()
        .lines()
        .next()
        .unwrap_or(seam.expression());
    if boundary_activation_operands_are_iterator_derived(seam, owner_fn) {
        format!(
            "Boundary activation operand is iterator-derived for seam `{expression}`; add analyzer support for iterator boundary operand resolution before emitting an actionable repair packet"
        )
    } else if boundary_activation_operands_are_closure_derived(seam, owner_fn) {
        format!(
            "Boundary activation operand is closure-derived for seam `{expression}`; add analyzer support for closure boundary operand resolution before emitting an actionable repair packet"
        )
    } else {
        format!(
            "Boundary activation operands are local or computed for seam `{expression}`; add analyzer support for local/computed boundary operand resolution before emitting an actionable repair packet"
        )
    }
}

fn boundary_activation_operands_are_iterator_derived(
    seam: &RepoSeam,
    owner_fn: Option<&FunctionSummary>,
) -> bool {
    if seam.kind() != SeamKind::PredicateBoundary {
        return false;
    }
    let Some(owner_fn) = owner_fn else {
        return false;
    };
    let Some((left, right)) = comparison_operands(seam.expression()) else {
        return false;
    };
    boundary_operand_is_iterator_derived(owner_fn, &left)
        || boundary_operand_is_iterator_derived(owner_fn, &right)
}

fn boundary_activation_operands_are_closure_derived(
    seam: &RepoSeam,
    owner_fn: Option<&FunctionSummary>,
) -> bool {
    if seam.kind() != SeamKind::PredicateBoundary {
        return false;
    }
    let Some(owner_fn) = owner_fn else {
        return false;
    };
    let Some((left, right)) = comparison_operands(seam.expression()) else {
        return false;
    };
    boundary_operand_is_closure_derived(owner_fn, &left)
        || boundary_operand_is_closure_derived(owner_fn, &right)
}

fn boundary_operand_is_iterator_derived(owner_fn: &FunctionSummary, operand: &str) -> bool {
    let operand = operand.trim();
    if !is_boundary_operand_identifier(operand) {
        return false;
    }
    owner_fn
        .body
        .lines()
        .any(|line| loop_binds_operand_from_iterator(line, operand))
}

fn boundary_operand_is_closure_derived(owner_fn: &FunctionSummary, operand: &str) -> bool {
    let operand_root = boundary_operand_root_identifier(operand);
    if operand_root.is_empty() {
        return false;
    }
    owner_fn
        .body
        .lines()
        .any(|line| closure_binds_operand_root(line, &operand_root))
}

fn boundary_operand_root_identifier(operand: &str) -> String {
    let operand = operand.trim().trim_start_matches('&').trim();
    operand
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect()
}

fn closure_binds_operand_root(line: &str, operand_root: &str) -> bool {
    let code = strip_comments_and_strings(line);
    let mut rest = code.as_str();
    while let Some(start) = rest.find('|') {
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find('|') else {
            return false;
        };
        let params = &after_start[..end];
        if closure_params_contain_operand(params, operand_root) {
            return true;
        }
        rest = &after_start[end + 1..];
    }
    false
}

fn closure_params_contain_operand(params: &str, operand_root: &str) -> bool {
    params
        .split(',')
        .filter_map(|param| {
            param
                .trim()
                .trim_start_matches("mut ")
                .split_once(':')
                .map(|(name, _)| name.trim())
                .or_else(|| Some(param.trim().trim_start_matches("mut ").trim()))
        })
        .any(|param| param == operand_root)
}

fn loop_binds_operand_from_iterator(line: &str, operand: &str) -> bool {
    let Some(for_index) = line.find("for ") else {
        return false;
    };
    let rest = &line[for_index + "for ".len()..];
    let Some((binding, source)) = rest.split_once(" in ") else {
        return false;
    };
    boundary_loop_source_is_iterator(source)
        && loop_binding_contains_boundary_operand(binding, operand)
}

fn boundary_loop_source_is_iterator(source: &str) -> bool {
    let source = source.split('{').next().unwrap_or(source);
    source.contains(".iter()")
        || source.contains(".iter_mut()")
        || source.contains(".into_iter()")
        || source.contains(".enumerate()")
        || source.contains(".keys()")
        || source.contains(".values()")
}

fn loop_binding_contains_boundary_operand(binding: &str, operand: &str) -> bool {
    binding
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .any(|token| token == operand)
}

fn is_boundary_operand_identifier(operand: &str) -> bool {
    let mut chars = operand.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn boundary_operand_argument(
    owner_fn: &FunctionSummary,
    parameters: &[String],
    operand: &str,
) -> Option<ObservedArgumentOperand> {
    parameters
        .iter()
        .position(|parameter| parameter == operand)
        .map(|index| ObservedArgumentOperand {
            index,
            projection: None,
        })
        .or_else(|| {
            boundary_local_operand_parameter_index(owner_fn, parameters, operand).map(|index| {
                ObservedArgumentOperand {
                    index,
                    projection: None,
                }
            })
        })
        .or_else(|| boundary_parameter_field_projection(parameters, operand))
}

fn boundary_parameter_field_projection(
    parameters: &[String],
    operand: &str,
) -> Option<ObservedArgumentOperand> {
    let operand = operand.trim().trim_start_matches('&').trim();
    for (index, parameter) in parameters.iter().enumerate() {
        let Some(rest) = operand.strip_prefix(parameter) else {
            continue;
        };
        let Some(field) = rest.strip_prefix('.') else {
            continue;
        };
        if is_boundary_operand_identifier(field) {
            return Some(ObservedArgumentOperand {
                index,
                projection: Some(field.to_string()),
            });
        }
    }
    None
}

fn projected_argument_expression(arg: &str, projection: Option<&str>) -> String {
    let arg = arg.trim();
    let Some(field) = projection else {
        return arg.to_string();
    };
    if is_boundary_operand_identifier(arg) {
        format!("{arg}.{field}")
    } else if let Some(borrowed) = arg.strip_prefix('&').map(str::trim)
        && is_boundary_operand_identifier(borrowed)
    {
        format!("&{borrowed}.{field}")
    } else {
        arg.to_string()
    }
}

fn boundary_local_operand_parameter_index(
    owner_fn: &FunctionSummary,
    parameters: &[String],
    operand: &str,
) -> Option<usize> {
    if operand.is_empty() {
        return None;
    }
    for (index, parameter) in parameters.iter().enumerate() {
        if body_contains_wrapped_local_alias(&owner_fn.body, "Some", operand, parameter)
            || body_contains_wrapped_local_alias(&owner_fn.body, "Ok", operand, parameter)
            || body_contains_direct_local_alias(&owner_fn.body, operand, parameter)
        {
            return Some(index);
        }
    }
    None
}

fn body_contains_wrapped_local_alias(
    body: &str,
    wrapper: &str,
    operand: &str,
    parameter: &str,
) -> bool {
    // Built once per body: this runs per seam, related test and parameter.
    let prefix = format!("if let {wrapper}({operand}) = ");
    body.lines().any(|line| {
        let line = code_line_before_comment(line);
        line.strip_prefix(&prefix)
            .is_some_and(|rest| starts_with_identifier_token(rest, parameter))
    }) || (body_contains_match_parameter(body, parameter)
        && body_contains_wrapper_pattern(body, wrapper, operand))
}

fn body_contains_match_parameter(body: &str, parameter: &str) -> bool {
    body.lines().any(|line| {
        let line = code_line_before_comment(line);
        if is_comment_line(line) {
            return false;
        }
        line.find("match ")
            .map(|index| &line[index + "match ".len()..])
            .is_some_and(|rest| starts_with_identifier_token(rest, parameter))
    })
}

fn body_contains_wrapper_pattern(body: &str, wrapper: &str, operand: &str) -> bool {
    let pattern = format!("{wrapper}({operand})");
    body.lines().any(|line| {
        let line = code_line_before_comment(line);
        !is_comment_line(line) && line.contains(&pattern)
    })
}

fn code_line_before_comment(line: &str) -> &str {
    let line = line.trim();
    let line = line.split_once("//").map_or(line, |(code, _comment)| code);
    line.split_once("/*")
        .map_or(line, |(code, _comment)| code)
        .trim()
}

fn is_comment_line(line: &str) -> bool {
    line.starts_with("//") || line.starts_with("/*") || line.starts_with('*')
}

fn starts_with_identifier_token(text: &str, token: &str) -> bool {
    let text = text.trim_start();
    let end = text
        .find(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .unwrap_or(text.len());
    end > 0 && &text[..end] == token
}

fn body_contains_direct_local_alias(body: &str, operand: &str, parameter: &str) -> bool {
    body.lines().any(|line| {
        let line = line.trim().trim_end_matches(';').trim();
        let Some(binding) = line.strip_prefix("let ") else {
            return false;
        };
        let Some((left, right)) = binding.split_once('=') else {
            return false;
        };
        let local_name = left.split_once(':').map(|(name, _)| name).unwrap_or(left);
        local_name.trim() == operand && right.trim() == parameter
    })
}

/// The seam's owner function and its argument selection, resolved once
/// per seam so per-test ranking and activation do not re-derive them.
pub(super) struct SeamOwnerActivation<'index> {
    owner_fn: &'index FunctionSummary,
    selection: ObservedArgumentSelection,
}

pub(super) fn seam_owner_activation<'index>(
    seam: &RepoSeam,
    context: &CompactGripContext<'index>,
) -> Option<SeamOwnerActivation<'index>> {
    let owner_fn = context.owner_function(seam.file(), seam.display_line())?;
    if owner_fn.name.is_empty() {
        return None;
    }
    Some(SeamOwnerActivation {
        owner_fn,
        selection: observed_argument_selection(seam, owner_fn),
    })
}

fn activation_overlap_score(
    seam: &RepoSeam,
    context: &CompactGripContext<'_>,
    indexed: &CompactTest<'_>,
    owner: Option<&SeamOwnerActivation<'_>>,
) -> usize {
    let Some(owner) = owner else {
        return 0;
    };
    let owner_fn = owner.owner_fn;

    let mut score = boundary_equality_overlap_score(seam, indexed, context, owner_fn);
    let required_text = required_discriminator_text(seam);
    score += observed_value_facts_for_test(seam, indexed, context, owner_fn, &owner.selection)
        .iter()
        .filter(|fact| observed_value_matches_required_discriminator(&fact.value, required_text))
        .count();
    score
}

fn observed_value_matches_required_discriminator(value: &str, required_text: &str) -> bool {
    let value = value.trim();
    let required_text = required_text.trim();
    !value.is_empty()
        && !required_text.is_empty()
        && (value == required_text
            || value.contains(required_text)
            || required_text.contains(value))
}

fn boundary_equality_overlap_score(
    seam: &RepoSeam,
    indexed: &CompactTest<'_>,
    context: &CompactGripContext<'_>,
    owner_fn: &FunctionSummary,
) -> usize {
    if seam.kind() != SeamKind::PredicateBoundary {
        return 0;
    }
    let Some((left, right)) = comparison_operands(seam.expression()) else {
        return 0;
    };
    let parameters = function_parameters(owner_fn);
    let Some(left_operand) = boundary_operand_argument(owner_fn, &parameters, &left) else {
        return 0;
    };
    let Some(right_operand) = boundary_operand_argument(owner_fn, &parameters, &right) else {
        return 0;
    };

    let mut score = 0;
    for call in indexed.test.body_calls() {
        if call.name != owner_fn.name {
            continue;
        }
        let Some(args) = owner_call_text(&call.text, owner_fn)
            .and_then(|text| call_arguments(text, &owner_fn.name))
        else {
            continue;
        };
        let Some(left_arg) = args.get(left_operand.index) else {
            continue;
        };
        let Some(right_arg) = args.get(right_operand.index) else {
            continue;
        };
        let left_arg = projected_argument_expression(left_arg, left_operand.projection.as_deref());
        let right_arg =
            projected_argument_expression(right_arg, right_operand.projection.as_deref());
        // Preserve the pre-existing identical-text equality route for
        // ordinary nonfree owners after owner_call_text. This does not grant
        // them the new suffixed-value reader: different text still needs the
        // free-owner authority below, as does every free-owner shortcut.
        let existing_nonfree_equality = owner_fn.impl_context != FunctionImplContext::Free
            && left_arg.trim() == right_arg.trim();
        // Guard the free-owner identical-text shortcut too: two equal typed
        // arguments to a shadowed function are not the owner's boundary.
        if [&left_arg, &right_arg]
            .into_iter()
            .any(|argument| suffixed_decimal_integer_value(argument).is_some())
            && !existing_nonfree_equality
            && !super::classify::bound_free_owner_calls(
                context.index,
                owner_fn,
                indexed.test,
                &call.text,
                context.pin_syntax(),
            )
        {
            continue;
        }
        if arguments_overlap_at_boundary(seam, indexed, context.index, left_arg, right_arg, call) {
            score += 1;
        }
    }
    score
}

fn arguments_overlap_at_boundary(
    seam: &RepoSeam,
    indexed: &CompactTest<'_>,
    index: &RustIndex,
    left_arg: String,
    right_arg: String,
    call: &CallFact,
) -> bool {
    if left_arg.trim() == right_arg.trim() && !left_arg.trim().is_empty() {
        return true;
    }
    let left_values = resolved_argument_values(seam, indexed, index, &left_arg, call);
    let right_values = resolved_argument_values(seam, indexed, index, &right_arg, call);
    left_values.iter().any(|left| {
        let left = comparable_value(left);
        right_values
            .iter()
            .any(|right| left == comparable_value(right))
    })
}

fn resolved_argument_values(
    seam: &RepoSeam,
    indexed: &CompactTest<'_>,
    index: &RustIndex,
    arg: &str,
    call: &CallFact,
) -> Vec<String> {
    let values = scalar_values(arg);
    if !values.is_empty() {
        return values;
    }
    let value_facts = indexed.value_facts(index);
    let env = super::value_resolution::ValueEnv::new(seam, value_facts);
    env.resolve_at_call(arg, call.line, &call.name, &call.text)
        .into_iter()
        .map(|(value, _context)| value)
        .collect()
}

fn compact_activate_evidence(
    seam: &RepoSeam,
    related: &[&CompactTest<'_>],
    context: &CompactGripContext<'_>,
    owner_fn: Option<&FunctionSummary>,
) -> (StageEvidence, Vec<MissingDiscriminatorFact>) {
    if seam.kind() == SeamKind::PredicateBoundary || seam.kind() == SeamKind::FieldConstruction {
        let (stage, _observed, missing) = activate_evidence(seam, related, context, owner_fn);
        return (stage, missing);
    }

    let owner_name = owner_fn.map(|f| f.name.as_str()).unwrap_or("");
    // Same contradiction gate as the full path (#7007 review): a related
    // test whose equality assertion statically contradicts the owner's fold
    // contributes no activation credit here either, so the compact evidence
    // cannot disagree with the full evidence on whose call counts.
    // Same contradiction gate as the full path (#7007 review): a related
    // test whose equality assertion statically contradicts the owner's fold
    // contributes no activation credit here either, so the compact evidence
    // cannot disagree with the full evidence on whose call counts.
    let activation_credited: Vec<&CompactTest<'_>> = related
        .iter()
        .copied()
        .filter(|indexed| !test_has_contradicted_assertion(indexed.test, owner_fn))
        .collect();
    let target_affinity_tokens =
        (!requires_concrete_activation_values(seam)).then(|| assertion_target_tokens(seam));
    // A free owner is called only by a bare or module-qualified spelling:
    // `ByteSize::kb(100)` activates the associated `kb`, not the free one
    // beside it (#6713). The helper and target-affinity arms read the
    // free-spelling projections for a free owner, so a helper calling only
    // `Type::name(..)` cannot activate the free `name` (#6713 review).
    let free_owner = owner_fn.is_some_and(|owner| owner.impl_context == FunctionImplContext::Free);
    let direct_owner_call = !owner_name.is_empty()
        && activation_credited.iter().any(|indexed| {
            if free_owner {
                (indexed.call_names.contains(owner_name)
                    && test_calls_free_function(indexed.test, owner_name))
                    || indexed
                        .free_spelling_helper_owner_call_names
                        .contains(owner_name)
                    || (indexed
                        .free_spelling_target_affinity_owner_call_names
                        .contains(owner_name)
                        && has_owner_call_via_target_affinity(
                            indexed,
                            owner_name,
                            target_affinity_tokens.as_ref(),
                        ))
            } else {
                indexed.call_names.contains(owner_name)
                    || indexed.helper_owner_call_names.contains(owner_name)
                    || has_owner_call_via_target_affinity(
                        indexed,
                        owner_name,
                        target_affinity_tokens.as_ref(),
                    )
            }
        });
    let ambiguous_constructor_field_owner = !owner_name.is_empty()
        && related
            .iter()
            .any(|indexed| has_ambiguous_constructor_field_owner(indexed, seam, owner_name));
    let state = if related.is_empty() {
        StageState::No
    } else if ambiguous_constructor_field_owner {
        StageState::Unknown
    } else if direct_owner_call {
        StageState::Yes
    } else {
        StageState::Unknown
    };
    let summary = if ambiguous_constructor_field_owner {
        format!(
            "constructor_field_owner_ambiguous: exact field observer found, but same-crate caller linkage to owner `{owner_name}` is ambiguous"
        )
    } else {
        format!(
            "Compact activation evidence for seam `{}` is `{}`",
            seam.expression()
                .lines()
                .next()
                .unwrap_or(seam.expression()),
            state.as_str()
        )
    };
    let stage = StageEvidence::new(
        state.clone(),
        if direct_owner_call && !ambiguous_constructor_field_owner {
            Confidence::Medium
        } else {
            Confidence::Low
        },
        summary,
    );
    (stage, Vec::new())
}

fn missing_discriminators_for(
    seam: &RepoSeam,
    observed: &[ValueFact],
    boundary_activation_operands_unresolved: bool,
    boundary_equality_observed: bool,
    boundary_constant: Option<&BoundaryConstantOperand>,
    boundary: Option<&PredicateBoundaryOperand>,
) -> Vec<MissingDiscriminatorFact> {
    match seam.kind() {
        SeamKind::PredicateBoundary => {
            if boundary_activation_operands_unresolved || boundary_equality_observed {
                return Vec::new();
            }
            // Without a value model we cannot prove the boundary value is
            // tested. Every comparison operator (strict or not) flips
            // against its neighbour only at the equality boundary, so
            // surface it unless an observed value names it (#4214).
            let boundary_token = boundary.map_or_else(
                || boundary_rhs_token(seam.expression()),
                |boundary| boundary.token.clone(),
            );
            if boundary_token.is_empty() {
                return Vec::new();
            }
            let any_observed = !observed.is_empty();
            if !any_observed {
                return vec![MissingDiscriminatorFact {
                    value: format!("{boundary_token} (boundary value)"),
                    reason: "no observed activation values for boundary predicate".to_string(),
                    flow_sink: None,
                }];
            }
            // We do not yet know the literal value of `boundary_token`,
            // so we can only flag that the equality boundary is not
            // explicitly named in the observed value set.
            //
            // Use whole-value equality rather than `contains` to avoid
            // false matches like `boundary_token = "10"` matching observed
            // value `"100"`. Compare through `comparable_value` so digit
            // separators do not matter (`1_000` names `1000`).
            // A local boundary (`let limit = 100;`) is matched by its
            // input-free value, the same value `check` evaluates (#4228).
            let local_value = boundary.and_then(PredicateBoundaryOperand::local_value);
            let boundary_value = comparable_value(local_value.unwrap_or(&boundary_token));
            let equality_seen = observed
                .iter()
                .any(|v| comparable_value(&v.value) == boundary_value);
            if equality_seen {
                Vec::new()
            } else {
                let reason = match boundary_constant {
                    _ if let Some(value) = local_value => format!(
                        "observed values do not include the equality-boundary case for this predicate ({boundary_token} = {value})"
                    ),
                    Some(constant) if let Some(value) = constant.lookup.value() => format!(
                        "observed values do not include the equality-boundary case for this predicate ({boundary_token} = {value})"
                    ),
                    Some(constant) => format!(
                        "observed values do not include the equality-boundary case for this predicate; ripr cannot see the value of constant {boundary_token} statically ({}), so pass {boundary_token} itself as the boundary input",
                        constant.lookup.limitation()
                    ),
                    None => {
                        "observed values do not include the equality-boundary case for this predicate"
                            .to_string()
                    }
                };
                vec![MissingDiscriminatorFact {
                    value: format!("{boundary_token} (equality boundary)"),
                    reason,
                    flow_sink: None,
                }]
            }
        }
        SeamKind::ErrorVariant => Vec::new(),
        SeamKind::ReturnValue
        | SeamKind::FieldConstruction
        | SeamKind::SideEffect
        | SeamKind::MatchArm
        | SeamKind::CallPresence => Vec::new(),
    }
}

/// A predicate-boundary seam's constant operand: the right-hand operand
/// names a constant (per the shared lookup in `value_resolution`) while the
/// left-hand operand is an owner parameter (or a projection of one).
struct BoundaryConstantOperand {
    name: String,
    lookup: super::value_resolution::NamedConstant,
    argument_index: usize,
}

fn boundary_constant_operand(
    seam: &RepoSeam,
    index: &RustIndex,
    owner_fn: &FunctionSummary,
) -> Option<BoundaryConstantOperand> {
    if seam.kind() != SeamKind::PredicateBoundary {
        return None;
    }
    let (left, right) = comparison_operands(seam.expression())?;
    let name = super::value_resolution::constant_operand_name(&right)?;
    let parameters = function_parameters(owner_fn);
    if boundary_operand_argument(owner_fn, &parameters, &right).is_some() {
        return None;
    }
    let left_operand = boundary_operand_argument(owner_fn, &parameters, &left)?;
    let lookup = index.files().get(&owner_fn.file).map_or(
        super::value_resolution::NamedConstant::Undeclared,
        |facts| super::value_resolution::named_constant(&facts.source, name),
    );
    Some(BoundaryConstantOperand {
        name: name.to_string(),
        lookup,
        argument_index: left_operand.index,
    })
}

/// A direct owner call whose compared argument names the boundary
/// constant itself (`discounted_total(DISCOUNT_THRESHOLD)`), in a test whose
/// own file does not declare a constant of the same name.
fn test_passes_boundary_constant(
    indexed: &CompactTest<'_>,
    owner_fn: Option<&FunctionSummary>,
    index: &RustIndex,
    constant: &BoundaryConstantOperand,
) -> bool {
    let Some(owner_fn) = owner_fn else {
        return false;
    };
    let owner_name = owner_fn.name.as_str();
    if super::value_resolution::test_file_may_shadow_constant(
        &owner_fn.file,
        &indexed.test.file,
        index
            .files()
            .get(&indexed.test.file)
            .map(|facts| facts.data().source.as_ref()),
        &constant.name,
    ) {
        return false;
    }
    indexed.test.calls.iter().any(|call| {
        call.name == owner_name
            && owner_call_text(&call.text, owner_fn)
                .and_then(|text| call_arguments(text, owner_name))
                .and_then(|arguments| arguments.get(constant.argument_index).cloned())
                .is_some_and(|argument| {
                    super::value_resolution::argument_names_constant(&argument, &constant.name)
                })
    })
}

/// The boundary side of a predicate seam: the operand that does not bind
/// to an owner parameter, so a reversed literal (`100 < amount`) names
/// `100`, not `amount` (#4228). A local boundary carries what `check`'s
/// evaluator can say about it.
struct PredicateBoundaryOperand {
    token: String,
    local: super::classify::LocalBoundary,
}

impl PredicateBoundaryOperand {
    fn local_value(&self) -> Option<&str> {
        match &self.local {
            super::classify::LocalBoundary::Exact(value) => Some(value),
            _ => None,
        }
    }

    fn local_unresolved(&self) -> bool {
        matches!(self.local, super::classify::LocalBoundary::Unresolved)
    }
}

fn predicate_boundary_operand(
    seam: &RepoSeam,
    owner_fn: &FunctionSummary,
) -> Option<PredicateBoundaryOperand> {
    if seam.kind() != SeamKind::PredicateBoundary {
        return None;
    }
    let (left, right) = comparison_operands(seam.expression())?;
    let parameters = function_parameters(owner_fn);
    // The reversed shape `observed_argument_selection` observes: a literal
    // or input-free local on the left compared against a parameter on the
    // right.
    let reversed = boundary_operand_argument(owner_fn, &parameters, &left).is_none()
        && left_is_boundary_value(owner_fn, &left, seam.display_line())
        && boundary_operand_argument(owner_fn, &parameters, &right).is_some();
    let token = boundary_token(if reversed { &left } else { &right });
    if token.is_empty() {
        return None;
    }
    let local = super::classify::local_boundary(owner_fn, &token, seam.display_line());
    Some(PredicateBoundaryOperand { token, local })
}

/// Whether the left operand of a predicate is a boundary value rather than
/// the tested input: a literal (`100 < amount`, `1.5f64 < amount`) or a
/// local whose initializer folds without test input (`let limit = 100;
/// limit < amount`), the value `check` evaluates per row (#4270). A local
/// that depends on inputs, or that the evaluator cannot fold, is not one.
fn left_is_boundary_value(owner_fn: &FunctionSummary, left: &str, predicate_line: usize) -> bool {
    numeric_boundary_literal(left).is_some()
        || !scalar_values(left).is_empty()
        || matches!(
            super::classify::local_boundary(
                owner_fn,
                &boundary_identifier_prefix(left),
                predicate_line
            ),
            super::classify::LocalBoundary::Exact(_)
        )
}

/// Best-effort right-hand-side identifier for a boundary predicate.
/// Returns empty if we cannot pick one out heuristically.
fn boundary_rhs_token(expression: &str) -> String {
    for op in [" >= ", " <= ", " == ", " != ", " > ", " < "] {
        if let Some(idx) = expression.find(op) {
            let token = boundary_identifier_prefix(&expression[idx + op.len()..]);
            if !token.is_empty() {
                return token;
            }
        }
    }
    String::new()
}

/// The boundary operand as observed values spell it: a whole integer,
/// decimal, char or string literal (`-100`, `1.5`, `'m'`), else its
/// identifier prefix. Decimals are whole in both owners (#4271); enum
/// paths keep the identifier-prefix spelling because `check` does not
/// read them as whole values either (#4228).
fn boundary_token(operand: &str) -> String {
    if let Some(literal) = numeric_boundary_literal(operand) {
        return literal;
    }
    match scalar_values(operand).into_iter().next() {
        Some(literal) if !literal.contains("::") => literal,
        _ => boundary_identifier_prefix(operand),
    }
}

/// A numeric boundary read by `check`'s own literal reader, so a suffixed
/// literal (`1.5f64`, `100u32`) spells the same value in both owners
/// instead of grip's identifier prefix (`1`, `100u32`).
fn numeric_boundary_literal(operand: &str) -> Option<String> {
    let trimmed = operand.trim();
    trimmed
        .strip_prefix('-')
        .unwrap_or(trimmed)
        .starts_with(|ch: char| ch.is_ascii_digit())
        .then(|| super::classify::literal_operand_value(trimmed))
        .flatten()
}

/// An operand up to its first non-identifier character.
fn boundary_identifier_prefix(operand: &str) -> String {
    operand
        .trim()
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect()
}

fn function_parameters(function: &FunctionSummary) -> Vec<String> {
    let signature = function
        .body
        .lines()
        .next()
        .unwrap_or(function.body.as_str());
    let Some(open) = signature.find('(') else {
        return Vec::new();
    };
    let after_open = &signature[open + 1..];
    let Some(close) = after_open.find(')') else {
        return Vec::new();
    };
    split_top_level_commas(&after_open[..close])
        .into_iter()
        .filter_map(|argument| {
            argument
                .split_once(':')
                .map(|(name, _type)| name.trim().to_string())
        })
        .filter(|name| !name.is_empty() && name != "self" && name != "&self" && name != "mut self")
        .collect()
}

fn comparison_operands(expression: &str) -> Option<(String, String)> {
    for operator in [">=", "<=", "==", "!=", ">", "<"] {
        if let Some((left, right)) = expression.split_once(operator) {
            let left = clean_operand(left);
            let right = clean_operand(right);
            if !left.is_empty() && !right.is_empty() {
                return Some((left, right));
            }
        }
    }
    None
}

fn clean_operand(operand: &str) -> String {
    let cleaned = operand
        .trim()
        .trim_start_matches("if ")
        .trim_end_matches('{')
        .trim_end_matches(';')
        .trim();
    cleaned
        .split_once('{')
        .map(|(before, _after)| before.trim())
        .unwrap_or(cleaned)
        .to_string()
}

fn comparable_value(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .chars()
        .filter(|ch| *ch != '_')
        .collect()
}

fn propagate_evidence(
    seam: &RepoSeam,
    related: &[&TestSummary],
    owner_fn: Option<&FunctionSummary>,
    index: &RustIndex,
    pin_syntax: &crate::analysis::classify::OwnerPinSyntax,
) -> StageEvidence {
    if related.is_empty() {
        return StageEvidence::new(
            StageState::No,
            Confidence::Medium,
            "No related tests; cannot infer propagation",
        );
    }
    // A predicate whose one-line arms all hold the same statement changes no
    // value a sink can carry, so an exact assertion on the owner's result
    // cannot notice the condition (#12053; `ripr check` reads the same line
    // as a limit since #10880).
    if seam.kind() == SeamKind::PredicateBoundary
        && predicate_line_arms_all_agree(owner_fn, seam.display_line())
    {
        return StageEvidence::new(
            StageState::Unknown,
            Confidence::Low,
            "The predicate's one-line arms hold the same statement, so no sink value depends on it",
        );
    }
    // Static heuristic: if any related test contains an oracle that
    // matches the expected sink class (e.g., return value -> assert_eq!),
    // call it Yes. Otherwise Unknown.
    let any_oracle = related.iter().any(|t| !t.assertions.is_empty());
    let any_matching_sink = related
        .iter()
        .any(|t| oracles_match_sink(seam, t, owner_fn, index, pin_syntax));
    let state = match (any_oracle, any_matching_sink) {
        (true, true) => StageState::Yes,
        (true, false) => StageState::Unknown,
        (false, _) => StageState::Unknown,
    };
    let summary = format!(
        "Static propagation to `{}` sink is {}",
        seam.expected_sink().as_str(),
        state.as_str()
    );
    StageEvidence::new(state, Confidence::Low, summary)
}

fn oracles_match_sink(
    seam: &RepoSeam,
    test: &TestSummary,
    owner_fn: Option<&FunctionSummary>,
    index: &RustIndex,
    pin_syntax: &crate::analysis::classify::OwnerPinSyntax,
) -> bool {
    test.assertions
        .iter()
        // A statically contradicted assertion propagates nothing: it pins
        // the mutant's value, not the owner's (#6026).
        .filter(|oracle| {
            !matches!(
                exact_value_assertion_verdict_for_test(owner_fn, &oracle.text, test),
                ExactValueVerdict::Contradicted { .. }
            )
        })
        .any(|oracle| match seam.expected_sink() {
            ExpectedSink::ReturnValue | ExpectedSink::OutputField => {
                matches!(
                    oracle.kind,
                    OracleKind::ExactValue
                        | OracleKind::WholeObjectEquality
                        | OracleKind::Snapshot
                        | OracleKind::RelationalCheck
                ) || (seam.kind() == SeamKind::ReturnValue
                    && matches!(oracle.kind, OracleKind::ExactErrorVariant)
                    && exact_err_return_matches_error_pin(seam, &oracle.text))
            }
            ExpectedSink::ErrorChannel => matches!(
                oracle.kind,
                OracleKind::ExactErrorVariant | OracleKind::BroadError
            ),
            ExpectedSink::SideEffect => {
                if direct_collection_mutation_receiver(seam.expression()).is_some() {
                    oracle_discriminates_seam_in_test(
                        seam, oracle, test, owner_fn, index, pin_syntax,
                    )
                } else {
                    matches!(oracle.kind, OracleKind::MockExpectation)
                }
            }
        })
}

fn observe_evidence(related: &[&TestSummary]) -> StageEvidence {
    if related.is_empty() {
        return StageEvidence::new(
            StageState::No,
            Confidence::Medium,
            "No related tests; nothing observes the seam",
        );
    }
    let any_oracle = related.iter().any(|t| !t.assertions.is_empty());
    let any_smoke_only = related.iter().all(|t| {
        !t.assertions.is_empty() && t.assertions.iter().all(|o| o.kind == OracleKind::SmokeOnly)
    });
    let state = if !any_oracle {
        StageState::No
    } else if any_smoke_only {
        StageState::Weak
    } else {
        StageState::Yes
    };
    let summary = format!("Observation evidence is `{}`", state.as_str());
    StageEvidence::new(state, Confidence::Medium, summary)
}

fn discriminate_evidence(
    seam: &RepoSeam,
    related: &[&TestSummary],
    owner_fn: Option<&FunctionSummary>,
    index: &RustIndex,
    pin_syntax: &crate::analysis::classify::OwnerPinSyntax,
) -> StageEvidence {
    if related.is_empty() {
        return StageEvidence::new(
            StageState::No,
            Confidence::Medium,
            "No related tests; oracle cannot discriminate",
        );
    }
    let boolean_pin = BooleanBoundaryPin::establish(seam, owner_fn, index);
    let mut best = OracleStrength::None;
    let mut best_matching = OracleStrength::None;
    let mut contradictions: Vec<String> = Vec::new();
    for test in related {
        for oracle in &test.assertions {
            if !collection_assertion_admitted(seam, oracle, test, index, pin_syntax) {
                continue;
            }
            // A statically contradicted exact-value assertion keeps at most
            // Weak credit: it pins the mutant's value and fails at baseline,
            // so it must not grade the seam's discrimination up (#6026).
            let (strength, contradiction) = effective_oracle_strength(
                owner_fn,
                oracle,
                test,
                boolean_pin.as_ref(),
                index,
                pin_syntax,
            );
            if let Some(contradiction) = contradiction {
                contradictions.push(contradiction);
            }
            if strength.rank() > best.rank() {
                best = strength.clone();
            }
            // RIPR-SPEC-0106 (Part B): for ErrorVariant seams, oracle_kind_matches_seam
            // is necessary but not sufficient — the oracle must also structurally pin
            // the seam's specific variant. oracle_discriminates_seam checks both.
            if oracle_discriminates_seam_in_test(seam, oracle, test, owner_fn, index, pin_syntax)
                && strength.rank() > best_matching.rank()
            {
                best_matching = strength;
            }
        }
    }
    let state = if seam.kind() == SeamKind::FieldConstruction {
        match (&best_matching, &best) {
            (OracleStrength::Strong | OracleStrength::Medium, _) => StageState::Yes,
            (OracleStrength::Weak | OracleStrength::Smoke, _) => StageState::Weak,
            (OracleStrength::Unknown, _) => StageState::Unknown,
            (OracleStrength::None, OracleStrength::None) => StageState::No,
            (OracleStrength::None, OracleStrength::Unknown) => StageState::Unknown,
            (OracleStrength::None, _) => StageState::Weak,
        }
    } else {
        match (best_matching != OracleStrength::None, &best) {
            (_, OracleStrength::None) => StageState::No,
            (_, OracleStrength::Unknown) => StageState::Unknown,
            (_, OracleStrength::Weak | OracleStrength::Smoke) => StageState::Weak,
            (true, OracleStrength::Strong | OracleStrength::Medium) => StageState::Yes,
            (false, OracleStrength::Strong | OracleStrength::Medium) => StageState::Weak,
        }
    };
    let mut summary = format!(
        "Strongest oracle for seam kind `{}` is `{}` (kind-match {})",
        seam.kind().as_str(),
        best.as_str(),
        best_matching != OracleStrength::None
    );
    if !contradictions.is_empty() {
        summary.push_str("; ");
        summary.push_str(&contradictions.join("; "));
    }
    StageEvidence::new(state, Confidence::Medium, summary)
}

/// Owner-relative oracle strength and any static-contradiction disclosure.
/// An equality contradicted by the owner's literal fold loses credit. A
/// separately admitted boolean boundary pin gains credit only when its exact
/// call is statically consistent and paired with this assertion. Other
/// consistent or not-evaluable assertions keep their extracted strength.
fn effective_oracle_strength(
    owner_fn: Option<&FunctionSummary>,
    oracle: &OracleFact,
    test: &TestSummary,
    boolean_pin: Option<&BooleanBoundaryPin<'_>>,
    index: &RustIndex,
    pin_syntax: &crate::analysis::classify::OwnerPinSyntax,
) -> (OracleStrength, Option<String>) {
    match assertion_verdict_for_test(owner_fn, &oracle.text, test) {
        ExactValueVerdict::Contradicted {
            expected,
            evaluated,
        } => (
            OracleStrength::Weak,
            Some(contradiction_summary(&expected, evaluated)),
        ),
        _ if boolean_pin.is_some_and(|pin| pin.admits(test, oracle, index, pin_syntax)) => {
            (OracleStrength::Strong, None)
        }
        _ => (oracle.strength.clone(), None),
    }
}

/// The equality fold's verdict, extended by #8610's admitted direct boolean
/// shape. Each entry point stays fail-closed, and the boolean fold is only
/// consulted where the equality reading was not evaluable, so the extension
/// can withhold credit or state a contradiction the equality fold could not
/// see — it never upgrades an oracle the equality fold already graded.
fn assertion_verdict_for_test(
    owner: Option<&FunctionSummary>,
    oracle_text: &str,
    test: &TestSummary,
) -> ExactValueVerdict {
    let Some(owner) = owner else {
        return ExactValueVerdict::NotEvaluable;
    };
    match exact_value_assertion_verdict_for_test(Some(owner), oracle_text, test) {
        ExactValueVerdict::NotEvaluable => {
            boolean_assertion_verdict_for_test(owner, oracle_text, test)
        }
        verdict => verdict,
    }
}

/// Whether any of `test`'s assertions statically contradicts the owner's
/// folded behavior (#6026). Such a test fails at baseline and its call-site
/// values pin the mutant's value, so its activation credit is withheld beside
/// its oracle credit (#7007). A supported direct boolean assertion counts the
/// same way: `assert!(owner(18)); assert!(!owner(18));` fails on its second
/// line, so the consistent first line must not lend the failing test's
/// promotion to the seam (#8610 review). Fail-closed toward prior credit:
/// without a resolved owner nothing is contradicted.
///
/// This is the one whole-test contradiction rule for every surface that
/// credits the test: the repo-seam activation authority reads it in
/// [`activate_evidence`], and the diff-mode classifier reads it for the same
/// purpose (#10980), so identical evidence reaches one verdict on both.
pub(in crate::analysis) fn test_has_contradicted_assertion(
    test: &TestSummary,
    owner_fn: Option<&FunctionSummary>,
) -> bool {
    let Some(owner_fn) = owner_fn else {
        return false;
    };
    test.assertions.iter().any(|oracle| {
        matches!(
            assertion_verdict_for_test(Some(owner_fn), &oracle.text, test),
            ExactValueVerdict::Contradicted { .. }
        )
    })
}

/// The diff-mode whole-test contradiction (#10980). A related test carries
/// the diff surface's credit only when it passes SOMEWHERE: it is withheld
/// when it fails on BOTH versions — some assertion row contradicts the
/// candidate body's fold and some row contradicts the baseline body — so
/// the test passes on neither version and cannot notice the change. A row
/// written for the baseline that the candidate breaks is the discriminator
/// itself (the exact-boundary witness of the agentic b4 journey), and a
/// row the candidate satisfies is the change's own credit; neither
/// withholds. The baseline body is the owner's current body with the
/// probe's `after` expression replaced by `before` exactly once (the
/// #8659/#10916 reconstruction); a probe without a baseline expression or
/// a body that does not reconstruct keeps the test's credit — the fold
/// can only withhold. The repo-seam rule above stays current-fold-only:
/// there the current body is the shipped code and a contradicted row is
/// a wrong-way discriminator, while on the diff surface the current body
/// is the candidate under review.
pub(in crate::analysis) fn test_fails_on_both_diff_versions(
    test: &TestSummary,
    owner: &FunctionSummary,
    before: Option<&str>,
    after: Option<&str>,
) -> bool {
    let (Some(before), Some(after)) = (before, after) else {
        return false;
    };
    if before == after || owner.body.matches(after).count() != 1 {
        return false;
    }
    let mut baseline_owner = owner.clone();
    baseline_owner.body =
        crate::analysis::facts::SourceText::from(owner.body.as_str().replacen(after, before, 1));
    let mut fails_on_candidate = false;
    let mut fails_on_base = false;
    for oracle in &test.assertions {
        if !fails_on_candidate
            && matches!(
                assertion_verdict_for_test(Some(owner), &oracle.text, test),
                ExactValueVerdict::Contradicted { .. }
            )
        {
            fails_on_candidate = true;
        }
        if !fails_on_base
            && matches!(
                assertion_verdict_for_test(Some(&baseline_owner), &oracle.text, test),
                ExactValueVerdict::Contradicted { .. }
            )
        {
            fails_on_base = true;
        }
        if fails_on_candidate && fails_on_base {
            return true;
        }
    }
    false
}

/// Whether the whole-test contradiction has an established reading. The
/// fold's premise is the written assertion's plain macro semantics, so a
/// row the owner pin's reading refuses (a binding that may rebind the
/// macro, a guard that may keep it from running) leaves the
/// baseline-failing claim unestablished and the caller must not withhold
/// on it (#5431, RIPR-SPEC-0240): the same refusal authority the outcome's
/// named-limit projection consults, not a second rule. An equality-family
/// row the pin admits reads established; non-equality rows (the #8610
/// direct boolean assertions) carry their own fail-closed shape guards in
/// [`value_contradiction::boolean_assertion_verdict_for_test`].
pub(in crate::analysis) fn contradicted_assertion_reading_is_established(
    test: &TestSummary,
    owner_fn: &FunctionSummary,
    probe: &crate::domain::Probe,
    pin_syntax: &crate::analysis::classify::OwnerPinSyntax,
    index: &RustIndex,
) -> bool {
    test.assertions.iter().all(|assertion| {
        let contradicted = matches!(
            assertion_verdict_for_test(Some(owner_fn), &assertion.text, test),
            ExactValueVerdict::Contradicted { .. }
        );
        !contradicted
            || pin_syntax
                .equality_assertion_refusal(probe, test, assertion, index)
                .is_none()
    })
}

/// Returns true when `oracle` is a discriminating match for `seam`.
///
/// For most non-ErrorVariant seams this is identical to
/// `oracle_kind_matches_seam` (the existing kind-category check).
/// `FieldConstruction` additionally requires the oracle to name the exact
/// constructed field through a member access or record field/pattern. A strong
/// assertion on a sibling field is not evidence for this field seam.
///
/// For `ErrorVariant` seams (RIPR-SPEC-0106, Part B) the oracle must also
/// structurally pin the seam's specific variant:
/// - The oracle kind must be `ExactErrorVariant`.
/// - The oracle text must contain the seam's exact variant identity
///   (from `RequiredDiscriminator::ErrorVariant { variant }`).
/// - If the seam variant cannot be parsed from the discriminator, or the
///   oracle text does not contain it, the oracle is NOT credited (fail-closed).
///
/// This is the over-credit guard: a test that pins `MyError::Negative` does
/// NOT discriminate a `MyError::TooLarge` seam.
fn oracle_discriminates_seam_in_test(
    seam: &RepoSeam,
    oracle: &OracleFact,
    test: &TestSummary,
    owner: Option<&FunctionSummary>,
    index: &RustIndex,
    pin_syntax: &crate::analysis::classify::OwnerPinSyntax,
) -> bool {
    if !collection_assertion_admitted(seam, oracle, test, index, pin_syntax) {
        return false;
    }
    // #8121: a snapshot admitted by kind for a return-value seam must also
    // observe the owner's own result, not an unrelated value in the test.
    if seam.kind() == SeamKind::ReturnValue
        && matches!(oracle.kind, OracleKind::Snapshot)
        && let Some(owner) = owner
        && !snapshot_owner_binding::snapshot_observes_owner_result(&owner.name, oracle, test)
    {
        return false;
    }
    // #11398: a field write is checked only by an exact read of the written
    // field that the diff probe's own rule confirms. The seam's field name
    // alone (any binding, any sibling) is never a discriminator.
    if let Some(path) = written_field(seam) {
        return owner.is_some_and(|owner| {
            oracle_kind_matches_seam_kind(seam.kind(), &oracle.kind)
                && !test_may_skip_read(test)
                && crate::analysis::classify::assigned_field_read_confirms(
                    owner, path, test, oracle, index,
                )
        });
    }
    oracle_discriminates_seam(seam, oracle)
        || direct_collection_mutation_receiver(seam.expression())
            .zip(owner)
            .is_some_and(|(receiver, owner)| {
                collection_state_write_oracle_kind(&oracle.kind)
                    && assertion_observes_collection_argument(
                        receiver, owner, index, test, oracle, pin_syntax,
                    )
            })
}

fn collection_assertion_admitted(
    seam: &RepoSeam,
    oracle: &OracleFact,
    test: &TestSummary,
    index: &RustIndex,
    pin_syntax: &crate::analysis::classify::OwnerPinSyntax,
) -> bool {
    !matches!(seam.kind(), SeamKind::SideEffect | SeamKind::CallPresence)
        || direct_collection_mutation_receiver(seam.expression()).is_none()
        || pin_syntax.admits_effect_assertion(test, oracle, index)
}

fn oracle_discriminates_seam(seam: &RepoSeam, oracle: &super::facts::OracleFact) -> bool {
    if let Some(receiver) = direct_collection_mutation_receiver(seam.expression()) {
        // #6640: a pure count read (`.len()`, `.is_empty()`) notices a
        // deleted push, not a changed pushed value, so it discriminates
        // only the call-presence seam.
        return collection_state_write_oracle_kind(&oracle.kind)
            && assertion_observes_direct_collection_for_probe(
                &oracle.text,
                receiver,
                seam.kind() == SeamKind::CallPresence,
            );
    }
    // #11111: a return-value seam whose changed expression constructs an exact
    // `Err(Type::Variant)` is the same behavior the error-variant seam over
    // that span grades, so an exact error-variant pin naming that variant
    // discriminates it too (equalizes the two seams; only ever lowers repair
    // actionability). Any other oracle kind keeps the ordinary kind rule.
    if seam.kind() == SeamKind::ReturnValue && matches!(oracle.kind, OracleKind::ExactErrorVariant)
    {
        return exact_err_return_matches_error_pin(seam, &oracle.text);
    }
    if !oracle_kind_matches_seam(seam, &oracle.kind) {
        return false;
    }
    if seam.kind() == SeamKind::FieldConstruction {
        // #8120/#8121: snapshot syntax and a matching field spelling do not
        // bind expected bytes or that field's receiver to the owner's result.
        // Preserve the existing expect-test equality operand admission; other
        // snapshot field observations remain unconfirmed until a producer
        // establishes that binding, even when configured strength is higher.
        if matches!(oracle.kind, OracleKind::Snapshot)
            && !crate::analysis::classify::is_inline_expect_snapshot(oracle)
        {
            return false;
        }
        return field_construction_oracle_matches_seam_field(seam, &oracle.text);
    }
    if matches!(oracle.kind, OracleKind::GuardedResultMatch) {
        // #3731 review: a guarded Result match discriminates only when its
        // exact pin names the seam's changed variant — the same rule as the
        // reveal-side confirmation gate. A sibling-variant or type-only pin
        // never discriminates. The fact's Ok-arm observation decision rides
        // along for success-payload return-value seams (RIPR-SPEC-0175).
        return guarded_result_oracle_matches_seam_variant(
            seam,
            &oracle.text,
            oracle.ok_value_observed,
        );
    }
    if seam.kind() != SeamKind::ErrorVariant {
        return true;
    }
    // ErrorVariant seam: require variant-level structural match.
    error_variant_oracle_matches_seam_variant(seam, &oracle.text)
}

fn collection_state_write_oracle_kind(kind: &OracleKind) -> bool {
    matches!(
        kind,
        OracleKind::ExactValue
            | OracleKind::WholeObjectEquality
            | OracleKind::Snapshot
            | OracleKind::RelationalCheck
    )
}

/// The scrutinee callee embedded in a synthesized guarded-Result-match
/// oracle text (`match <path>(..) { .. }`), when the scrutinee is a BARE
/// single-segment path (#3731 review F20: repo discrimination aligns with
/// the diff-path bare-only rule). A qualified scrutinee
/// (`other_crate::parse`) shares its terminal segment with any same-named
/// owner, so reducing the path to that segment would bind a different
/// entity by token coincidence — qualified scrutinees do not discriminate,
/// and `None` fails closed. `None` also when the text does not embed a
/// recognizable plain-path scrutinee (#3731 review round 4).
fn guarded_oracle_scrutinee_callee(oracle_text: &str) -> Option<&str> {
    let rest = oracle_text.strip_prefix("match ")?;
    let open = rest.find("(..)")?;
    let path = rest[..open].trim();
    let plain = !path.is_empty()
        && path.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == ':'
        })
        && path.split("::").all(|segment| !segment.is_empty());
    if !plain {
        return None;
    }
    if path.contains("::") {
        return None;
    }
    Some(path)
}

/// The variant pin carried by a synthesized guarded-Result-match oracle
/// text (`... Err(..) => Some(ParseError::InvalidData { .. }) }`): the
/// slice after the `Err(..) =>` template marker — cut before the
/// catch-all template tail when the routing form appended one — reduced
/// to qualified variant paths. Empty when the pin names no variant (a
/// type-only pin), so exact-variant seams fail closed.
fn guarded_oracle_variant_pins(oracle_text: &str) -> Vec<String> {
    use super::classify::enum_variant_values;

    const MARKER: &str = "Err(..) => ";
    let Some(start) = oracle_text.find(MARKER).map(|at| at + MARKER.len()) else {
        return Vec::new();
    };
    let rest = &oracle_text[start..];
    let pin = match rest.find(", _ => ..") {
        Some(end) => &rest[..end],
        None => rest.strip_suffix(" }").unwrap_or(rest),
    };
    enum_variant_values(pin)
}

/// Variant comparison for a `GuardedResultMatch` oracle against an
/// ErrorVariant or ReturnValue seam (#3731 review).
///
/// Callee identity gates everything (#3731 review round 4, F20): the
/// synthesized text embeds the scrutinee path after `match `, and a
/// guarded match over a DIFFERENT callee observes someone else's result no
/// matter which variant it pins. The scrutinee must be BARE and exactly
/// equal to the seam's owner terminal name — a qualified scrutinee whose
/// terminal segment matches (`other_crate::parse` over an owner named
/// `parse`) is the token-coincidence family and never discriminates, the
/// same bare-only rule the reveal side applies; an unrecognizable or
/// qualified scrutinee fails closed.
///
/// - An `ErrorVariant` seam compares the pin against the producer-owned
///   exact variant identity. A payload-shaped identity (constructor or
///   string payloads) fails closed: the synthesized pin masks string
///   contents, so payload equality is not provable.
/// - A `ReturnValue` seam compares pins only when its changed expression
///   constructs an exact `Err(Type::Variant)` (the repo-mode analog of
///   the reveal-side `error_construction_variant` gate); without one, the
///   seam's changed value is the SUCCESS payload and the oracle must
///   report an observing Ok arm (RIPR-SPEC-0175): the routing form (no Ok
///   arm) and a payload-ignoring `Ok(_) => ..` arm never observe a changed
///   Ok value, so they do not discriminate (fail closed, under-credit).
///   The error-side comparisons (an `ErrorVariant` seam, and a
///   `ReturnValue` seam on an exact Err construction) are unchanged — the
///   Err guard is the discriminator there.
fn guarded_result_oracle_matches_seam_variant(
    seam: &RepoSeam,
    oracle_text: &str,
    ok_value_observed: Option<bool>,
) -> bool {
    use super::classify::{changed_error_variant, enum_variant_values, exact_error_variant};
    use crate::analysis::seams::RequiredDiscriminator;

    let Some(owner_terminal) = seam.owner().rsplit("::").next() else {
        return false;
    };
    if guarded_oracle_scrutinee_callee(oracle_text) != Some(owner_terminal) {
        return false;
    }
    let pins = guarded_oracle_variant_pins(oracle_text);
    if pins.is_empty() {
        return false;
    }
    match seam.kind() {
        SeamKind::ErrorVariant => {
            let RequiredDiscriminator::ErrorVariant { variant } = seam.required_discriminator()
            else {
                return false;
            };
            let Some(seam_variant) = exact_error_variant(variant) else {
                let candidate = variant.trim();
                let values = enum_variant_values(candidate);
                if values.len() == 1 && values[0] == candidate {
                    return pins.iter().any(|pin| pin == candidate);
                }
                return false;
            };
            pins.iter().any(|pin| pin == &seam_variant)
                && tuple_variant_payload_oracle_matches_seam(seam, oracle_text)
        }
        // The shared identity owner (#6695): an `ok_or(Type::Variant)?`
        // seam compares pins the same way an `Err(Type::Variant)` one does.
        _ => match changed_error_variant(seam.expression()) {
            Some(seam_variant) => {
                pins.iter().any(|pin| pin == &seam_variant)
                    && tuple_variant_payload_oracle_matches_seam(seam, oracle_text)
            }
            // The seam's changed value is the SUCCESS payload: the oracle
            // must report an observing Ok arm (RIPR-SPEC-0175) — `None`
            // (not a guarded fact) and `Some(false)` both fail closed.
            None => ok_value_observed == Some(true),
        },
    }
}

/// The `self` field a field-write seam (`self.count += 1`) requires a test to
/// check (#11398). `None` for every other seam, including a constructed field.
/// A write seam's field is graded only by the exact-read rule, never by the
/// by-name match that a constructed field uses.
fn written_field(seam: &RepoSeam) -> Option<&str> {
    use crate::analysis::seams::RequiredDiscriminator;

    if seam.kind() != SeamKind::FieldConstruction
        || crate::analysis::classify::field_assignment_target(seam.expression()).is_none()
    {
        return None;
    }
    match seam.required_discriminator() {
        RequiredDiscriminator::FieldValue { field } => Some(field.as_str()),
        _ => None,
    }
}

/// Whether `test` never reads after the owner call in a default run: an
/// `#[ignore]`d test does not run, and a `#[should_panic]` test passes by
/// panicking before its read.
fn test_may_skip_read(test: &TestSummary) -> bool {
    is_ignored(test) || is_should_panic(test)
}

fn field_construction_oracle_matches_seam_field(seam: &RepoSeam, oracle_text: &str) -> bool {
    use crate::analysis::seams::RequiredDiscriminator;

    if written_field(seam).is_some() {
        return false;
    }

    let RequiredDiscriminator::FieldValue { field } = seam.required_discriminator() else {
        return false;
    };
    let Some(field_name) = record_field_name(field) else {
        return false;
    };
    let oracle = strip_comments_and_strings(oracle_text);
    code_contains_member_field(&oracle, field_name)
        || code_contains_record_field(&oracle, field_name)
}

fn record_field_name(field_expression: &str) -> Option<&str> {
    let candidate = field_expression
        .split_once(':')
        .map_or(field_expression, |(field, _value)| field)
        .trim();
    let candidate = candidate.strip_prefix("r#").unwrap_or(candidate);
    (!candidate.is_empty()
        && candidate
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
        && candidate
            .chars()
            .next()
            .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic()))
    .then_some(candidate)
}

fn code_contains_member_field(code: &str, field: &str) -> bool {
    let pattern = format!(".{field}");
    code.match_indices(&pattern).any(|(start, _)| {
        let after = code[start + pattern.len()..].chars().next();
        after.is_none_or(|ch| ch != '_' && !ch.is_ascii_alphanumeric())
    })
}

fn code_contains_record_field(code: &str, field: &str) -> bool {
    code.match_indices(field).any(|(start, _)| {
        let before = code[..start].chars().next_back();
        if before.is_some_and(|ch| ch == '_' || ch.is_ascii_alphanumeric()) {
            return false;
        }
        let after = code[start + field.len()..].trim_start();
        after.starts_with(':')
    })
}

/// Returns true when the oracle text structurally pins the same error variant
/// as the seam requires.
///
/// The seam variant is extracted from `RequiredDiscriminator::ErrorVariant { variant }`.
/// The producer normally stores the bare exact variant identity (such as
/// `"MyError::TooLarge"`). Constructor and string-payload expressions that do
/// not contain an enum variant retain their source text and use the dedicated
/// payload matcher instead. The assertion variant is extracted from the oracle
/// text using the same `exact_error_variant` + `enum_variant_values` parsers.
///
/// Fail-closed: exact-variant seams return `false` when either identity cannot
/// be parsed or the identities differ. Payload-shaped seams may use the
/// dedicated exact payload matcher; arbitrary opaque text never establishes a
/// discriminator by itself.
fn error_variant_oracle_matches_seam_variant(seam: &RepoSeam, oracle_text: &str) -> bool {
    use super::classify::{enum_variant_values, exact_error_variant};
    use crate::analysis::seams::RequiredDiscriminator;

    // Extract the seam's required variant from the producer-owned identity.
    let seam_variant = match seam.required_discriminator() {
        RequiredDiscriminator::ErrorVariant { variant } => {
            if let Some(value) = exact_error_variant(variant) {
                value
            } else {
                let candidate = variant.trim();
                let values = enum_variant_values(candidate);
                if values.len() == 1 && values[0] == candidate {
                    candidate.to_string()
                } else {
                    return error_constructor_payload_oracle_matches_seam(variant, oracle_text)
                        || error_string_payload_oracle_matches_seam(variant, oracle_text);
                }
            }
        }
        _ => return false,
    };

    // Credit only when the oracle names the seam's exact variant.
    if !oracle_names_error_variant(oracle_text, &seam_variant) {
        return false;
    }
    // Tuple-variant payload guard (#3244 review): when the seam expression
    // constructs the variant with string-literal payloads, the oracle must
    // construct the same path with the same literals. Naming the variant
    // while observing a different payload is identity coincidence, not
    // discrimination — the token-coincidence over-credit family. Seams whose
    // payloads carry no string literals stay identity-creditable: there is
    // nothing statically observable to compare.
    tuple_variant_payload_oracle_matches_seam(seam, oracle_text)
}

/// True when the oracle assertion text names `seam_variant` as the error
/// variant it pins.
fn oracle_names_error_variant(oracle_text: &str, seam_variant: &str) -> bool {
    use super::classify::{enum_variant_values, exact_error_variant};

    // Extract the variant(s) named in the oracle assertion text.
    // For `assert_eq!(err, MyError::Negative)` there is no `Err(` in the text,
    // so we use `enum_variant_values` directly on the full oracle text.
    let oracle_variants = if oracle_text.contains("Err(") {
        // Classic inline form: `assert_matches!(result, Err(MyError::Negative))`
        match exact_error_variant(oracle_text) {
            Some(v) => vec![v],
            None => return false,
        }
    } else {
        // Unwrap_err-bound form: `assert_eq!(err, MyError::Negative)`
        enum_variant_values(oracle_text)
    };
    oracle_variants.iter().any(|v| v == seam_variant)
}

/// A `ReturnValue` seam whose changed expression constructs an exact
/// `Err(Type::Variant)` is discriminated by an exact error-variant pin that
/// names that same variant, with the same string-payload guard the
/// error-variant seam applies. Fail-closed: an expression that is not a plain
/// `Err(..)` construction (`.ok_or(..)?`, a success payload) or a pin naming
/// another variant never discriminates.
fn exact_err_return_matches_error_pin(seam: &RepoSeam, oracle_text: &str) -> bool {
    use super::classify::exact_error_variant;

    let Some(seam_variant) = exact_error_variant(seam.expression()) else {
        return false;
    };
    oracle_names_error_variant(oracle_text, &seam_variant)
        && tuple_variant_payload_oracle_matches_seam(seam, oracle_text)
}

/// Payload-level complement to the variant-identity route above. The
/// discriminator was already reduced to the bare variant identity, so the
/// seam's own expression is the payload source; the oracle must construct
/// the same constructor path with the same string literals.
fn tuple_variant_payload_oracle_matches_seam(seam: &RepoSeam, oracle_text: &str) -> bool {
    use super::classify::error_constructor_payloads;

    let seam_payloads = error_constructor_payloads(seam.expression());
    if !seam_payloads
        .iter()
        .any(|payload| !payload.string_literals.is_empty())
    {
        return true;
    }
    let oracle_payloads = error_constructor_payloads(oracle_text);
    seam_payloads.iter().any(|seam_payload| {
        !seam_payload.string_literals.is_empty()
            && oracle_payloads.iter().any(|oracle| {
                oracle.path == seam_payload.path
                    && oracle.string_literals == seam_payload.string_literals
            })
    })
}

fn error_constructor_payload_oracle_matches_seam(seam_text: &str, oracle_text: &str) -> bool {
    use super::classify::error_constructor_payloads;

    let seam_payloads = error_constructor_payloads(seam_text);
    let oracle_payloads = error_constructor_payloads(oracle_text);
    seam_payloads
        .iter()
        .filter(|seam| !seam.string_literals.is_empty())
        .any(|seam| {
            oracle_payloads.iter().any(|oracle| {
                oracle.path == seam.path && oracle.string_literals == seam.string_literals
            })
        })
}

fn error_string_payload_oracle_matches_seam(seam_text: &str, oracle_text: &str) -> bool {
    use super::classify::error_result_payload_literal_sets;

    let seam_payloads = error_result_payload_literal_sets(seam_text);
    if seam_payloads.is_empty() {
        return false;
    }
    let oracle_payloads = oracle_string_payload_literal_sets(oracle_text);
    seam_payloads.iter().any(|seam| {
        oracle_payloads
            .iter()
            .any(|oracle| payload_literals_match(seam, oracle))
    })
}

fn oracle_string_payload_literal_sets(oracle_text: &str) -> Vec<Vec<String>> {
    use super::classify::{error_result_payload_literal_sets, rust_string_literals};
    use super::extract::equality_assertion_arguments;

    let result_payloads = error_result_payload_literal_sets(oracle_text);
    if !result_payloads.is_empty() {
        return result_payloads;
    }
    let Some(args) = equality_assertion_arguments(oracle_text) else {
        return Vec::new();
    };
    args.into_iter()
        .take(2)
        .filter_map(|arg| {
            let literals = rust_string_literals(&arg);
            (!literals.is_empty()).then_some(literals)
        })
        .collect()
}

fn payload_literals_match(seam_literals: &[String], oracle_literals: &[String]) -> bool {
    seam_literals
        .iter()
        .filter(|literal| payload_literal_has_fixed_text(literal))
        .any(|seam_literal| {
            oracle_literals
                .iter()
                .filter(|literal| payload_literal_has_fixed_text(literal))
                .any(|oracle_literal| format_literal_matches(seam_literal, oracle_literal))
        })
}

fn format_literal_matches(pattern: &str, observed: &str) -> bool {
    if pattern == observed {
        return true;
    }
    let fragments = format_literal_fixed_fragments(pattern);
    if !fragments.removed_placeholder {
        return false;
    }
    let meaningful = fragments
        .values
        .iter()
        .filter(|fragment| substantial_literal_fragment(fragment))
        .collect::<Vec<_>>();
    if meaningful.is_empty() {
        return false;
    }
    let mut search_from = 0usize;
    let last_index = meaningful.len().saturating_sub(1);
    for (index, fragment) in meaningful.iter().enumerate() {
        let fragment = fragment.as_str();
        let Some(relative) = observed[search_from..].find(fragment) else {
            return false;
        };
        let absolute = search_from + relative;
        if index == 0 && !fragments.starts_with_placeholder && absolute != 0 {
            return false;
        }
        let fragment_end = absolute + fragment.len();
        if index == last_index && !fragments.ends_with_placeholder && fragment_end != observed.len()
        {
            return false;
        }
        search_from = fragment_end;
    }
    true
}

fn payload_literal_has_fixed_text(literal: &str) -> bool {
    format_literal_fixed_fragments(literal)
        .values
        .iter()
        .any(|fragment| fragment.chars().any(|ch| ch.is_alphanumeric()))
}

struct FormatLiteralFragments {
    values: Vec<String>,
    removed_placeholder: bool,
    starts_with_placeholder: bool,
    ends_with_placeholder: bool,
}

fn format_literal_fixed_fragments(pattern: &str) -> FormatLiteralFragments {
    let mut fragments = Vec::new();
    let mut current = String::new();
    let mut removed_placeholder = false;
    let mut saw_content = false;
    let mut starts_with_placeholder = false;
    let mut ends_with_placeholder = false;
    let mut chars = pattern.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '{' => {
                if matches!(chars.peek(), Some('{')) {
                    let _ = chars.next();
                    current.push('{');
                    saw_content = true;
                    ends_with_placeholder = false;
                    continue;
                }
                if !current.is_empty() {
                    fragments.push(std::mem::take(&mut current));
                }
                if !saw_content {
                    starts_with_placeholder = true;
                }
                saw_content = true;
                removed_placeholder = true;
                ends_with_placeholder = true;
                for inner in chars.by_ref() {
                    if inner == '}' {
                        break;
                    }
                }
            }
            '}' => {
                if matches!(chars.peek(), Some('}')) {
                    let _ = chars.next();
                    current.push('}');
                    saw_content = true;
                    ends_with_placeholder = false;
                }
            }
            _ => {
                current.push(ch);
                saw_content = true;
                ends_with_placeholder = false;
            }
        }
    }
    if !current.is_empty() {
        fragments.push(current);
    }
    FormatLiteralFragments {
        values: fragments,
        removed_placeholder,
        starts_with_placeholder,
        ends_with_placeholder,
    }
}

fn substantial_literal_fragment(fragment: &str) -> bool {
    fragment.chars().filter(|ch| ch.is_alphanumeric()).count() >= 8
}

/// Returns true when `oracle_kind` is an acceptable discriminator for `seam_kind`.
///
/// This is the single source of truth for the kind-matching rule used by both
/// the grader (via `oracle_kind_matches_seam`) and the exemplar selector in
/// `output::agent_seam_packets::nearest_strong_test_to_imitate`.
/// Do not duplicate this rule — call this function instead.
pub(crate) fn oracle_kind_matches_seam_kind(seam_kind: SeamKind, oracle_kind: &OracleKind) -> bool {
    match seam_kind {
        SeamKind::PredicateBoundary | SeamKind::MatchArm | SeamKind::FieldConstruction => matches!(
            oracle_kind,
            OracleKind::ExactValue
                | OracleKind::WholeObjectEquality
                | OracleKind::Snapshot
                | OracleKind::RelationalCheck
        ),
        // #3731 review: a guarded Result match observes the owner's
        // returned `Result` — the sink a return-value seam changes through.
        // Kind matching admits it; `oracle_discriminates_seam` applies the
        // exact-variant comparison where the seam carries one.
        SeamKind::ReturnValue => matches!(
            oracle_kind,
            OracleKind::ExactValue
                | OracleKind::WholeObjectEquality
                | OracleKind::Snapshot
                | OracleKind::RelationalCheck
                | OracleKind::GuardedResultMatch
        ),
        // #3731 review: a guarded match's Err-arm pin is an error
        // discriminator; the exact-variant comparison happens in
        // `oracle_discriminates_seam` (a sibling-variant or type-only pin
        // never discriminates).
        SeamKind::ErrorVariant => matches!(
            oracle_kind,
            OracleKind::ExactErrorVariant | OracleKind::GuardedResultMatch
        ),
        SeamKind::SideEffect | SeamKind::CallPresence => {
            matches!(oracle_kind, OracleKind::MockExpectation)
        }
    }
}

fn oracle_kind_matches_seam(seam: &RepoSeam, oracle: &OracleKind) -> bool {
    oracle_kind_matches_seam_kind(seam.kind(), oracle)
}

pub(crate) fn oracle_semantics_for(
    kind: &OracleKind,
    strength: &OracleStrength,
    seam_kind: SeamKind,
) -> OracleSemantics {
    let semantics = oracle_semantics_by_kind(kind, strength, seam_kind);
    // #3731 review (coderabbit): an upgrade suggestion contradicts the
    // strength it accompanies — a STRONG oracle already discriminates, so
    // suggesting an upgrade would tell the reader the evidence is weaker
    // than the field just stated. Medium-or-below keeps the suggestion.
    if matches!(strength, OracleStrength::Strong) {
        return OracleSemantics {
            upgrade_suggestion: None,
            ..semantics
        };
    }
    semantics
}

fn oracle_semantics_by_kind(
    kind: &OracleKind,
    strength: &OracleStrength,
    seam_kind: SeamKind,
) -> OracleSemantics {
    if matches!(strength, OracleStrength::None) {
        return OracleSemantics {
            observes: "no recognized test oracle".to_string(),
            missing: "an observable discriminator for this seam".to_string(),
            upgrade_suggestion: Some(upgrade_suggestion_for_seam(seam_kind).to_string()),
        };
    }

    match kind {
        OracleKind::ExactValue => OracleSemantics {
            observes: "the exact value or value pattern asserted by the test".to_string(),
            missing: "no obvious value-shape discriminator gap under static scope".to_string(),
            upgrade_suggestion: None,
        },
        OracleKind::ExactErrorVariant => OracleSemantics {
            observes: "the exact error variant".to_string(),
            missing: "error payload details if the changed behavior depends on payload".to_string(),
            upgrade_suggestion: Some(
                "assert the payload inside the matched error variant when payload behavior changed"
                    .to_string(),
            ),
        },
        OracleKind::WholeObjectEquality => OracleSemantics {
            observes: "whole output object equality".to_string(),
            missing:
                "field-specific intent only if the whole-object assertion is too broad to review"
                    .to_string(),
            upgrade_suggestion: None,
        },
        OracleKind::Snapshot => OracleSemantics {
            observes: "a snapshot of rendered or debug output".to_string(),
            missing: "a small explicit discriminator if the snapshot is too broad to review"
                .to_string(),
            upgrade_suggestion: Some(
                "add an exact assertion for the changed field or value when the snapshot is broad"
                    .to_string(),
            ),
        },
        OracleKind::RelationalCheck => OracleSemantics {
            observes: "a partial relationship or broad predicate about the result".to_string(),
            missing: "the exact changed value or boundary discriminator".to_string(),
            upgrade_suggestion: Some(upgrade_suggestion_for_seam(seam_kind).to_string()),
        },
        OracleKind::BroadError => OracleSemantics {
            observes: "some error occurred".to_string(),
            missing:
                "the exact error variant or payload that would discriminate the changed behavior"
                    .to_string(),
            upgrade_suggestion: Some(upgrade_suggestion_for_seam(seam_kind).to_string()),
        },
        OracleKind::SmokeOnly => OracleSemantics {
            observes: "the call completed or returned a broad ok/some/none shape".to_string(),
            missing: "the output value, error variant, field, effect, or call discriminator"
                .to_string(),
            upgrade_suggestion: Some(upgrade_suggestion_for_seam(seam_kind).to_string()),
        },
        OracleKind::MockExpectation => OracleSemantics {
            observes: "an expected call, event, state write, or persistence effect".to_string(),
            missing:
                "effect payload, count, order, or state details if those discriminate the behavior"
                    .to_string(),
            upgrade_suggestion: None,
        },
        OracleKind::GuardedResultMatch => OracleSemantics {
            observes: "the callee's returned Result through a guarded Ok/Err match".to_string(),
            missing:
                "an exact error-variant pin in the Err guard when only the error type is pinned"
                    .to_string(),
            upgrade_suggestion: Some(
                "pin the exact error variant inside the Err guard with matches! or assert_matches!"
                    .to_string(),
            ),
        },
        OracleKind::Unknown => OracleSemantics {
            observes: "no recognized concrete oracle shape".to_string(),
            missing: "a discriminator assertion for the seam's observable behavior".to_string(),
            upgrade_suggestion: Some(upgrade_suggestion_for_seam(seam_kind).to_string()),
        },
    }
}

fn upgrade_suggestion_for_seam(seam_kind: SeamKind) -> &'static str {
    match seam_kind {
        SeamKind::PredicateBoundary => {
            "add an exact returned-value assertion at the missing boundary value"
        }
        SeamKind::ErrorVariant => "assert the exact error variant with matches! or assert_matches!",
        SeamKind::ReturnValue => "add an exact returned-value assertion for the changed output",
        SeamKind::FieldConstruction => {
            "assert the specific output field that carries the changed behavior"
        }
        SeamKind::SideEffect => {
            "assert the event, state write, persistence effect, or mock expectation payload"
        }
        SeamKind::MatchArm => "assert the exact enum or value produced by the changed match arm",
        SeamKind::CallPresence => "assert the expected call happened with the relevant arguments",
    }
}

fn related_test_grip<'a>(
    seam: &RepoSeam,
    test: &'a TestSummary,
    reason: RelationReason,
    context: &CompactGripContext<'_>,
    (kind, strength, contradiction): related_tests::BestOracle,
    validated_targets: &mut HashMap<&'a Path, bool>,
) -> RelatedTestGrip {
    let summary = if let Some(contradiction) = contradiction {
        contradiction
    } else if matches!(strength, OracleStrength::None) {
        "no oracle in test body".to_string()
    } else {
        match kind {
            OracleKind::ExactValue => "exact value assertion".to_string(),
            OracleKind::ExactErrorVariant => "exact error-variant assertion".to_string(),
            OracleKind::WholeObjectEquality => "whole-object equality".to_string(),
            OracleKind::Snapshot => "snapshot oracle".to_string(),
            OracleKind::RelationalCheck => "relational check".to_string(),
            OracleKind::BroadError => "is_err / broad-error assertion".to_string(),
            OracleKind::SmokeOnly => "smoke-only assertion".to_string(),
            OracleKind::MockExpectation => "mock expectation".to_string(),
            OracleKind::GuardedResultMatch => "guarded Result match".to_string(),
            OracleKind::Unknown => "no recognised oracle".to_string(),
        }
    };
    let confidence = reason.confidence();
    RelatedTestGrip {
        test_name: test.name.clone(),
        file: test.file.clone(),
        line: test.start_line,
        test_target: test_target_evidence_memo(context, seam, test, reason, validated_targets),
        oracle_kind: kind,
        oracle_strength: strength,
        evidence_summary: summary,
        relation_reason: reason,
        relation_confidence: confidence,
    }
}

#[cfg(test)]
fn test_target_evidence(
    context: &CompactGripContext<'_>,
    seam: &RepoSeam,
    test: &TestSummary,
    relation: RelationReason,
) -> Option<TestTargetEvidence> {
    test_target_evidence_memo(context, seam, test, relation, &mut HashMap::new())
}

/// [`test_target_evidence`] with the caller's per-seam verdicts for
/// `(test file, seam file)` admissions, so the filesystem fingerprint of a
/// shared test file is read once per seam, not once per test.
fn test_target_evidence_memo<'a>(
    context: &CompactGripContext<'_>,
    seam: &RepoSeam,
    test: &'a TestSummary,
    relation: RelationReason,
    validated_targets: &mut HashMap<&'a Path, bool>,
) -> Option<TestTargetEvidence> {
    let index = context.index;
    let function = context.unique_evidence_function(&test.file, &test.name, test.start_line)?;
    let authority = index.workspace_authority.as_ref()?;
    let valid = match validated_targets.get(test.file.as_path()) {
        Some(valid) => *valid,
        None => {
            let valid = context
                .indexed_source_digest(&test.file)
                .is_some_and(|digest| {
                    authority.validates_target_digest(&test.file, seam.file(), &digest)
                });
            validated_targets.insert(test.file.as_path(), valid);
            valid
        }
    };
    if !valid {
        return None;
    }
    Some(TestTargetEvidence::from_index(
        function.id.clone(),
        function.file.clone(),
        function.start_line,
        if function.file == seam.file() || context.is_default_unit_test_file(&function.file) {
            TestKind::InlineUnit
        } else {
            TestKind::Integration
        },
        relation,
        authority.workspace_identity.clone(),
    ))
}

/// The strongest oracle a related test may still claim for `seam`, with the
/// static-contradiction disclosure when that oracle is a statically
/// contradicted equality assertion (#6026): such an oracle keeps Weak
/// strength and names the contradiction in the receipt instead of crediting
/// the mutant's asserted value as observed.
fn best_oracle(
    test: &TestSummary,
    seam: &RepoSeam,
    owner_fn: Option<&FunctionSummary>,
    index: &RustIndex,
    pin_syntax: &crate::analysis::classify::OwnerPinSyntax,
    boolean_pin: Option<&BooleanBoundaryPin<'_>>,
) -> (OracleKind, OracleStrength, Option<String>) {
    let mut best_kind = OracleKind::Unknown;
    let mut best_strength = OracleStrength::None;
    let mut best_contradiction = None;
    let mut best_matching_kind = OracleKind::Unknown;
    let mut best_matching_strength = OracleStrength::None;
    let mut best_matching_contradiction = None;
    for oracle in &test.assertions {
        if !collection_assertion_admitted(seam, oracle, test, index, pin_syntax) {
            continue;
        }
        let (strength, contradiction) =
            effective_oracle_strength(owner_fn, oracle, test, boolean_pin, index, pin_syntax);
        if strength.rank() > best_strength.rank() {
            best_strength = strength.clone();
            best_kind = oracle.kind.clone();
            best_contradiction = contradiction.clone();
        } else if strength.rank() == best_strength.rank()
            && oracle_kind_matches_seam(seam, &oracle.kind)
        {
            best_kind = oracle.kind.clone();
            best_contradiction = contradiction.clone();
        }
        if seam.kind() == SeamKind::FieldConstruction
            && oracle_discriminates_seam_in_test(seam, oracle, test, owner_fn, index, pin_syntax)
            && strength.rank() > best_matching_strength.rank()
        {
            best_matching_strength = strength;
            best_matching_kind = oracle.kind.clone();
            best_matching_contradiction = contradiction;
        }
    }
    if seam.kind() == SeamKind::FieldConstruction && best_matching_strength != OracleStrength::None
    {
        (
            best_matching_kind,
            best_matching_strength,
            best_matching_contradiction,
        )
    } else {
        (best_kind, best_strength, best_contradiction)
    }
}

// --- Argument-extraction helpers, lifted from analysis::classifier and
// trimmed to the shape this module needs. The classifier originals stay
// authoritative for diff-scoped findings; copying keeps the seam path
// from getting tangled in `Probe`-flavored helpers.

fn call_arguments(text: &str, callee: &str) -> Option<Vec<String>> {
    let start = named_call_open_paren_index(text, callee)?;
    let inside = delimited_contents_at(text, start)?;
    Some(split_top_level_commas(&inside))
}

fn named_call_open_paren_index(text: &str, callee: &str) -> Option<usize> {
    text.match_indices(callee).find_map(|(start, _)| {
        let before = text[..start].chars().next_back();
        if before.is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
            return None;
        }
        let after_start = start + callee.len();
        let after = &text[after_start..];
        after.starts_with('(').then_some(after_start)
    })
}

fn delimited_contents_at(text: &str, start: usize) -> Option<String> {
    let bytes = text.as_bytes();
    let open = *bytes.get(start)?;
    let close = match open {
        b'(' => b')',
        b'[' => b']',
        b'{' => b'}',
        _ => return None,
    };
    let open = char::from(open);
    let close = char::from(close);
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    let mut content_start = None;
    for (offset, ch) in text[start..].char_indices() {
        let idx = start + offset;
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            c if c == open => {
                depth += 1;
                if depth == 1 {
                    content_start = Some(idx + ch.len_utf8());
                }
            }
            c if c == close => {
                depth -= 1;
                if depth == 0 {
                    let content_start = content_start?;
                    return text.get(content_start..idx).map(str::to_string);
                }
            }
            _ => {}
        }
    }
    None
}

fn split_top_level_commas(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();
    for ch in input.chars() {
        match ch {
            '(' | '[' | '{' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                out.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    let trailing = current.trim().to_string();
    if !trailing.is_empty() {
        out.push(trailing);
    }
    out
}

/// Extract literal scalar values from a single call argument.
///
/// Identifiers are intentionally rejected: a value-fact reflects a
/// concrete activation seen at the call site. A bare identifier (e.g.,
/// `amount`, `t`) means the test gets the value through a helper, so
/// the activation is opaque and should not be counted as observed.
fn scalar_values(arg: &str) -> Vec<String> {
    let trimmed = arg.trim().trim_end_matches([',', ';']);
    if trimmed.is_empty() {
        return Vec::new();
    }
    // String / char literal.
    if trimmed.starts_with('"') || trimmed.starts_with('\'') {
        return vec![trimmed.to_string()];
    }
    if let Some(value) = suffixed_decimal_integer_value(trimmed) {
        return vec![value];
    }
    // Numeric literal (optionally negative, decimal, with `_` separators).
    let numeric_body = trimmed.strip_prefix('-').unwrap_or(trimmed);
    if !numeric_body.is_empty()
        && numeric_body
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
        && numeric_body
            .chars()
            .all(|c| c.is_ascii_digit() || c == '_' || c == '.')
    {
        return vec![trimmed.to_string()];
    }
    // Path-shaped enum-variant literal, e.g. `Color::Red` or
    // `AuthError::RevokedToken`. Must contain `::` and otherwise be
    // identifier-shaped.
    if trimmed.contains("::")
        && trimmed
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == ':')
    {
        return vec![trimmed.to_string()];
    }
    Vec::new()
}

/// Preserve the explicit type in source while comparing its numeric value
/// through the same reader as a diff boundary. That reader scans expressions,
/// so first establish one whole decimal integer token, optionally negated.
/// Casts, methods, ranges, exponents and computed expressions do not qualify.
fn suffixed_decimal_integer_value(text: &str) -> Option<String> {
    let suffixes: &[&str] = if text.starts_with('-') {
        &["i8", "i16", "i32", "i64", "i128", "isize"]
    } else {
        &[
            "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
        ]
    };
    let unsigned = text.strip_prefix('-').unwrap_or(text);
    let digits = suffixes
        .iter()
        .find_map(|suffix| unsigned.strip_suffix(*suffix))?;
    if !digits.starts_with(|ch: char| ch.is_ascii_digit())
        || !digits
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'_')
    {
        return None;
    }
    numeric_boundary_literal(text)
}

fn sort_value_facts(values: &mut Vec<ValueFact>) {
    values.sort_by(|a, b| {
        a.line
            .cmp(&b.line)
            .then(a.value.cmp(&b.value))
            .then(a.text.cmp(&b.text))
            .then(a.file.cmp(&b.file))
    });
    // File is part of a value's identity (#8389), as in the activation sorter:
    // only exact same-file duplicates collapse, so one test file's fact never
    // hides another file's at the same line.
    values.dedup_by(|a, b| {
        a.file == b.file && a.line == b.line && a.value == b.value && a.text == b.text
    });
}

#[cfg(test)]
mod tests;

//! Availability-qualified input facts a deadline refusal may retain (#10796).
//!
//! Staged foundation: production has no capture registration or reader yet.
//! The registration, reader, and wire labels are compiled only for these tests;
//! the consumer must enable and export them when it adds the timeout refusal.
//! Pipeline publishers currently discard their facts, with TLS and cloning cost.
//!
//! When `ripr check` exhausts its wall-clock budget, the typed
//! `analysis_timeout` refusal must not invent input context: every retained
//! fact has to come from a producer that actually completed and published it
//! before the deadline. Cancellation is cooperative and the budget owner
//! waits a short cancel grace, so a producer can finish inside that window
//! and publish after the deadline; the budget owner therefore stamps the
//! deadline at registration, and a publish at or after the stamp is recorded
//! in a post-deadline mask the renderer reads as `published_after_deadline`
//! instead of `available`. This module is the narrow publication channel
//! between those producers (the diff loader and the scoped intake) and the
//! refusal renderer. It adds no discovery of its own: producers publish facts
//! they already hold, and a run with no registered capture slot (the LSP
//! refresh, library callers, tests) is a no-op.
//!
//! The intended consumer registers a capture slot on the analysis thread,
//! mirroring the cancellation-token thread boundary
//! ([`crate::analysis::cancellation`]): the loader and the intake run on that
//! thread, so a thread-local capture — not a process global — keeps sibling
//! runs in one process from publishing into each other's refusal.

use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Which input source the run analyzed; the requested source mode the CLI
/// selected, echoed back without confusing it with the analysis effort mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InputSource {
    /// `<base>...HEAD` committed-history diff.
    Committed,
    /// Working-tree diff against the merge base.
    WorkingTree,
    /// A caller-supplied `--diff` file (or stdin), where `base` is ignored.
    SuppliedDiff,
    /// An immutable Git candidate subject (exact-tree semantics).
    GitCandidate,
}

impl InputSource {
    /// Stable wire label for the refusal `input_context`.
    #[cfg(test)]
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Committed => "committed",
            Self::WorkingTree => "working-tree",
            Self::SuppliedDiff => "supplied-diff",
            Self::GitCandidate => "git-candidate",
        }
    }
}

/// Input facts one producer completed and published before any deadline.
/// Every field is `None` until its own producer finished it, so the refusal
/// renderer can mark each fact available or name why it is not.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PublishedInputFacts {
    /// The input source the published facts belong to.
    pub(crate) source: Option<InputSource>,
    /// The base the loaded diff actually used (`LoadedDiff::effective_base`).
    pub(crate) effective_base: Option<String>,
    /// The commit id the loader resolved for the base, when it kept one.
    pub(crate) resolved_base_commit: Option<String>,
    /// The merge-base origin the working-tree diff started from.
    pub(crate) merge_base_origin: Option<String>,
    /// Changed file sections the scoped intake admitted for analysis, after
    /// the `--path`/`--exclude` scope and the ripr-artifact partition — the
    /// adapter-facing file list, never the raw Git path list.
    pub(crate) scoped_changed_file_count: Option<usize>,
    /// File sections the scope filter excluded (`out_of_scope` count).
    pub(crate) out_of_scope_file_count: Option<usize>,
}

/// Facts captured for a deadline refusal, with the post-deadline mask. The
/// budget owner stamps the deadline when the run starts; a producer that
/// publishes at or after that stamp (a worker finishing inside the cancel
/// grace) has its fields recorded in `post_deadline`, a presence mask parallel
/// to `facts`, so the renderer can say `published_after_deadline` instead of
/// claiming the fact was available before the deadline.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CapturedInputFacts {
    /// The merged facts, first-writer-wins across producers.
    pub(crate) facts: PublishedInputFacts,
    /// Presence mask of which `facts` fields arrived at or after the deadline.
    pub(crate) post_deadline: PublishedInputFacts,
    /// The diff-load stage (source label and revision facts) completed and
    /// published before the deadline. Distinct from [`Self::intake_published`]
    /// so an interrupted intake reads `not_published_before_deadline` instead
    /// of the false `not_computed_for_source` (#10796 review).
    pub(crate) loader_published: bool,
    /// The scoped-intake stage (file counts) completed and published before
    /// the deadline.
    pub(crate) intake_published: bool,
}

/// Which producer stage a publication belongs to. The renderer uses this to
/// name why a fact is unavailable: only a stage that actually completed may
/// turn a missing fact into `not_computed_for_source`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InputFactsStage {
    /// The diff loader (or the candidate-identity producer): source label and
    /// revision facts.
    DiffLoad,
    /// The scoped intake: adapter-facing file counts.
    ScopedIntake,
}

/// Shared slot the check run registers and the refusal renderer reads. `None`
/// means no producer completed before the deadline — the early-timeout case.
pub(crate) type InputFactsSlot = Arc<Mutex<Option<CapturedInputFacts>>>;

thread_local! {
    static CAPTURE: std::cell::RefCell<Option<(InputFactsSlot, Option<Instant>)>> = const { std::cell::RefCell::new(None) };
}

/// Register `slot` as this thread's publication target for `work`, restoring
/// the previous registration afterwards. `deadline` is the budget owner's
/// stamp of when the run's wall-clock budget expires: publishes at or after
/// it are recorded in the slot's post-deadline mask rather than as
/// before-deadline facts. Without a registration every `publish_*` call is a
/// no-op, so producers stay free on non-CLI surfaces.
#[cfg(test)]
pub(crate) fn with_input_facts_capture<T>(
    slot: InputFactsSlot,
    deadline: Option<Instant>,
    work: impl FnOnce() -> T,
) -> T {
    CAPTURE.with(|capture| {
        let previous = capture.replace(Some((slot, deadline)));
        let result = work();
        capture.replace(previous);
        result
    })
}

/// Merge `facts` into the registered slot. Lock poisoning keeps the poisoned
/// value in place (the run is already failing); publishing never panics. A
/// publish at or after the registered deadline (a producer finishing inside
/// the cancel grace) lands in the slot but is masked post-deadline, and the
/// stage's published flag stays unset: only a before-deadline completion may
/// turn a missing fact into `not_computed_for_source`.
fn publish(facts: PublishedInputFacts, stage: InputFactsStage) {
    CAPTURE.with(|capture| {
        let Some((slot, deadline)) = capture.borrow().as_ref().cloned() else {
            return;
        };
        let post_deadline = deadline.is_some_and(|stamp| Instant::now() >= stamp);
        let Ok(mut published) = slot.lock() else {
            return;
        };
        let merged = published.take().unwrap_or_default();
        // First writer wins on the value; a later publish cannot overwrite a
        // before-deadline fact, and a before-deadline publish cannot un-mark
        // a field an earlier grace-window publish masked. The stage flags are
        // monotone and record before-deadline completions only.
        let mask_source = if post_deadline { facts.source } else { None };
        let mask_effective_base = if post_deadline {
            facts.effective_base.clone()
        } else {
            None
        };
        let mask_resolved_base_commit = if post_deadline {
            facts.resolved_base_commit.clone()
        } else {
            None
        };
        let mask_merge_base_origin = if post_deadline {
            facts.merge_base_origin.clone()
        } else {
            None
        };
        let mask_scoped_changed_file_count = if post_deadline {
            facts.scoped_changed_file_count
        } else {
            None
        };
        let mask_out_of_scope_file_count = if post_deadline {
            facts.out_of_scope_file_count
        } else {
            None
        };
        let published_before_deadline = !post_deadline;
        *published = Some(CapturedInputFacts {
            facts: PublishedInputFacts {
                source: facts.source.or(merged.facts.source),
                effective_base: facts.effective_base.or(merged.facts.effective_base),
                resolved_base_commit: facts
                    .resolved_base_commit
                    .or(merged.facts.resolved_base_commit),
                merge_base_origin: facts.merge_base_origin.or(merged.facts.merge_base_origin),
                scoped_changed_file_count: facts
                    .scoped_changed_file_count
                    .or(merged.facts.scoped_changed_file_count),
                out_of_scope_file_count: facts
                    .out_of_scope_file_count
                    .or(merged.facts.out_of_scope_file_count),
            },
            post_deadline: PublishedInputFacts {
                source: mask_source.or(merged.post_deadline.source),
                effective_base: mask_effective_base.or(merged.post_deadline.effective_base),
                resolved_base_commit: mask_resolved_base_commit
                    .or(merged.post_deadline.resolved_base_commit),
                merge_base_origin: mask_merge_base_origin
                    .or(merged.post_deadline.merge_base_origin),
                scoped_changed_file_count: mask_scoped_changed_file_count
                    .or(merged.post_deadline.scoped_changed_file_count),
                out_of_scope_file_count: mask_out_of_scope_file_count
                    .or(merged.post_deadline.out_of_scope_file_count),
            },
            loader_published: merged.loader_published
                || (published_before_deadline && matches!(stage, InputFactsStage::DiffLoad)),
            intake_published: merged.intake_published
                || (published_before_deadline && matches!(stage, InputFactsStage::ScopedIntake)),
        });
    });
}

/// Publish the requested input source alone — the immutable Git candidate
/// path, whose exact-tree identity is assembled without a [`LoadedDiff`].
/// The candidate-identity producer is the load stage: once it completes, an
/// absent revision fact is genuinely not computed for this source.
pub(crate) fn publish_input_source(source: InputSource) {
    publish(
        PublishedInputFacts {
            source: Some(source),
            ..PublishedInputFacts::default()
        },
        InputFactsStage::DiffLoad,
    );
}

/// Publish the source and revision facts a completed diff load holds. Called
/// by the pipeline at each loader call site, only after the load returned.
pub(crate) fn publish_loaded_diff(source: InputSource, loaded: &crate::analysis::diff::LoadedDiff) {
    publish(
        PublishedInputFacts {
            source: Some(source),
            effective_base: loaded.effective_base.clone(),
            resolved_base_commit: loaded.resolved_base_commit.clone(),
            merge_base_origin: loaded.merge_base_origin.clone(),
            ..PublishedInputFacts::default()
        },
        InputFactsStage::DiffLoad,
    );
}

/// Publish the scoped intake counts the parse and partition produced. These
/// are adapter-facing scoped files; the raw Git path count is a different
/// quantity and is deliberately not published here.
pub(crate) fn publish_scoped_intake(
    scoped_changed_file_count: usize,
    out_of_scope_file_count: usize,
) {
    publish(
        PublishedInputFacts {
            scoped_changed_file_count: Some(scoped_changed_file_count),
            out_of_scope_file_count: Some(out_of_scope_file_count),
            ..PublishedInputFacts::default()
        },
        InputFactsStage::ScopedIntake,
    );
}

/// Read the facts published so far without blocking. The future refusal path
/// can call this after the run fails, when no producer is running.
#[cfg(test)]
pub(crate) fn published(slot: &InputFactsSlot) -> Option<CapturedInputFacts> {
    slot.lock().ok().and_then(|published| published.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loaded(
        base: Option<&str>,
        commit: Option<&str>,
        merge_base: Option<&str>,
    ) -> crate::analysis::diff::LoadedDiff {
        crate::analysis::diff::LoadedDiff {
            text: String::new(),
            effective_base: base.map(str::to_string),
            resolved_base_commit: commit.map(str::to_string),
            merge_base_origin: merge_base.map(str::to_string),
        }
    }

    #[test]
    fn publish_without_capture_is_a_no_op() {
        // Library/LSP runs register no slot: producers stay silent and the
        // refusal renderer simply sees nothing.
        publish_loaded_diff(InputSource::Committed, &loaded(Some("main"), None, None));
        publish_scoped_intake(3, 1);
    }

    #[test]
    fn captured_facts_round_trip_and_merge_across_producers() -> Result<(), String> {
        let slot: InputFactsSlot = Arc::new(Mutex::new(None));
        with_input_facts_capture(slot.clone(), None, || {
            publish_loaded_diff(
                InputSource::WorkingTree,
                &loaded(Some("main"), Some("abc123"), Some("def456")),
            );
            publish_scoped_intake(6, 2);
        });
        let captured = published(&slot).ok_or("published facts must be retained")?;
        let facts = &captured.facts;
        assert_eq!(facts.source, Some(InputSource::WorkingTree));
        assert_eq!(facts.effective_base.as_deref(), Some("main"));
        assert_eq!(facts.resolved_base_commit.as_deref(), Some("abc123"));
        assert_eq!(facts.merge_base_origin.as_deref(), Some("def456"));
        assert_eq!(facts.scoped_changed_file_count, Some(6));
        assert_eq!(facts.out_of_scope_file_count, Some(2));
        assert!(captured.loader_published, "the load stage published");
        assert!(captured.intake_published, "the intake stage published");
        assert_eq!(
            captured.post_deadline,
            PublishedInputFacts::default(),
            "no deadline was registered, so nothing may be masked post-deadline"
        );
        Ok(())
    }

    #[test]
    fn interrupted_intake_keeps_its_deadline_reason_distinct() -> Result<(), String> {
        // The loader completing must not speak for the intake: an absent
        // count is an unfinished producer, never `not_computed_for_source`.
        let slot: InputFactsSlot = Arc::new(Mutex::new(None));
        with_input_facts_capture(slot.clone(), None, || {
            publish_loaded_diff(
                InputSource::WorkingTree,
                &loaded(Some("main"), Some("abc123"), None),
            );
            // The intake never runs: the budget fires inside it.
        });
        let captured = published(&slot).ok_or("loader facts must be retained")?;
        assert!(captured.loader_published);
        assert!(
            !captured.intake_published,
            "the intake must not be marked published when it never ran"
        );
        assert_eq!(captured.facts.scoped_changed_file_count, None);
        // And the inverse stage split: an intake-only completion must not
        // speak for the loader either.
        let counts_slot: InputFactsSlot = Arc::new(Mutex::new(None));
        with_input_facts_capture(counts_slot.clone(), None, || {
            publish_scoped_intake(4, 1);
        });
        let counts = published(&counts_slot).ok_or("intake facts must be retained")?;
        assert!(
            !counts.loader_published,
            "the load stage must not be marked published when it never ran"
        );
        assert!(counts.intake_published);
        assert_eq!(counts.facts.effective_base, None);
        Ok(())
    }

    #[test]
    fn facts_published_at_or_after_the_deadline_stamp_are_masked() -> Result<(), String> {
        // A producer finishing inside the cancel grace publishes after the
        // budget fired: those facts read `published_after_deadline`, never
        // `available: true`.
        let grace_slot: InputFactsSlot = Arc::new(Mutex::new(None));
        with_input_facts_capture(
            grace_slot.clone(),
            Some(Instant::now() - std::time::Duration::from_mins(1)),
            || {
                publish_loaded_diff(
                    InputSource::WorkingTree,
                    &loaded(Some("main"), Some("abc123"), None),
                );
            },
        );
        let grace = published(&grace_slot).ok_or("grace-window facts must be captured")?;
        assert_eq!(grace.facts.effective_base.as_deref(), Some("main"));
        assert!(
            !grace.loader_published,
            "a post-deadline completion must not read as a before-deadline stage publication"
        );
        assert_eq!(
            grace.post_deadline.effective_base.as_deref(),
            Some("main"),
            "a publish after the deadline stamp must be masked, not available"
        );
        assert_eq!(
            grace.post_deadline.resolved_base_commit.as_deref(),
            Some("abc123")
        );
        // Before-deadline control: the same publish under a future deadline
        // lands unmasked and reads available.
        let live_slot: InputFactsSlot = Arc::new(Mutex::new(None));
        with_input_facts_capture(
            live_slot.clone(),
            Some(Instant::now() + std::time::Duration::from_mins(1)),
            || {
                publish_loaded_diff(
                    InputSource::WorkingTree,
                    &loaded(Some("main"), Some("abc123"), None),
                );
            },
        );
        let live = published(&live_slot).ok_or("live facts must be captured")?;
        assert_eq!(
            live.post_deadline,
            PublishedInputFacts::default(),
            "a publish before the deadline stamp must stay available"
        );
        Ok(())
    }

    #[test]
    fn early_deadline_leaves_the_slot_empty() {
        // A producer that never completes publishes nothing, not zero facts.
        let slot: InputFactsSlot = Arc::new(Mutex::new(None));
        let seen = with_input_facts_capture(slot.clone(), None, || published(&slot));
        assert_eq!(seen, None);
    }

    #[test]
    fn source_labels_stay_distinct_from_analysis_mode() {
        for (source, label) in [
            (InputSource::Committed, "committed"),
            (InputSource::WorkingTree, "working-tree"),
            (InputSource::SuppliedDiff, "supplied-diff"),
            (InputSource::GitCandidate, "git-candidate"),
        ] {
            assert_eq!(source.as_str(), label);
        }
    }
}

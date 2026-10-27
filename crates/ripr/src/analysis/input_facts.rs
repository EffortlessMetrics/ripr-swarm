//! Availability-qualified input facts a deadline refusal may retain (#10796).
//!
//! When `ripr check` exhausts its wall-clock budget, the typed
//! `analysis_timeout` refusal must not invent input context: every retained
//! fact has to come from a producer that actually completed and published it
//! before the deadline. This module is the narrow publication channel between
//! those producers (the diff loader and the scoped intake) and the refusal
//! renderer. It adds no discovery of its own: producers publish facts they
//! already hold, and a run with no registered capture slot (the LSP refresh,
//! library callers, tests) is a no-op.
//!
//! The capture slot is registered on the analysis thread by the check budget
//! owner, mirroring the cancellation-token thread boundary
//! ([`crate::analysis::cancellation`]): the loader and the intake run on that
//! thread, so a thread-local capture — not a process global — keeps sibling
//! runs in one process from publishing into each other's refusal.

use std::sync::{Arc, Mutex};

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

/// Shared slot the check run registers and the refusal renderer reads. `None`
/// means no producer completed before the deadline — the early-timeout case.
pub(crate) type InputFactsSlot = Arc<Mutex<Option<PublishedInputFacts>>>;

thread_local! {
    static CAPTURE: std::cell::RefCell<Option<InputFactsSlot>> = const { std::cell::RefCell::new(None) };
}

/// Register `slot` as this thread's publication target for `work`, restoring
/// the previous registration afterwards. Without a registration every
/// `publish_*` call is a no-op, so producers stay free on non-CLI surfaces.
pub(crate) fn with_input_facts_capture<T>(slot: InputFactsSlot, work: impl FnOnce() -> T) -> T {
    CAPTURE.with(|capture| {
        let previous = capture.replace(Some(slot));
        let result = work();
        capture.replace(previous);
        result
    })
}

/// Merge `facts` into the registered slot. Lock poisoning keeps the poisoned
/// value in place (the run is already failing); publishing never panics.
fn publish(facts: PublishedInputFacts) {
    CAPTURE.with(|capture| {
        let Some(slot) = capture.borrow().as_ref().cloned() else {
            return;
        };
        let Ok(mut published) = slot.lock() else {
            return;
        };
        let merged = published.take().unwrap_or_default();
        *published = Some(PublishedInputFacts {
            source: facts.source.or(merged.source),
            effective_base: facts.effective_base.or(merged.effective_base),
            resolved_base_commit: facts.resolved_base_commit.or(merged.resolved_base_commit),
            merge_base_origin: facts.merge_base_origin.or(merged.merge_base_origin),
            scoped_changed_file_count: facts
                .scoped_changed_file_count
                .or(merged.scoped_changed_file_count),
            out_of_scope_file_count: facts
                .out_of_scope_file_count
                .or(merged.out_of_scope_file_count),
        });
    });
}

/// Publish the requested input source alone — the immutable Git candidate
/// path, whose exact-tree identity is assembled without a [`LoadedDiff`].
pub(crate) fn publish_input_source(source: InputSource) {
    publish(PublishedInputFacts {
        source: Some(source),
        ..PublishedInputFacts::default()
    });
}

/// Publish the source and revision facts a completed diff load holds. Called
/// by the pipeline at each loader call site, only after the load returned.
pub(crate) fn publish_loaded_diff(source: InputSource, loaded: &crate::analysis::diff::LoadedDiff) {
    publish(PublishedInputFacts {
        source: Some(source),
        effective_base: loaded.effective_base.clone(),
        resolved_base_commit: loaded.resolved_base_commit.clone(),
        merge_base_origin: loaded.merge_base_origin.clone(),
        ..PublishedInputFacts::default()
    });
}

/// Publish the scoped intake counts the parse and partition produced. These
/// are adapter-facing scoped files; the raw Git path count is a different
/// quantity and is deliberately not published here.
pub(crate) fn publish_scoped_intake(scoped_changed_file_count: usize, out_of_scope_file_count: usize) {
    publish(PublishedInputFacts {
        scoped_changed_file_count: Some(scoped_changed_file_count),
        out_of_scope_file_count: Some(out_of_scope_file_count),
        ..PublishedInputFacts::default()
    });
}

/// Read the facts published so far without blocking. The refusal path calls
/// this only after the run already failed; no producer runs at this point.
pub(crate) fn published(slot: &InputFactsSlot) -> Option<PublishedInputFacts> {
    slot.lock().ok().and_then(|published| published.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loaded(base: Option<&str>, commit: Option<&str>, merge_base: Option<&str>) -> crate::analysis::diff::LoadedDiff {
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
    fn captured_facts_round_trip_and_merge_across_producers() {
        let slot: InputFactsSlot = Arc::new(Mutex::new(None));
        with_input_facts_capture(slot.clone(), || {
            publish_loaded_diff(
                InputSource::WorkingTree,
                &loaded(Some("main"), Some("abc123"), Some("def456")),
            );
            publish_scoped_intake(6, 2);
        });
        let facts = published(&slot).expect("published facts must be retained");
        assert_eq!(facts.source, Some(InputSource::WorkingTree));
        assert_eq!(facts.effective_base.as_deref(), Some("main"));
        assert_eq!(facts.resolved_base_commit.as_deref(), Some("abc123"));
        assert_eq!(facts.merge_base_origin.as_deref(), Some("def456"));
        assert_eq!(facts.scoped_changed_file_count, Some(6));
        assert_eq!(facts.out_of_scope_file_count, Some(2));
    }

    #[test]
    fn early_deadline_leaves_the_slot_empty() {
        // A producer that never completes publishes nothing: the early
        // timeout case must read as "no input facts", not as zero facts.
        let slot: InputFactsSlot = Arc::new(Mutex::new(None));
        let seen = with_input_facts_capture(slot.clone(), || published(&slot));
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

//! Passive progress events for the shared check application pipeline.
//!
//! The producer reports only boundaries it has reached. Elapsed time is
//! observational and never becomes part of analysis identity or output.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Instant;

/// Closed stage vocabulary for a check invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnalysisProgressStage {
    LoadingInput,
    Analyzing,
    BuildingOutput,
    Completed,
    Cancelled,
    Failed,
}

/// Mode identity without a checkout path or source text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnalysisProgressScope {
    Diff,
    Worktree,
    Repo,
}

/// One bounded, path-free progress observation. The current producer has no
/// honest denominator, so both counters remain `None`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnalysisProgressEvent {
    pub stage: AnalysisProgressStage,
    pub scope: AnalysisProgressScope,
    pub completed_units: Option<u64>,
    pub total_units: Option<u64>,
    pub elapsed_ms: u64,
}

/// Optional, best-effort observer. Implementations must return quickly.
/// A sink failure, including a panic, cannot change the check result.
pub trait AnalysisProgressSink {
    fn emit(&self, event: AnalysisProgressEvent);
}

pub(super) struct ProgressRun<'a> {
    sink: Option<&'a dyn AnalysisProgressSink>,
    scope: AnalysisProgressScope,
    started: Instant,
    terminal: bool,
}

impl<'a> ProgressRun<'a> {
    pub(super) fn new(
        sink: Option<&'a dyn AnalysisProgressSink>,
        scope: AnalysisProgressScope,
    ) -> Self {
        Self {
            sink,
            scope,
            started: Instant::now(),
            terminal: false,
        }
    }

    pub(super) fn emit(&self, stage: AnalysisProgressStage) {
        let Some(sink) = self.sink else {
            return;
        };
        let event = AnalysisProgressEvent {
            stage,
            scope: self.scope,
            completed_units: None,
            total_units: None,
            elapsed_ms: self.started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        };
        let _ = catch_unwind(AssertUnwindSafe(|| sink.emit(event)));
    }

    pub(super) fn complete(&mut self) {
        self.terminal = true;
        self.emit(AnalysisProgressStage::Completed);
    }
}

impl Drop for ProgressRun<'_> {
    fn drop(&mut self) {
        if !self.terminal {
            let stage = if crate::analysis::cancellation::checkpoint().is_err() {
                AnalysisProgressStage::Cancelled
            } else {
                AnalysisProgressStage::Failed
            };
            self.emit(stage);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::cancellation::{self, AnalysisAbortKind, AnalysisCancellationToken};
    use std::sync::Mutex;

    #[derive(Default)]
    struct Recorder(Mutex<Vec<AnalysisProgressEvent>>);

    impl AnalysisProgressSink for Recorder {
        fn emit(&self, event: AnalysisProgressEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    #[test]
    fn cancelled_run_has_a_single_non_success_terminal() {
        let recorder = Recorder::default();
        let token = AnalysisCancellationToken::new();
        cancellation::with_token(&token, || {
            let progress = ProgressRun::new(Some(&recorder), AnalysisProgressScope::Diff);
            progress.emit(AnalysisProgressStage::LoadingInput);
            assert!(token.cancel(AnalysisAbortKind::Cancelled));
        });
        let stages = recorder
            .0
            .lock()
            .unwrap()
            .iter()
            .map(|event| event.stage)
            .collect::<Vec<_>>();
        assert_eq!(
            stages,
            [
                AnalysisProgressStage::LoadingInput,
                AnalysisProgressStage::Cancelled
            ]
        );
    }
}

//! Passive progress events for the shared check application pipeline.
//!
//! The producer reports only boundaries it has reached. Elapsed time is
//! observational and never becomes part of analysis identity or output.
//! CLI and LSP projections consume these events; they do not invent stages.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Instant;

/// Closed stage vocabulary for a check invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AnalysisProgressStage {
    LoadingInput,
    Analyzing,
    BuildingOutput,
    Completed,
    Cancelled,
    Failed,
}

impl AnalysisProgressStage {
    /// Stable, path-free token used by every projection of this event.
    pub(crate) const fn token(self) -> &'static str {
        match self {
            Self::LoadingInput => "loading_input",
            Self::Analyzing => "analyzing",
            Self::BuildingOutput => "building_output",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    pub(crate) const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }

    pub(crate) const fn is_success_terminal(self) -> bool {
        matches!(self, Self::Completed)
    }
}

/// Mode identity without a checkout path or source text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AnalysisProgressScope {
    Diff,
    Worktree,
    Repo,
}

impl AnalysisProgressScope {
    pub(crate) const fn token(self) -> &'static str {
        match self {
            Self::Diff => "diff",
            Self::Worktree => "worktree",
            Self::Repo => "repo",
        }
    }
}

/// One bounded, path-free progress observation. The current producer has no
/// honest denominator, so both counters remain `None`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AnalysisProgressEvent {
    pub stage: AnalysisProgressStage,
    pub scope: AnalysisProgressScope,
    pub completed_units: Option<u64>,
    pub total_units: Option<u64>,
    pub elapsed_ms: u64,
}

/// Optional, best-effort observer. Implementations must return quickly.
/// A sink failure, including a panic, cannot change the check result.
pub(crate) trait AnalysisProgressSink {
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
    use crate::app::{CheckInput, Mode, OutputFormat};
    use crate::config::RiprConfig;
    use std::path::PathBuf;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Recorder(Mutex<Vec<AnalysisProgressEvent>>);

    impl AnalysisProgressSink for Recorder {
        fn emit(&self, event: AnalysisProgressEvent) {
            let mut events = match self.0.lock() {
                Ok(events) => events,
                Err(poisoned) => poisoned.into_inner(),
            };
            events.push(event);
        }
    }

    impl Recorder {
        fn events(&self) -> Vec<AnalysisProgressEvent> {
            match self.0.lock() {
                Ok(events) => events.clone(),
                Err(poisoned) => poisoned.into_inner().clone(),
            }
        }
    }

    fn sample_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/sample")
    }

    fn sample_diff_input() -> CheckInput {
        let root = sample_root();
        CheckInput {
            root: root.clone(),
            diff_file: Some(root.join("example.diff")),
            mode: Mode::Draft,
            format: OutputFormat::Json,
            ..CheckInput::default()
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
        let events = recorder.events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].stage, AnalysisProgressStage::LoadingInput);
        assert_eq!(events[1].stage, AnalysisProgressStage::Cancelled);
        assert!(!events[1].stage.is_success_terminal());
        assert_eq!(
            events
                .iter()
                .filter(|event| event.stage.is_terminal())
                .count(),
            1
        );
    }

    #[test]
    fn tokens_are_closed_and_path_free() {
        for stage in [
            AnalysisProgressStage::LoadingInput,
            AnalysisProgressStage::Analyzing,
            AnalysisProgressStage::BuildingOutput,
            AnalysisProgressStage::Completed,
            AnalysisProgressStage::Cancelled,
            AnalysisProgressStage::Failed,
        ] {
            let token = stage.token();
            assert!(!token.contains('/'));
            assert!(!token.contains('\\'));
            assert!(!token.contains('%'));
            assert!(token.chars().all(|ch| ch.is_ascii_lowercase() || ch == '_'));
        }
        for scope in [
            AnalysisProgressScope::Diff,
            AnalysisProgressScope::Worktree,
            AnalysisProgressScope::Repo,
        ] {
            let token = scope.token();
            assert!(token.chars().all(|ch| ch.is_ascii_lowercase()));
        }
    }

    #[test]
    fn check_progress_reports_real_boundaries_without_invented_totals() -> Result<(), String> {
        let recorder = Recorder::default();
        let output = crate::app::check_workspace_with_config_and_progress(
            sample_diff_input(),
            &RiprConfig::default(),
            Some(&recorder),
        )?;
        assert!(
            !output.findings.is_empty(),
            "sample diff must execute analysis"
        );
        let events = recorder.events();
        assert_eq!(
            events.iter().map(|event| event.stage).collect::<Vec<_>>(),
            [
                AnalysisProgressStage::LoadingInput,
                AnalysisProgressStage::Analyzing,
                AnalysisProgressStage::BuildingOutput,
                AnalysisProgressStage::Completed,
            ]
        );
        assert!(events.iter().all(|event| {
            event.scope == AnalysisProgressScope::Diff
                && event.completed_units.is_none()
                && event.total_units.is_none()
        }));
        assert!(
            events
                .windows(2)
                .all(|pair| pair[0].elapsed_ms <= pair[1].elapsed_ms)
        );
        Ok(())
    }

    #[test]
    fn check_progress_failure_has_one_terminal_and_no_false_completion() {
        let recorder = Recorder::default();
        let mut input = sample_diff_input();
        input.diff_file = Some(input.root.join("absent-progress-input.diff"));
        assert!(
            crate::app::check_workspace_with_config_and_progress(
                input,
                &RiprConfig::default(),
                Some(&recorder)
            )
            .is_err()
        );
        let events = recorder.events();
        assert_eq!(
            events.last().map(|event| event.stage),
            Some(AnalysisProgressStage::Failed)
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| event.stage.is_terminal())
                .count(),
            1
        );
        assert!(
            !events
                .iter()
                .any(|event| event.stage == AnalysisProgressStage::Completed)
        );
    }

    #[test]
    fn progress_sink_panic_cannot_change_analysis_result() -> Result<(), String> {
        struct BrokenSink;
        impl AnalysisProgressSink for BrokenSink {
            fn emit(&self, _: AnalysisProgressEvent) {
                std::panic::panic_any("instrumentation failure");
            }
        }
        let input = sample_diff_input();
        let baseline =
            crate::app::check_workspace_with_config(input.clone(), &RiprConfig::default())?;
        let observed = crate::app::check_workspace_with_config_and_progress(
            input,
            &RiprConfig::default(),
            Some(&BrokenSink),
        )?;
        assert_eq!(observed.findings.len(), baseline.findings.len());
        assert_eq!(observed.summary.probes, baseline.summary.probes);
        Ok(())
    }

    #[test]
    fn omitting_the_sink_removes_stage_evidence() -> Result<(), String> {
        let recorder = Recorder::default();
        let with_sink = crate::app::check_workspace_with_config_and_progress(
            sample_diff_input(),
            &RiprConfig::default(),
            Some(&recorder),
        )?;
        let without_sink = crate::app::check_workspace_with_config_and_progress(
            sample_diff_input(),
            &RiprConfig::default(),
            None,
        )?;
        assert_eq!(with_sink.findings.len(), without_sink.findings.len());
        assert!(!recorder.events().is_empty());
        let silent = Recorder::default();
        let _ = crate::app::check_workspace_with_config_and_progress(
            sample_diff_input(),
            &RiprConfig::default(),
            None,
        )?;
        assert!(
            silent.events().is_empty(),
            "a missing producer sink must leave the observer empty"
        );
        Ok(())
    }
}

//! CLI projection of producer-owned analysis progress onto stderr.
//!
//! This module does not invent stages, percentages, or analysis counters. It
//! renders [`crate::app::AnalysisProgressEvent`] records as bounded stderr
//! lines, throttles heartbeats, and restores the terminal on drop.

use crate::app::{
    AnalysisProgressEvent, AnalysisProgressScope, AnalysisProgressSink, AnalysisProgressStage,
};
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Documented heartbeat and visibility policy for CLI progress.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProgressPolicy {
    /// Interactive terminals hold the first update until this duration so a
    /// sub-threshold run does not flash. Non-TTY / CI emits immediately.
    pub min_visible: Duration,
    /// First heartbeat is eligible only after the current stage has been
    /// active this long.
    pub first_heartbeat: Duration,
    /// Minimum spacing between heartbeats.
    pub heartbeat_every: Duration,
    /// Hard ceiling so a blocked stage cannot grow logs without bound.
    pub max_heartbeats: u32,
}

impl ProgressPolicy {
    pub(crate) const STANDARD: Self = Self {
        min_visible: Duration::from_millis(250),
        first_heartbeat: Duration::from_secs(2),
        heartbeat_every: Duration::from_secs(2),
        max_heartbeats: 16,
    };
}

/// Elapsed classes used in heartbeat lines. Raw millisecond counts never
/// appear in the projection.
pub(crate) fn elapsed_class(elapsed: Duration) -> Option<&'static str> {
    const CLASSES: &[(u64, &str)] = &[
        (2_000, "2s"),
        (5_000, "5s"),
        (10_000, "10s"),
        (30_000, "30s"),
        (60_000, "1m"),
        (120_000, "2m"),
        (300_000, "5m"),
        (600_000, "10m"),
    ];
    let ms = elapsed.as_millis().min(u128::from(u64::MAX)) as u64;
    CLASSES
        .iter()
        .rev()
        .find(|(floor, _)| ms >= *floor)
        .map(|(_, label)| *label)
}

pub(crate) const fn stage_token(stage: AnalysisProgressStage) -> &'static str {
    match stage {
        AnalysisProgressStage::LoadingInput => "loading_input",
        AnalysisProgressStage::Analyzing => "analyzing",
        AnalysisProgressStage::BuildingOutput => "building_output",
        AnalysisProgressStage::Completed => "completed",
        AnalysisProgressStage::Cancelled => "cancelled",
        AnalysisProgressStage::Failed => "failed",
    }
}

pub(crate) const fn scope_token(scope: AnalysisProgressScope) -> &'static str {
    match scope {
        AnalysisProgressScope::Diff => "diff",
        AnalysisProgressScope::Worktree => "worktree",
        AnalysisProgressScope::Repo => "repo",
    }
}

pub(crate) const fn stage_is_terminal(stage: AnalysisProgressStage) -> bool {
    matches!(
        stage,
        AnalysisProgressStage::Completed
            | AnalysisProgressStage::Cancelled
            | AnalysisProgressStage::Failed
    )
}

pub(crate) const fn stage_is_success_terminal(stage: AnalysisProgressStage) -> bool {
    matches!(stage, AnalysisProgressStage::Completed)
}

pub(crate) fn format_stage_line(
    stage: AnalysisProgressStage,
    scope: AnalysisProgressScope,
) -> String {
    format!(
        "ripr progress: {} [{}]",
        stage_token(stage),
        scope_token(scope)
    )
}

pub(crate) fn format_heartbeat_line(stage: AnalysisProgressStage, class: &str) -> String {
    format!(
        "ripr progress: {} still active after {class}",
        stage_token(stage)
    )
}

/// Fail closed: a projected line may only carry closed tokens and elapsed
/// classes. Absolute paths, percents, ETAs, and source text are rejected.
pub(crate) fn progress_line_is_safe(line: &str) -> bool {
    if !line.starts_with("ripr progress: ") {
        return false;
    }
    if line.len() > 80 {
        return false;
    }
    if line.contains('/') || line.contains('\\') {
        return false;
    }
    let lowered = line.to_ascii_lowercase();
    if lowered.contains('%') || lowered.contains("eta") {
        return false;
    }
    let payload = &line["ripr progress: ".len()..];
    payload.chars().all(|ch| {
        ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '_' | ' ' | '[' | ']' | ':')
    })
}

struct Projection {
    writer: Box<dyn Write + Send>,
    tty: bool,
    policy: ProgressPolicy,
    current_stage: Option<AnalysisProgressStage>,
    terminal: bool,
    visible: bool,
    started: Instant,
    stage_started: Instant,
    last_heartbeat: Option<Instant>,
    heartbeat_count: u32,
    in_place: bool,
}

struct SinkInner {
    state: Mutex<Projection>,
    stop: Arc<AtomicBool>,
    heartbeat: Mutex<Option<JoinHandle<()>>>,
}

pub(crate) struct CliProgressSink {
    inner: Arc<SinkInner>,
}

impl CliProgressSink {
    pub(crate) fn for_stderr(tty: bool, policy: ProgressPolicy) -> Self {
        Self::with_writer(Box::new(io::stderr()), tty, policy)
    }

    pub(crate) fn with_writer(
        writer: Box<dyn Write + Send>,
        tty: bool,
        policy: ProgressPolicy,
    ) -> Self {
        let now = Instant::now();
        Self {
            inner: Arc::new(SinkInner {
                state: Mutex::new(Projection {
                    writer,
                    tty,
                    policy,
                    current_stage: None,
                    terminal: false,
                    visible: !tty,
                    started: now,
                    stage_started: now,
                    last_heartbeat: None,
                    heartbeat_count: 0,
                    in_place: false,
                }),
                stop: Arc::new(AtomicBool::new(false)),
                heartbeat: Mutex::new(None),
            }),
        }
    }

    fn lock_state(inner: &SinkInner) -> std::sync::MutexGuard<'_, Projection> {
        match inner.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn write_line(projection: &mut Projection, line: &str) {
        if !progress_line_is_safe(line) {
            return;
        }
        let rendered = if projection.tty {
            projection.in_place = true;
            format!("\r{line:<48}")
        } else {
            let mut owned = line.to_string();
            owned.push('\n');
            owned
        };
        let _ = projection.writer.write_all(rendered.as_bytes());
        let _ = projection.writer.flush();
    }

    fn finish_in_place(projection: &mut Projection) {
        if projection.in_place {
            let _ = projection.writer.write_all(b"\n");
            let _ = projection.writer.flush();
            projection.in_place = false;
        }
    }

    fn emit_event(&self, event: AnalysisProgressEvent) {
        let mut projection = Self::lock_state(&self.inner);
        if projection.terminal {
            return;
        }
        let now = Instant::now();
        if projection.current_stage.is_none() {
            projection.started = now;
        }
        if projection.current_stage != Some(event.stage) {
            projection.stage_started = now;
            projection.last_heartbeat = None;
        }
        projection.current_stage = Some(event.stage);

        if stage_is_terminal(event.stage) {
            projection.terminal = true;
            let too_short = projection.tty
                && stage_is_success_terminal(event.stage)
                && now.duration_since(projection.started) < projection.policy.min_visible;
            if too_short {
                Self::finish_in_place(&mut projection);
                return;
            }
            projection.visible = true;
            let line = format_stage_line(event.stage, event.scope);
            Self::write_line(&mut projection, &line);
            Self::finish_in_place(&mut projection);
            return;
        }

        if projection.tty && now.duration_since(projection.started) < projection.policy.min_visible
        {
            return;
        }
        projection.visible = true;
        let line = format_stage_line(event.stage, event.scope);
        Self::write_line(&mut projection, &line);
    }

    fn heartbeat_tick(inner: &SinkInner) {
        let mut projection = Self::lock_state(inner);
        if projection.terminal || !projection.visible {
            return;
        }
        if projection.heartbeat_count >= projection.policy.max_heartbeats {
            return;
        }
        let Some(stage) = projection.current_stage else {
            return;
        };
        if stage_is_terminal(stage) {
            return;
        }
        let now = Instant::now();
        let stage_age = now.duration_since(projection.stage_started);
        if stage_age < projection.policy.first_heartbeat {
            return;
        }
        if let Some(last) = projection.last_heartbeat
            && now.duration_since(last) < projection.policy.heartbeat_every
        {
            return;
        }
        let Some(class) = elapsed_class(stage_age) else {
            return;
        };
        let line = format_heartbeat_line(stage, class);
        Self::write_line(&mut projection, &line);
        projection.last_heartbeat = Some(now);
        projection.heartbeat_count = projection.heartbeat_count.saturating_add(1);
    }

    fn start_heartbeat_thread(&self) {
        let mut slot = match self.inner.heartbeat.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if slot.is_some() || self.inner.stop.load(Ordering::Relaxed) {
            return;
        }
        let inner = Arc::clone(&self.inner);
        *slot = Some(thread::spawn(move || {
            while !inner.stop.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(50));
                if inner.stop.load(Ordering::Relaxed) {
                    break;
                }
                CliProgressSink::heartbeat_tick(&inner);
            }
        }));
    }

    fn stop_heartbeat(&self) {
        self.inner.stop.store(true, Ordering::Relaxed);
        let mut slot = match self.inner.heartbeat.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(handle) = slot.take() {
            let _ = handle.join();
        }
    }
}

impl AnalysisProgressSink for CliProgressSink {
    fn emit(&self, event: AnalysisProgressEvent) {
        self.emit_event(event);
        if stage_is_terminal(event.stage) {
            self.stop_heartbeat();
        } else {
            self.start_heartbeat_thread();
        }
    }
}

impl Drop for CliProgressSink {
    fn drop(&mut self) {
        self.stop_heartbeat();
        let mut projection = Self::lock_state(&self.inner);
        Self::finish_in_place(&mut projection);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    #[derive(Clone)]
    struct Buffer(Arc<StdMutex<Vec<u8>>>);

    impl Buffer {
        fn new() -> Self {
            Self(Arc::new(StdMutex::new(Vec::new())))
        }

        fn text(&self) -> String {
            let bytes = match self.0.lock() {
                Ok(guard) => guard.clone(),
                Err(poisoned) => poisoned.into_inner().clone(),
            };
            String::from_utf8_lossy(&bytes).into_owned()
        }
    }

    impl Write for Buffer {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            match self.0.lock() {
                Ok(mut guard) => guard.extend_from_slice(buf),
                Err(poisoned) => poisoned.into_inner().extend_from_slice(buf),
            };
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct BrokenWriter;

    impl Write for BrokenWriter {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("progress sink failed"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn event(stage: AnalysisProgressStage) -> AnalysisProgressEvent {
        AnalysisProgressEvent {
            stage,
            scope: AnalysisProgressScope::Diff,
            completed_units: None,
            total_units: None,
            elapsed_ms: 0,
        }
    }

    fn non_tty_policy() -> ProgressPolicy {
        ProgressPolicy {
            min_visible: Duration::ZERO,
            first_heartbeat: Duration::from_secs(2),
            heartbeat_every: Duration::from_millis(50),
            max_heartbeats: 4,
        }
    }

    #[test]
    fn non_tty_stage_lines_are_newline_delimited_and_path_free() {
        let buffer = Buffer::new();
        let sink = CliProgressSink::with_writer(Box::new(buffer.clone()), false, non_tty_policy());
        sink.emit(event(AnalysisProgressStage::LoadingInput));
        sink.emit(event(AnalysisProgressStage::Analyzing));
        sink.emit(event(AnalysisProgressStage::BuildingOutput));
        sink.emit(event(AnalysisProgressStage::Completed));
        let text = buffer.text();
        assert!(!text.contains('\u{1b}'));
        assert!(!text.contains('\r'));
        assert!(!text.contains('%'));
        assert!(!text.to_ascii_lowercase().contains("eta"));
        assert!(text.contains("ripr progress: loading_input [diff]"));
        assert!(text.contains("ripr progress: analyzing [diff]"));
        assert!(text.contains("ripr progress: building_output [diff]"));
        assert!(text.contains("ripr progress: completed [diff]"));
        assert_eq!(text.matches("ripr progress: completed").count(), 1);
        for line in text.lines() {
            assert!(progress_line_is_safe(line), "unsafe progress line: {line}");
        }
    }

    #[test]
    fn unknown_totals_never_render_as_percentage_or_eta() {
        let buffer = Buffer::new();
        let sink = CliProgressSink::with_writer(Box::new(buffer.clone()), false, non_tty_policy());
        sink.emit(AnalysisProgressEvent {
            stage: AnalysisProgressStage::Analyzing,
            scope: AnalysisProgressScope::Diff,
            completed_units: Some(3),
            total_units: None,
            elapsed_ms: 4_000,
        });
        let text = buffer.text();
        assert!(!text.contains('%'));
        assert!(!text.contains("3/"));
        assert!(!text.to_ascii_lowercase().contains("eta"));
        assert!(text.contains("analyzing [diff]"));
    }

    #[test]
    fn tty_short_run_does_not_flash_completed() {
        let buffer = Buffer::new();
        let policy = ProgressPolicy {
            min_visible: Duration::from_secs(30),
            first_heartbeat: Duration::from_secs(30),
            heartbeat_every: Duration::from_secs(30),
            max_heartbeats: 1,
        };
        let sink = CliProgressSink::with_writer(Box::new(buffer.clone()), true, policy);
        sink.emit(event(AnalysisProgressStage::LoadingInput));
        sink.emit(event(AnalysisProgressStage::Completed));
        drop(sink);
        assert!(
            !buffer.text().contains("ripr progress:"),
            "sub-threshold TTY run must not spray stages: {}",
            buffer.text()
        );
    }

    #[test]
    fn tty_and_non_tty_share_stage_tokens() {
        let tty_buf = Buffer::new();
        let ci_buf = Buffer::new();
        let policy = ProgressPolicy {
            min_visible: Duration::ZERO,
            first_heartbeat: Duration::from_secs(30),
            heartbeat_every: Duration::from_secs(30),
            max_heartbeats: 1,
        };
        let tty = CliProgressSink::with_writer(Box::new(tty_buf.clone()), true, policy);
        let ci = CliProgressSink::with_writer(Box::new(ci_buf.clone()), false, policy);
        for sink in [&tty, &ci] {
            sink.emit(event(AnalysisProgressStage::LoadingInput));
            sink.emit(event(AnalysisProgressStage::Analyzing));
            sink.emit(event(AnalysisProgressStage::Completed));
        }
        drop(tty);
        drop(ci);
        for token in ["loading_input", "analyzing", "completed"] {
            assert!(tty_buf.text().contains(token));
            assert!(ci_buf.text().contains(token));
        }
        assert!(tty_buf.text().contains('\r'));
        assert!(!ci_buf.text().contains('\r'));
    }

    #[test]
    fn failure_terminal_never_emits_completed() {
        let buffer = Buffer::new();
        let sink = CliProgressSink::with_writer(Box::new(buffer.clone()), false, non_tty_policy());
        sink.emit(event(AnalysisProgressStage::LoadingInput));
        sink.emit(event(AnalysisProgressStage::Failed));
        sink.emit(event(AnalysisProgressStage::Completed));
        let text = buffer.text();
        assert!(text.contains("ripr progress: failed [diff]"));
        assert!(!text.contains("completed"));
        assert_eq!(text.matches("ripr progress: failed").count(), 1);
    }

    #[test]
    fn rendering_failure_is_isolated() {
        let sink = CliProgressSink::with_writer(Box::new(BrokenWriter), false, non_tty_policy());
        sink.emit(event(AnalysisProgressStage::Analyzing));
        sink.emit(event(AnalysisProgressStage::Completed));
    }

    #[test]
    fn heartbeat_is_throttled_and_bounded() {
        let buffer = Buffer::new();
        let sink = CliProgressSink::with_writer(Box::new(buffer.clone()), false, non_tty_policy());
        sink.emit(event(AnalysisProgressStage::Analyzing));
        let deadline = Instant::now() + Duration::from_millis(2500);
        while Instant::now() < deadline {
            if buffer.text().matches("still active after").count() >= 2 {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        let during = buffer.text();
        assert!(
            during.contains("still active after"),
            "blocked stage must heartbeat, got: {during:?}"
        );
        sink.emit(event(AnalysisProgressStage::Completed));
        drop(sink);
        let heartbeats = during.matches("still active after").count();
        assert!(heartbeats <= 4, "heartbeat ceiling exceeded: {heartbeats}");
        assert!(!during.contains('%'));
        assert!(!during.contains('\u{1b}'));
    }

    #[test]
    fn unsafe_constructed_lines_are_rejected() {
        assert!(!progress_line_is_safe(
            "ripr progress: analyzing /tmp/secret.rs"
        ));
        assert!(!progress_line_is_safe("ripr progress: analyzing 40%"));
        assert!(!progress_line_is_safe("ripr progress: analyzing eta 12s"));
        assert!(!progress_line_is_safe("warning: something else"));
        assert!(progress_line_is_safe("ripr progress: analyzing [diff]"));
        assert!(progress_line_is_safe(
            "ripr progress: analyzing still active after 2s"
        ));
    }

    #[test]
    fn elapsed_class_is_bucketed_not_raw_millis() {
        assert_eq!(elapsed_class(Duration::from_millis(1999)), None);
        assert_eq!(elapsed_class(Duration::from_millis(2000)), Some("2s"));
        assert_eq!(elapsed_class(Duration::from_millis(4999)), Some("2s"));
        assert_eq!(elapsed_class(Duration::from_secs(5)), Some("5s"));
        assert_eq!(elapsed_class(Duration::from_secs(90)), Some("1m"));
    }

    #[test]
    fn projection_tokens_cover_the_closed_producer_vocabulary() {
        assert_eq!(
            stage_token(AnalysisProgressStage::LoadingInput),
            "loading_input"
        );
        assert_eq!(stage_token(AnalysisProgressStage::Analyzing), "analyzing");
        assert_eq!(
            stage_token(AnalysisProgressStage::BuildingOutput),
            "building_output"
        );
        assert_eq!(stage_token(AnalysisProgressStage::Completed), "completed");
        assert_eq!(stage_token(AnalysisProgressStage::Cancelled), "cancelled");
        assert_eq!(stage_token(AnalysisProgressStage::Failed), "failed");
        assert_eq!(scope_token(AnalysisProgressScope::Diff), "diff");
        assert_eq!(scope_token(AnalysisProgressScope::Worktree), "worktree");
        assert_eq!(scope_token(AnalysisProgressScope::Repo), "repo");
        assert!(!stage_is_terminal(AnalysisProgressStage::Analyzing));
        assert!(stage_is_terminal(AnalysisProgressStage::Cancelled));
        assert!(stage_is_success_terminal(AnalysisProgressStage::Completed));
        assert!(!stage_is_success_terminal(AnalysisProgressStage::Failed));
        assert!(!stage_is_success_terminal(AnalysisProgressStage::Cancelled));
        assert_eq!(
            format_stage_line(
                AnalysisProgressStage::Analyzing,
                AnalysisProgressScope::Worktree
            ),
            "ripr progress: analyzing [worktree]"
        );
        assert_eq!(
            format_stage_line(
                AnalysisProgressStage::Completed,
                AnalysisProgressScope::Repo
            ),
            "ripr progress: completed [repo]"
        );
    }

    #[test]
    fn cancelled_terminal_never_emits_completed() {
        let buffer = Buffer::new();
        let sink = CliProgressSink::with_writer(Box::new(buffer.clone()), false, non_tty_policy());
        sink.emit(event(AnalysisProgressStage::Analyzing));
        sink.emit(event(AnalysisProgressStage::Cancelled));
        sink.emit(event(AnalysisProgressStage::Completed));
        let text = buffer.text();
        assert!(text.contains("ripr progress: cancelled [diff]"));
        assert!(!text.contains("completed"));
        assert_eq!(text.matches("ripr progress: cancelled").count(), 1);
    }

    #[test]
    fn tty_short_failure_still_projects_failed() {
        let buffer = Buffer::new();
        let policy = ProgressPolicy {
            min_visible: Duration::from_secs(30),
            first_heartbeat: Duration::from_secs(30),
            heartbeat_every: Duration::from_secs(30),
            max_heartbeats: 1,
        };
        let sink = CliProgressSink::with_writer(Box::new(buffer.clone()), true, policy);
        sink.emit(event(AnalysisProgressStage::LoadingInput));
        sink.emit(event(AnalysisProgressStage::Failed));
        drop(sink);
        let text = buffer.text();
        assert!(
            text.contains("failed"),
            "non-success terminals must remain visible on a short TTY run: {text}"
        );
        assert!(!text.contains("completed"));
    }
}

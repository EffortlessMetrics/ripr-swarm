//! Availability-aware input context and bounded recovery for the
//! `analysis_timeout` refusal (#10796).
//!
//! When the `ripr check` wall-clock budget expires, the typed refusal stays
//! honest — exit 2, empty findings, zeroed summary, non-consumable — but the
//! caller needs to know what was asked, what the producers actually resolved
//! before the deadline, and one concrete bounded next action. This module is
//! the projection authority for that context: the CLI stashes a
//! [`TimeoutRefusalProjection`] when the budget fires, and the refusal
//! renderer consumes it with [`take_stashed`]. No new discovery runs here;
//! resolved facts come only from [`PublishedInputFacts`] the producers
//! completed and published before the deadline.

use crate::analysis::{CapturedInputFacts, InputSource};
use crate::app::CheckInput;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::Duration;

/// The requested input source the CLI established before analysis, with the
/// explicitness of its source flags. `source` is the input mode
/// (committed/working-tree/supplied-diff/git-candidate), never the analysis
/// effort mode.
pub(crate) struct RequestedSourceContext {
    pub(crate) source: InputSource,
    /// `--worktree` was on argv.
    pub(crate) worktree_explicit: bool,
    /// `--committed` was on argv.
    pub(crate) committed_explicit: bool,
    /// `--candidate-tree` value, when a candidate subject was requested.
    pub(crate) candidate_tree: Option<String>,
    /// `--candidate-base` value, when one was requested.
    pub(crate) candidate_base: Option<String>,
}

/// Invocation authority plus producer-published facts for one timed-out run.
pub(crate) struct TimeoutRefusalProjection {
    root: PathBuf,
    base: Option<String>,
    diff_file: Option<PathBuf>,
    analysis_mode: &'static str,
    scope_include: Vec<String>,
    scope_exclude: Vec<String>,
    requested: RequestedSourceContext,
    expired_budget: Duration,
    json_output: bool,
    /// The explicit `--format VALUE` spelling from argv, preserved verbatim;
    /// `None` when the format was `--json` or the default human renderer.
    format_explicit: Option<String>,
    /// Explicit `--output-dir` destination, when the fanout invocation named one.
    output_dir: Option<PathBuf>,
    published: Option<CapturedInputFacts>,
}

/// Build the projection from the invocation snapshot, the requested source
/// selection, the expired budget, the explicit format/output choices argv
/// made, and whatever the producers published before the deadline (`None` =
/// early timeout, no producer completed).
pub(crate) fn build(
    input: &CheckInput,
    requested: RequestedSourceContext,
    expired_budget: Duration,
    json_output: bool,
    format_explicit: Option<String>,
    output_dir: Option<PathBuf>,
    published: Option<CapturedInputFacts>,
) -> TimeoutRefusalProjection {
    TimeoutRefusalProjection {
        root: input.root.clone(),
        base: input.base.clone(),
        diff_file: input.diff_file.clone(),
        analysis_mode: input.mode.as_str(),
        scope_include: input.scope.include().to_vec(),
        scope_exclude: input.scope.exclude().to_vec(),
        requested,
        expired_budget,
        json_output,
        format_explicit,
        output_dir,
        published,
    }
}

/// One resolved fact: available with its value, or unavailable with the
/// reason. An early deadline reads `not_published_before_deadline`; a fact
/// the producer never computes for this source reads `not_computed_for_source`;
/// a fact published at or after the deadline stamp (a producer finishing in
/// the cancel grace) reads `published_after_deadline` — never `available`.
fn fact_string(value: Option<&str>, published_any: bool, post_deadline: bool) -> Value {
    if post_deadline {
        return json!({
            "available": false,
            "reason": "published_after_deadline",
        });
    }
    match value {
        Some(value) => json!({"available": true, "value": value}),
        None if published_any => json!({
            "available": false,
            "reason": "not_computed_for_source",
        }),
        None => json!({
            "available": false,
            "reason": "not_published_before_deadline",
        }),
    }
}

fn fact_count(
    value: Option<usize>,
    published_any: bool,
    post_deadline: bool,
    producer: &str,
) -> Value {
    if post_deadline {
        return json!({
            "available": false,
            "reason": "published_after_deadline",
        });
    }
    match value {
        Some(value) => json!({"available": true, "value": value, "producer": producer}),
        None if published_any => json!({
            "available": false,
            "reason": "not_computed_for_source",
        }),
        None => json!({
            "available": false,
            "reason": "not_published_before_deadline",
        }),
    }
}

/// What a retry re-reads: named from the requested source, never silently
/// pinned to the timed-out run's snapshot. The committed-history diff is the
/// `<base>...HEAD` range, so both endpoints are live refs; a working-tree
/// diff re-reads live bytes and re-resolves the merge base, which reads the
/// base ref and `HEAD`.
fn rereads(self_: &TimeoutRefusalProjection) -> Vec<&'static str> {
    match self_.requested.source {
        InputSource::Committed => {
            let mut reads = vec!["base-ref", "head-ref"];
            if self_.base.is_none() {
                reads.push("default-base-resolution");
            }
            reads
        }
        InputSource::WorkingTree => {
            let mut reads = vec!["worktree-bytes"];
            if self_.base.is_none() {
                // Default-base resolution covers the merge-base re-resolution
                // (base and HEAD) under one label.
                reads.push("default-base-resolution");
            } else {
                // The merge base of the explicit base and HEAD is
                // re-resolved on retry, so both live refs are named.
                reads.push("base-ref");
                reads.push("head-ref");
            }
            reads
        }
        InputSource::SuppliedDiff => vec!["diff-file-bytes"],
        // Exact-tree object identities are immutable; a retry re-resolves
        // the same objects rather than live bytes.
        InputSource::GitCandidate => vec!["none (immutable git objects)"],
    }
}

fn retry_note(self_: &TimeoutRefusalProjection) -> &'static str {
    match self_.requested.source {
        InputSource::Committed | InputSource::WorkingTree => {
            "the retry re-reads live refs and/or worktree bytes; it is not pinned to the timed-out run's snapshot"
        }
        InputSource::SuppliedDiff => {
            "the retry re-reads the named diff file; edit the file in place to retry different input"
        }
        InputSource::GitCandidate => {
            "the retry re-resolves the named immutable git objects; the input identity is unchanged"
        }
    }
}

/// The bounded next budget: twice the expired budget, never "no limit".
fn next_budget_secs(expired: Duration) -> u64 {
    expired.as_secs().saturating_mul(2).max(1)
}

impl TimeoutRefusalProjection {
    /// The literal, shell-safe retry command rebuilt from the invocation
    /// authority that actually ran. Explicit source/base/scope/format/output
    /// choices are preserved; auto-selected sources keep their
    /// auto-selection instead of fabricating flags that would silently
    /// narrow or broaden the diff. (`ripr check` has no `--config` or
    /// `--lang` switch — config is discovered from the root and the language
    /// set comes from `ripr.toml` — so preserving `--root` retains the
    /// discovery anchor and there is nothing else to carry.)
    pub(crate) fn recovery_command(&self) -> String {
        let mut argv: Vec<String> = vec!["ripr".to_string(), "check".to_string()];
        argv.push(format!(
            "--root {}",
            crate::agent::loop_commands::shell_arg(&crate::output::path::display_path(&self.root),)
        ));
        match self.requested.source {
            InputSource::SuppliedDiff => {
                if let Some(diff_file) = &self.diff_file {
                    argv.push(format!(
                        "--diff {}",
                        crate::agent::loop_commands::shell_arg(&diff_file.to_string_lossy())
                    ));
                }
            }
            InputSource::WorkingTree => {
                if self.requested.worktree_explicit {
                    argv.push("--worktree".to_string());
                }
                if let Some(base) = &self.base {
                    argv.push(format!(
                        "--base {}",
                        crate::agent::loop_commands::shell_arg(base)
                    ));
                }
            }
            InputSource::Committed => {
                if self.requested.committed_explicit {
                    argv.push("--committed".to_string());
                }
                if let Some(base) = &self.base {
                    argv.push(format!(
                        "--base {}",
                        crate::agent::loop_commands::shell_arg(base)
                    ));
                }
            }
            InputSource::GitCandidate => {
                if let Some(tree) = &self.requested.candidate_tree {
                    argv.push(format!(
                        "--candidate-tree {}",
                        crate::agent::loop_commands::shell_arg(tree)
                    ));
                }
                if let Some(base) = &self.requested.candidate_base {
                    argv.push(format!(
                        "--candidate-base {}",
                        crate::agent::loop_commands::shell_arg(base)
                    ));
                }
            }
        }
        for include in &self.scope_include {
            argv.push(format!(
                "--path {}",
                crate::agent::loop_commands::shell_arg(include)
            ));
        }
        for exclude in &self.scope_exclude {
            argv.push(format!(
                "--exclude {}",
                crate::agent::loop_commands::shell_arg(exclude)
            ));
        }
        if let Some(value) = &self.format_explicit {
            argv.push(format!(
                "--format {}",
                crate::agent::loop_commands::shell_arg(value)
            ));
        } else if self.json_output {
            argv.push("--json".to_string());
        }
        if let Some(output_dir) = &self.output_dir {
            argv.push(format!(
                "--output-dir {}",
                crate::agent::loop_commands::shell_arg(&output_dir.to_string_lossy())
            ));
        }
        argv.push(format!(
            "--timeout {}",
            next_budget_secs(self.expired_budget)
        ));
        argv.join(" ")
    }

    /// The `input_context` object for the refusal envelope. Same-keys:
    /// `requested`, `resolved`, and `recovery` always appear; every resolved
    /// fact carries its own availability instead of being omitted.
    pub(crate) fn input_context_value(&self) -> Value {
        let published_any = self.published.is_some();
        let facts = self.published.as_ref().map(|captured| &captured.facts);
        let mask = self
            .published
            .as_ref()
            .map(|captured| &captured.post_deadline);
        let requested = json!({
            "root": crate::output::path::display_path(&self.root),
            "source": self.requested.source.as_str(),
            "base": self.base,
            "diff_file": self.diff_file.as_ref().map(|path| path.to_string_lossy()),
            "analysis_mode": self.analysis_mode,
            "scope": {
                "include": self.scope_include.clone(),
                "exclude": self.scope_exclude.clone(),
            },
        });
        let resolved = json!({
            "effective_base": fact_string(
                facts.and_then(|facts| facts.effective_base.as_deref()),
                published_any,
                mask.is_some_and(|mask| mask.effective_base.is_some()),
            ),
            "base_commit": fact_string(
                facts.and_then(|facts| facts.resolved_base_commit.as_deref()),
                published_any,
                mask.is_some_and(|mask| mask.resolved_base_commit.is_some()),
            ),
            "merge_base": fact_string(
                facts.and_then(|facts| facts.merge_base_origin.as_deref()),
                published_any,
                mask.is_some_and(|mask| mask.merge_base_origin.is_some()),
            ),
            "scoped_changed_files": fact_count(
                facts.and_then(|facts| facts.scoped_changed_file_count),
                published_any,
                mask.is_some_and(|mask| mask.scoped_changed_file_count.is_some()),
                "scoped-diff-intake",
            ),
            "out_of_scope_files": fact_count(
                facts.and_then(|facts| facts.out_of_scope_file_count),
                published_any,
                mask.is_some_and(|mask| mask.out_of_scope_file_count.is_some()),
                "scoped-diff-intake",
            ),
        });
        let recovery = json!({
            "kind": "retry_with_larger_budget",
            "command": self.recovery_command(),
            "budget_seconds": next_budget_secs(self.expired_budget),
            "rereads": rereads(self),
            "note": retry_note(self),
        });
        json!({
            "requested": requested,
            "resolved": resolved,
            "recovery": recovery,
        })
    }

    /// Human-format rendering of the same projection, appended to the stderr
    /// refusal for native CLI consumers.
    pub(crate) fn human_recovery_text(&self) -> String {
        let mut lines = vec![format!(
            "ripr: requested input: source={} root={}",
            self.requested.source.as_str(),
            crate::output::path::display_path(&self.root),
        )];
        if let Some(base) = &self.base {
            lines.push(format!("ripr: requested base: {base}"));
        }
        match &self.published {
            Some(captured) => {
                let facts = &captured.facts;
                let mask = &captured.post_deadline;
                let mut resolved = Vec::new();
                if facts.effective_base.is_some() && mask.effective_base.is_none() {
                    resolved.push(format!("effective base {}", facts.effective_base.as_deref().unwrap_or_default()));
                }
                if facts.resolved_base_commit.is_some()
                    && mask.resolved_base_commit.is_none()
                {
                    resolved.push(format!(
                        "base commit {}",
                        facts.resolved_base_commit.as_deref().unwrap_or_default()
                    ));
                }
                if facts.merge_base_origin.is_some() && mask.merge_base_origin.is_none() {
                    resolved.push(format!(
                        "merge base {}",
                        facts.merge_base_origin.as_deref().unwrap_or_default()
                    ));
                }
                if facts.scoped_changed_file_count.is_some()
                    && mask.scoped_changed_file_count.is_none()
                {
                    let count = facts.scoped_changed_file_count.unwrap_or(0);
                    let out = facts.out_of_scope_file_count.unwrap_or(0);
                    resolved.push(format!(
                        "scoped changed files {count} ({out} out of scope)"
                    ));
                }
                if resolved.is_empty() {
                    lines.push(
                        "ripr: resolved before the deadline: none (no input producer had published)"
                            .to_string(),
                    );
                } else {
                    lines.push(format!(
                        "ripr: resolved before the deadline: {}",
                        resolved.join(", ")
                    ));
                }
                // Honesty: a producer that finished inside the cancel grace
                // published too late to be claimed as resolved context.
                let mut late = Vec::new();
                if mask.effective_base.is_some() {
                    late.push("effective base");
                }
                if mask.resolved_base_commit.is_some() {
                    late.push("base commit");
                }
                if mask.merge_base_origin.is_some() {
                    late.push("merge base");
                }
                if mask.scoped_changed_file_count.is_some() {
                    late.push("scoped intake counts");
                }
                if !late.is_empty() {
                    lines.push(format!(
                        "ripr: published after the deadline (not retained): {}",
                        late.join(", ")
                    ));
                }
            }
            None => lines.push(
                "ripr: resolved before the deadline: none (no input producer had published when the budget fired)"
                    .to_string(),
            ),
        }
        lines.push(format!("ripr: bounded retry: {}", self.recovery_command()));
        lines.push(format!("ripr: note: {}", retry_note(self)));
        lines.join("\n")
    }
}

thread_local! {
    static STASHED: std::cell::RefCell<Option<TimeoutRefusalProjection>> = const { std::cell::RefCell::new(None) };
}

/// Stash the projection for the refusal renderer on this thread. The CLI
/// calls this once when the budget fires; [`take_stashed`] consumes it, so a
/// stale projection can never leak into an unrelated refusal on this thread.
pub(crate) fn stash(projection: TimeoutRefusalProjection) {
    STASHED.with(|stashed| {
        stashed.replace(Some(projection));
    });
}

/// Consume the stashed projection, if any.
pub(crate) fn take_stashed() -> Option<TimeoutRefusalProjection> {
    STASHED.with(|stashed| stashed.take())
}

/// Drop any stashed projection (command start, so one CLI invocation cannot
/// inherit another's context).
pub(crate) fn clear_stashed() {
    STASHED.with(|stashed| {
        stashed.take();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::PublishedInputFacts;
    use crate::app::{Mode, OutputFormat};

    fn input(root: &str, base: Option<&str>, diff_file: Option<&str>) -> CheckInput {
        CheckInput {
            root: PathBuf::from(root),
            base: base.map(str::to_string),
            diff_file: diff_file.map(PathBuf::from),
            mode: Mode::Draft,
            format: OutputFormat::Json,
            ..CheckInput::default()
        }
    }

    fn requested(source: InputSource) -> RequestedSourceContext {
        RequestedSourceContext {
            source,
            worktree_explicit: false,
            committed_explicit: false,
            candidate_tree: None,
            candidate_base: None,
        }
    }

    fn published_full() -> PublishedInputFacts {
        PublishedInputFacts {
            source: Some(InputSource::Committed),
            effective_base: Some("origin/main".to_string()),
            resolved_base_commit: Some("abc123".to_string()),
            merge_base_origin: None,
            scoped_changed_file_count: Some(6),
            out_of_scope_file_count: Some(2),
        }
    }

    /// Facts that arrived before the deadline stamp: nothing masked.
    fn captured(facts: PublishedInputFacts) -> CapturedInputFacts {
        CapturedInputFacts {
            facts,
            post_deadline: PublishedInputFacts::default(),
        }
    }

    /// Facts a producer published at or after the deadline stamp (a worker
    /// finishing inside the cancel grace): every field masked post-deadline.
    fn captured_post_deadline(facts: PublishedInputFacts) -> CapturedInputFacts {
        CapturedInputFacts {
            post_deadline: facts.clone(),
            facts,
        }
    }

    #[test]
    fn late_timeout_marks_published_facts_available() {
        let projection = build(
            &input("repo", Some("origin/main"), None),
            requested(InputSource::Committed),
            Duration::from_mins(1),
            true,
            None,
            None,
            Some(captured(published_full())),
        );
        let value = projection.input_context_value();
        assert_eq!(
            value["resolved"]["effective_base"],
            json!({"available": true, "value": "origin/main"})
        );
        assert_eq!(
            value["resolved"]["base_commit"],
            json!({"available": true, "value": "abc123"})
        );
        // A committed-history diff never computes a merge base: explicit
        // unavailability, not a missing key.
        assert_eq!(
            value["resolved"]["merge_base"],
            json!({"available": false, "reason": "not_computed_for_source"})
        );
        assert_eq!(
            value["resolved"]["scoped_changed_files"],
            json!({"available": true, "value": 6, "producer": "scoped-diff-intake"})
        );
        assert_eq!(value["recovery"]["budget_seconds"], json!(120));
        assert_eq!(
            value["recovery"]["command"],
            json!("ripr check --root repo --base origin/main --json --timeout 120")
        );
    }

    #[test]
    fn early_timeout_marks_every_fact_unpublished() {
        let projection = build(
            &input("repo", Some("origin/main"), None),
            requested(InputSource::Committed),
            Duration::from_mins(1),
            true,
            None,
            None,
            None,
        );
        let value = projection.input_context_value();
        for pointer in [
            "/resolved/effective_base",
            "/resolved/base_commit",
            "/resolved/merge_base",
            "/resolved/scoped_changed_files",
            "/resolved/out_of_scope_files",
        ] {
            assert_eq!(
                value
                    .pointer(pointer)
                    .and_then(|fact| fact.get("available")),
                Some(&Value::Bool(false)),
                "{pointer} must be unavailable: {value}"
            );
            assert_eq!(
                value.pointer(pointer).and_then(|fact| fact.get("reason")),
                Some(&Value::String("not_published_before_deadline".to_string())),
                "{pointer} must name the deadline reason: {value}"
            );
        }
    }

    #[test]
    fn requested_echo_distinguishes_source_from_analysis_mode() {
        let projection = build(
            &input("repo", Some("origin/main"), None),
            requested(InputSource::WorkingTree),
            Duration::from_secs(30),
            false,
            None,
            None,
            None,
        );
        let value = projection.input_context_value();
        assert_eq!(value["requested"]["source"], json!("working-tree"));
        assert_eq!(value["requested"]["analysis_mode"], json!("draft"));
        assert_eq!(value["requested"]["base"], json!("origin/main"));
    }

    #[test]
    fn quoted_roots_and_refs_stay_shell_safe() {
        let projection = build(
            &input("my repo", Some("feature/it''s"), None),
            requested(InputSource::Committed),
            Duration::from_mins(1),
            true,
            None,
            None,
            None,
        );
        let command = projection.recovery_command();
        // The host's quoting form is owned and pinned by
        // `loop_commands::shell_arg_for`; here we pin that the recovery
        // command routes the values through it (the #4378 human_path_text
        // pattern).
        let windows = cfg!(windows);
        let root = crate::agent::loop_commands::shell_arg_for("my repo", windows);
        let base = crate::agent::loop_commands::shell_arg_for("feature/it''s", windows);
        assert!(
            command.contains(&format!("--root {root}")),
            "root with a space must be quoted: {command}"
        );
        assert!(
            command.contains(&format!("--base {base}")),
            "base with a quote must be escaped: {command}"
        );
        assert!(command.ends_with("--timeout 120"), "bounded: {command}");
    }

    #[test]
    fn supplied_diff_retry_repeats_the_diff_and_never_broadens() {
        let projection = build(
            &input("repo", None, Some("change.diff")),
            requested(InputSource::SuppliedDiff),
            Duration::from_secs(45),
            false,
            None,
            None,
            // The loader published only the source label for the supplied
            // diff: its producer never computes base facts.
            Some(captured(PublishedInputFacts {
                source: Some(InputSource::SuppliedDiff),
                ..PublishedInputFacts::default()
            })),
        );
        let command = projection.recovery_command();
        assert!(
            command.contains("--diff change.diff"),
            "supplied diff must be preserved: {command}"
        );
        assert!(
            !command.contains("--base"),
            "base is ignored for a supplied diff and must not appear: {command}"
        );
        let value = projection.input_context_value();
        assert_eq!(value["recovery"]["rereads"], json!(["diff-file-bytes"]));
        assert_eq!(
            value["resolved"]["effective_base"],
            json!({
                "available": false,
                "reason": "not_computed_for_source",
            })
        );
    }

    #[test]
    fn explicit_source_flags_and_scope_are_preserved() -> Result<(), String> {
        let mut check_input = input("repo", Some("main"), None);
        check_input.scope = crate::analysis::DiffScopeFilter::new(
            vec!["crates/api".to_string()],
            vec!["crates/api/tests".to_string()],
        )
        .map_err(|err| format!("scope patterns are valid: {err}"))?;
        let mut requested_context = requested(InputSource::Committed);
        requested_context.committed_explicit = true;
        let projection = build(
            &check_input,
            requested_context,
            Duration::from_secs(10),
            false,
            None,
            None,
            None,
        );
        let command = projection.recovery_command();
        assert!(
            command.contains("--committed"),
            "explicit --committed must be preserved: {command}"
        );
        assert!(
            command.contains("--path crates/api"),
            "include scope must be preserved: {command}"
        );
        assert!(
            command.contains("--exclude crates/api/tests"),
            "exclude scope must be preserved: {command}"
        );
        Ok(())
    }

    #[test]
    fn auto_selected_worktree_states_rereads_instead_of_fake_flags() {
        // A working-tree source selected by default (no --worktree flag) keeps
        // no fabricated flag and names the live-byte re-read.
        let projection = build(
            &input("repo", None, None),
            requested(InputSource::WorkingTree),
            Duration::from_mins(1),
            false,
            None,
            None,
            None,
        );
        let command = projection.recovery_command();
        assert!(
            !command.contains("--worktree"),
            "auto selection must not fabricate --worktree: {command}"
        );
        assert!(
            !command.contains("--committed"),
            "auto selection must not fabricate --committed: {command}"
        );
        let value = projection.input_context_value();
        assert_eq!(
            value["recovery"]["rereads"],
            json!(["worktree-bytes", "default-base-resolution"])
        );
    }

    #[test]
    fn human_text_covers_late_and_early_timeouts() {
        let late = build(
            &input("repo", Some("origin/main"), None),
            requested(InputSource::Committed),
            Duration::from_mins(1),
            false,
            None,
            None,
            Some(captured(published_full())),
        );
        let text = late.human_recovery_text();
        assert!(text.contains("source=committed"), "{text}");
        assert!(text.contains("effective base origin/main"), "{text}");
        assert!(
            text.contains("scoped changed files 6 (2 out of scope)"),
            "{text}"
        );
        assert!(text.contains("bounded retry: ripr check"), "{text}");
        assert!(text.contains("not pinned"), "{text}");

        let early = build(
            &input("repo", Some("origin/main"), None),
            requested(InputSource::Committed),
            Duration::from_mins(1),
            false,
            None,
            None,
            None,
        );
        let text = early.human_recovery_text();
        assert!(
            text.contains("none (no input producer had published"),
            "{text}"
        );
    }

    #[test]
    fn stash_take_and_clear_are_scoped_to_one_projection() {
        clear_stashed();
        assert!(take_stashed().is_none());
        stash(build(
            &input("repo", None, None),
            requested(InputSource::Committed),
            Duration::from_mins(1),
            false,
            None,
            None,
            None,
        ));
        assert!(take_stashed().is_some(), "first take consumes the stash");
        assert!(
            take_stashed().is_none(),
            "a second take must find nothing (no leak into a later refusal)"
        );
    }

    #[test]
    fn post_deadline_facts_read_unavailable_and_are_named_in_human_text() {
        // #10796 review (MINOR 1): a producer finishing inside the cancel
        // grace publishes after the deadline; the projection must render the
        // fact as published_after_deadline, never available:true.
        let projection = build(
            &input("repo", Some("origin/main"), None),
            requested(InputSource::Committed),
            Duration::from_mins(1),
            true,
            None,
            None,
            Some(captured_post_deadline(published_full())),
        );
        let value = projection.input_context_value();
        for pointer in [
            "/resolved/effective_base",
            "/resolved/base_commit",
            "/resolved/scoped_changed_files",
            "/resolved/out_of_scope_files",
        ] {
            assert_eq!(
                value.pointer(pointer).and_then(|fact| fact.get("reason")),
                Some(&Value::String("published_after_deadline".to_string())),
                "{pointer} must name the grace-window reason: {value}"
            );
        }
        let text = projection.human_recovery_text();
        assert!(
            text.contains("published after the deadline (not retained)"),
            "{text}"
        );
        assert!(
            !text.contains("resolved before the deadline: effective base"),
            "a post-deadline fact must not be listed as resolved: {text}"
        );
    }

    #[test]
    fn explicit_format_and_output_dir_choices_are_preserved() {
        // #10796 review (MINOR 2): the bounded retry must repeat the explicit
        // output choices argv made — here a non-JSON --format and --output-dir
        // with a space, both shell-quoted like every other preserved value.
        let projection = build(
            &input("repo", None, Some("change.diff")),
            requested(InputSource::SuppliedDiff),
            Duration::from_secs(45),
            false,
            Some("sarif".to_string()),
            Some(PathBuf::from("out dir")),
            None,
        );
        let command = projection.recovery_command();
        assert!(
            command.contains("--format sarif"),
            "explicit non-JSON format must be preserved: {command}"
        );
        let output_dir = crate::agent::loop_commands::shell_arg_for("out dir", cfg!(windows));
        assert!(
            command.contains(&format!("--output-dir {output_dir}")),
            "explicit output dir must be preserved and quoted: {command}"
        );
        assert!(
            !command.contains("--json"),
            "--json must not appear when the user chose --format sarif: {command}"
        );
    }

    #[test]
    fn auto_selected_format_and_output_stay_absent() {
        // No explicit --format/--output-dir: the retry keeps the --json
        // spelling (or the default human renderer) and adds no output dir —
        // nothing the user did not ask for.
        let json_run = build(
            &input("repo", None, None),
            requested(InputSource::Committed),
            Duration::from_mins(1),
            true,
            None,
            None,
            None,
        );
        let command = json_run.recovery_command();
        assert!(command.contains("--json"), "{command}");
        assert!(!command.contains("--format"), "{command}");
        assert!(!command.contains("--output-dir"), "{command}");
        let human_run = build(
            &input("repo", None, None),
            requested(InputSource::Committed),
            Duration::from_mins(1),
            false,
            None,
            None,
            None,
        );
        let command = human_run.recovery_command();
        assert!(!command.contains("--json"), "{command}");
        assert!(!command.contains("--format"), "{command}");
    }

    #[test]
    fn committed_and_worktree_rereads_name_every_live_ref() {
        // #10796 review (NIT 3): a committed diff is the <base>...HEAD range,
        // so both endpoint refs are live; a working-tree retry re-resolves
        // the merge base, which reads the base ref and HEAD as well.
        let committed = build(
            &input("repo", Some("origin/main"), None),
            requested(InputSource::Committed),
            Duration::from_mins(1),
            false,
            None,
            None,
            None,
        );
        assert_eq!(
            committed.input_context_value()["recovery"]["rereads"],
            json!(["base-ref", "head-ref"])
        );
        let committed_default = build(
            &input("repo", None, None),
            requested(InputSource::Committed),
            Duration::from_mins(1),
            false,
            None,
            None,
            None,
        );
        assert_eq!(
            committed_default.input_context_value()["recovery"]["rereads"],
            json!(["base-ref", "head-ref", "default-base-resolution"])
        );
        let worktree = build(
            &input("repo", Some("origin/main"), None),
            requested(InputSource::WorkingTree),
            Duration::from_mins(1),
            false,
            None,
            None,
            None,
        );
        assert_eq!(
            worktree.input_context_value()["recovery"]["rereads"],
            json!(["worktree-bytes", "base-ref", "head-ref"])
        );
    }
}

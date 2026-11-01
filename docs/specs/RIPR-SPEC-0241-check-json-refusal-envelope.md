# RIPR-SPEC-0241: Check JSON typed refusal envelope

Status: proposed

Owner: product / cli

Created: 2026-10-07

Linked issues:

- #6834 (this slice)
- #4861 (scope-guard identities stay there)
- #4862 (typed-error migration parent)

Support-tier impact:

- None. The envelope is a read-only projection of existing failure states
  onto stdout under `--json`; it accepts no edit or execution authority and
  writes nothing. See [docs/status/SUPPORT_TIERS.md](../status/SUPPORT_TIERS.md).

Policy impact:

- None. No new process, network, or file-policy surface. The envelope
  carries caller-supplied invocation context and the verbatim human
  diagnostic, with config messages redacted per RIPR-SPEC-0007.

## Problem

Every `ripr check --json` failure except two string-matched scope guards
collapsed to exit code `2` with zero stdout bytes (#6834). Machine consumers
— loop drivers, PR automations, agent context — could not distinguish an
unresolvable base from a malformed config from a git timeout except by
parsing English stderr prose the output contracts leave free to change.
"Fix the base" and "raise the deadline" need opposite remedies; silence
sends an autonomous loop to guess.

## Behavior

Every post-argv-parse `ripr check --json` failure writes exactly one refusal
document to stdout before reporting the unchanged human diagnostic on
stderr. The document reuses the scope-guard envelope shape byte for byte —
same `schema_version` (`"0.2"`), same keys, same zeroed `summary`, same
empty `findings`, same `downstream_consumable: false` — with a typed
failure identity selecting `analysis_scope.run_status` / `basis` /
`limitation` and `run_limitations[0].category` / `run_status` / `basis`,
plus the matching `repair_route`. The closed identity vocabulary:

| Identity | Repair route | Meaning |
|---|---|---|
| `base_unresolvable` | `analysis/base-resolution` | The requested base revision does not resolve to a commit. |
| `repository_root_unusable` | `analysis/repository-root` | The root is not a directory, is not inside a Git work tree, or is a repository Git cannot read. |
| `config_invalid` | `analysis/config-load` | A discovered `ripr.toml` entry failed to load or parse. |
| `suppression_policy_invalid` | `analysis/suppression-policy` | An explicit `--suppression-policy` file is missing or malformed. |
| `git_invocation_timeout` | `analysis/git-timeout` | A git invocation exceeded its cooperative deadline and was terminated. |
| `analysis_failed` | `analysis/failure` | Honest fallback for any other failure: the run produced no findings, with no claim about which stage stopped. |

Whether the analysis ran is implicit per identity, not a separate field.
`suppression_policy_invalid` is the one identity where classification ran to
completion; the rest mean no findings were produced, and `analysis_failed`
makes no stage claim either way.

`root` and `base` echo the caller-supplied invocation context, and
`run_limitations[0].message` echoes the human diagnostic verbatim, except
for `config_invalid`, whose message is the redacted config summary (path
and parse location, no TOML source excerpt) per RIPR-SPEC-0007. No other
caller-unsupplied value enters the document.

Argv usage errors (an unknown flag, a missing value, two disagreeing
output selections) stay prose-only: exit `2`, empty stdout, the cause on
stderr. There is no successfully parsed invocation to echo, so no envelope
exists.

## Required Evidence

- A parseable stdout document for each of the six identities plus the
  fallback, each naming its identity, carrying its repair route, and
  staying non-consumable with empty findings.
- Stderr unchanged from the pre-envelope diagnostic for every class.
- Scope-guard documents byte-identical; success-path stdout byte-identical.

## Inputs

- `ripr check --json` (or `--format json`) invocations that fail after argv
  parsing: bad base, unusable root, malformed config, malformed suppression
  policy, git timeout, and unlisted failures.

## Outputs

- One refusal document on stdout per failed run, per Behavior above.
- Exit code `2` and the unchanged human diagnostic on stderr.

## Acceptance Examples

- `ripr check --json --base definitely-not-a-real-ref` emits
  `base_unresolvable` with route `analysis/base-resolution`, exit 2.
- `ripr check --json --bogus-flag` emits nothing on stdout (argv
  carve-out), exit 2, cause on stderr.
- A malformed `ripr.toml` emits `config_invalid` with the redacted summary;
  a directory with no config entry at all falls back to `analysis_failed`.

## Non-Goals

- Exit-code semantics, human-format wording, and scope-guard behavior are
  unchanged.
- No generic every-command envelope: `check --json` only.
- No new identity for timeout-vs-absence conflations inside default-base
  probing or candidate config reads; those producers keep reporting what
  they observe, and the fallback covers the rest.

## Test Mapping

- `crates/ripr/tests/cli_smoke.rs::check_json_unresolvable_base_emits_refusal_envelope`
- `crates/ripr/tests/cli_smoke.rs::check_json_unusable_root_emits_repository_root_unusable`
- `crates/ripr/tests/cli_smoke.rs::check_json_malformed_config_emits_redacted_refusal`
- `crates/ripr/tests/cli_smoke.rs::check_json_malformed_suppression_policy_emits_refusal`
- `crates/ripr/tests/cli_smoke.rs::check_json_unmigrated_analysis_failure_emits_analysis_failed`
- `crates/ripr/tests/cli_smoke.rs::check_json_timeout_and_bad_base_have_distinct_identities`
- `crates/ripr/tests/cli_smoke.rs::check_json_success_carries_no_refusal_shape`
- `crates/ripr/tests/cli_smoke.rs::check_json_argv_usage_errors_stay_prose_only`
- `crates/ripr/tests/cli_smoke/check_artifact_stdin.rs::python_stdin_artifact_refusal_preserves_named_recovery`
- `crates/ripr/tests/cli_smoke/check_artifact_stdin.rs::javascript_stdin_artifact_refusal_preserves_named_recovery`
- `crates/ripr/src/cli/commands/check.rs::config_load_refusal_without_any_config_entry_falls_back`

## Implementation Mapping

- `crates/ripr/src/core_error.rs::CheckFailureKind`
- `crates/ripr/src/core_error.rs::check_refusal`
- `crates/ripr/src/cli/commands/check.rs::refuse_check`
- `crates/ripr/src/cli/commands/check.rs::config_load_refusal`
- `crates/ripr/src/output/limited_check.rs::render_check_failure_json`

## CI Proof

- `cargo xtask check-output-contracts`
- `cargo xtask goldens check` (zero drift)
- `cargo xtask check-spec-format`
- `cargo xtask check-traceability`

## Metrics

- None. Refusal rendering is a single document write on an already-failing
  path; no timing budget governs it.

## Amendment (#6956): stalled probes and config reads name the timeout

The Non-Goals deferral above ("no new identity for timeout-vs-absence
conflations") is superseded for these two producers. No new identity was
introduced: both now report the existing `git_invocation_timeout`
identity with route `analysis/git-timeout`, which is what a stalled
probe observes.

- A default-base probe that exceeds its deadline yields
  `git_invocation_timeout` instead of `base_unresolvable`, with the
  timeout diagnostic on stderr echoed verbatim into the envelope
  message. A genuinely unanswered probe (no timeout evidence) still
  keeps `base_unresolvable`.
- A candidate-tree config read that exceeds its deadline yields
  `git_invocation_timeout` instead of `config_invalid`, with the same
  echo parity. Genuine load/parse failures stay `config_invalid`. The
  read honors the run's effective git deadline
  (`--git-timeout`/`RIPR_GIT_TIMEOUT`, default 300s) instead of a fixed
  30s, so the timeout repair guidance stays effective.

## Test Mapping (Amendment #6956)

- `crates/ripr/tests/cli_smoke.rs::check_json_omitted_base_git_timeout_names_timeout_identity`
- `crates/ripr/tests/cli_smoke.rs::check_json_bound_subject_config_timeout_names_timeout_identity`
- `crates/ripr/src/config/tests.rs::candidate_config_error_timeout_renders_the_timeout_diagnostic`
- `crates/ripr/src/analysis/diff/load.rs::core_error_for_git_root_probe_matches_the_legacy_messages`
  (extended with the `TimedOut` arm)
- `crates/ripr/src/domain/git_candidate.rs::error_display_names_each_failure_class`
  (extended with the `ExecutionTimedOut` case)

## Implementation Mapping (Amendment #6956)

- `crates/ripr/src/analysis/diff/load.rs::GitRootProbe::TimedOut`
- `crates/ripr/src/domain/git_candidate.rs::GitCandidateSubjectError::ExecutionTimedOut`
- `crates/ripr/src/config.rs::CandidateConfigError`

## Amendment (#7331, #7332): sibling JSON commands and machine check formats

The Non-Goals "no generic every-command envelope: `check --json` only" is
superseded. One envelope shape now covers every JSON-emitting command, and the
machine-readable `check` formats carry their own refusal documents:

- `ripr diff --json`, `ripr rerun --json`, `ripr context` (JSON only), `ripr
  agent brief --json`, and `ripr agent packet --json` write the same envelope
  as `check --json` for every post-argv-parse operational failure, keyed with
  the refusing command's own `schema_version` (`"0.1"` for diff and agent brief,
  `"ripr-targeted-rerun-v1"` for rerun, `"1.1"` for context, `"0.5"` for agent
  packet). `mode` appears only on commands that accept `--mode` (check, diff,
  context); `scope` is `"diff"` for diff-scoped commands and `"repo"` for
  agent brief and agent packet; agent brief echoes the caller-supplied
  `--base` when its working set selected one. Exit stays `2`, prose stays on
  stderr, and argv usage errors stay prose-only with empty stdout.
- The closed identity vocabulary includes eight members. All but
  `analysis_timeout` and `analysis_worker_stopped` make no claim that analysis
  ran; `analysis_timeout` (#5369) means the wall-clock budget expired before
  the budgeted check analysis returned, and `analysis_worker_stopped` (#11116)
  means the analysis worker died before returning:

| Identity | Repair route | Meaning |
|---|---|---|
| `check_artifact_unusable` | `analysis/check-artifact` | A named `ripr context --from` check artifact is unreadable, malformed, foreign, or identity-mismatched. |
| `gap_ledger_unusable` | `analysis/gap-ledger` | A named `ripr rerun --gap-ledger` file cannot be read or parsed. |
| `seam_id_unknown` | `analysis/seam-inventory` | A named `ripr agent packet --seam-id` matches no current seam. |
| `seam_not_actionable` | `analysis/seam-limitation` | A named `ripr agent start --seam-id` matches a seam policy permits but the shared repair-admission gate refuses to start: a static-limitation seam with no repairable gap, or one `ripr repair` reports as not repair-eligible (#10712). |
| `seam_policy_omitted` | `analysis/seam-policy` | A named `ripr agent packet`, `ripr agent start`, or `ripr agent repair --phase before` `--seam-id` matches a seam that agent policy withholds (configured off, already gripped, or not agent-visible) (#10712). |
| `diff_input_unsupported` | `analysis/diff-input` | An `agent brief --diff` or `--base` working set carries a shared-parser limitation, admission refusal, or truncated textual section; brief selection did not run. Empty valid patches retain their successful brief behavior. |
| `analysis_timeout` | `analysis/timeout` | The `ripr check` wall-clock budget (`--timeout SECS`, `RIPR_CHECK_TIMEOUT`) ran out before analysis finished (#5369). No findings are produced and the result is never a clean pass. |
| `analysis_worker_stopped` | `analysis/worker-stopped` | The `ripr check` analysis worker stopped without producing a result, almost always a panic (#11116). No findings are produced and the result is never a clean pass. |

- For `ripr check`, the machine-readable formats `sarif`, `github`,
  `badge-json`, `repo-exposure-json`, and `agent-seam-packets-json` render a
  per-format refusal document on stdout (SARIF tool-configuration
  notification; one GitHub error annotation; the badge degraded-state
  convention; the repo-exposure and agent-seam-packets refused envelopes with
  zeroed arrays) instead of zero bytes. Machine formats without an honest
  refusal shape yet (`repo-sarif`, `badge-plus`, `repo-badge-*`, `shields`,
  `repo-seams-*`, `repo-exposure-summary-json`) stay prose-only; that gap is a
  named limitation, not a clean result.

The config-redaction duty (RIPR-SPEC-0007) and the verbatim-diagnostic echo
apply to every refusal document, whatever the target format: one resolved
`RefusalNotice` (identity, repair route, redacted-or-verbatim message) feeds
all renderers.

## Test Mapping (Amendment #7331, #7332)

- `crates/ripr/tests/task_first_facade/brief_diff_status.rs::brief_empty_and_ordinary_patches_keep_successful_selection`
- `crates/ripr/tests/task_first_facade/brief_diff_status.rs::brief_parser_refusals_cannot_become_empty_or_fallback_successes`
- `crates/ripr/tests/task_first_facade/brief_diff_status.rs::brief_base_uses_the_same_parser_refusal_before_inventory`
- `crates/ripr/tests/task_first_facade/brief_diff_status.rs::brief_parser_errors_keep_diff_input_identity_and_budget_guards`
- `crates/ripr/tests/task_first_facade/brief_diff_status.rs::brief_truncated_sections_cannot_become_empty_or_partial_successes`
- `crates/ripr/tests/task_first_facade/brief_diff_status.rs::brief_path_like_hunk_payload_keeps_real_changed_line_selection`
- `crates/ripr/tests/cli_smoke.rs::diff_json_unresolvable_base_emits_refusal_envelope`
- `crates/ripr/tests/cli_smoke.rs::diff_json_outside_work_tree_emits_refusal_envelope`
- `crates/ripr/tests/cli_smoke.rs::rerun_json_unreadable_gap_ledger_emits_refusal_envelope`
- `crates/ripr/tests/cli_smoke.rs::context_json_missing_artifact_emits_refusal_envelope`
- `crates/ripr/tests/cli_smoke.rs::context_json_fresh_analysis_preserves_base_failures`
- `crates/ripr/tests/cli_smoke.rs::context_json_fresh_analysis_preserves_repository_root_failure`
- `crates/ripr/tests/cli_smoke.rs::context_json_fresh_analysis_keeps_untyped_failure_fallback`
- `crates/ripr/tests/cli_smoke.rs::agent_brief_json_unresolvable_base_emits_refusal_envelope`
- `crates/ripr/tests/cli_smoke.rs::agent_packet_json_unknown_seam_id_emits_refusal_envelope`
- `crates/ripr/tests/cli_smoke.rs::check_sarif_refusal_emits_a_schema_valid_run`
- `crates/ripr/tests/cli_smoke.rs::check_github_refusal_emits_one_error_annotation`
- `crates/ripr/tests/cli_smoke.rs::check_badge_json_refusal_emits_the_degraded_badge`
- `crates/ripr/tests/cli_smoke.rs::check_repo_exposure_json_refusal_emits_the_refused_envelope`
- `crates/ripr/tests/cli_smoke.rs::check_agent_seam_packets_json_refusal_emits_the_refused_envelope`
- `crates/ripr/src/output/limited_check.rs::refusal_envelope_names_the_artifact_ledger_and_seam_identities`
- `crates/ripr/src/output/limited_check.rs::sibling_refusal_envelope_omits_mode_and_base_when_unsupplied`
- `crates/ripr/src/output/sarif.rs::refusal_sarif_is_a_schema_valid_run_with_the_typed_notification`
- `crates/ripr/src/output/github.rs::refusal_renders_one_workflow_command_error_with_identity_and_route`
- `crates/ripr/src/output/badge/tests.rs::refusal_badge_summary_applies_the_degraded_state_convention`
- `crates/ripr/src/output/repo_exposure.rs::refusal_document_keeps_the_format_contract_with_zeroed_metrics`
- `crates/ripr/src/output/agent_seam_packets.rs::refusal_document_keeps_the_packet_envelope_with_no_packets`

## Implementation Mapping (Amendment #7331, #7332)

- `crates/ripr/src/core_error.rs::CheckFailureKind::{CheckArtifactUnusable, GapLedgerUnusable, SeamIdUnknown, SeamPolicyOmitted, SeamNotActionable, DiffInputUnsupported, AnalysisTimeout, AnalysisWorkerStopped}`
- `crates/ripr/src/cli/commands/check_budget.rs::run_with_budget`
- `crates/ripr/src/cli/commands_agent_support.rs::agent_brief_checked_lines_from_diff`
- `crates/ripr/src/core_error.rs::RefusalNotice`
- `crates/ripr/src/core_error.rs::refusal_notice`
- `crates/ripr/src/output/limited_check.rs::RefusalEnvelopeContext`
- `crates/ripr/src/output/limited_check.rs::render_refusal_envelope`
- `crates/ripr/src/cli/commands_context.rs::refuse_json_envelope`
- `crates/ripr/src/cli/commands/check.rs::machine_refusal_document`
- `crates/ripr/src/output/sarif.rs::render_refusal_sarif`
- `crates/ripr/src/output/github.rs::render_refusal`
- `crates/ripr/src/output/badge/summaries.rs::refusal_badge_summary`
- `crates/ripr/src/output/repo_exposure.rs::render_refusal_json`
- `crates/ripr/src/output/agent_seam_packets.rs::render_refusal_json`
- `crates/ripr/src/cli/commands.rs::diff` (refusal wiring)
- `crates/ripr/src/cli/rerun.rs::run_refusing`
- `crates/ripr/src/cli/commands/context.rs` (refusal wiring)
- `crates/ripr/src/cli/commands/agent.rs::{refuse_agent_brief, refuse_agent_packet}`
- `crates/ripr/src/app/check_artifact.rs::load_findings_for_reuse_core`
- `crates/ripr/src/cli/rerun.rs::rerun_gap` (typed ledger errors)

## Amendment (#10720): preserve fresh context analysis failures

The fresh `context` pipeline retains the check producer's `CoreError` through
both committed and working-tree analysis. A named base or repository-root
failure reaches the existing refusal renderer with its original identity and
repair route. Unmigrated loader failures retain `analysis_failed`; no message
parsing or new failure family is introduced. The context envelope uses version
`1.1`, as established by RIPR-SPEC-0284.

Selector misses and ambiguity remain usage-class under RIPR-SPEC-0003: exit 2,
empty stdout, and recovery guidance on stderr. Supplied-artifact refusals retain
`check_artifact_unusable`. Public String-returning library compatibility wrappers
keep their existing error text.

## Amendment (#10796): input context and bounded recovery in analysis-timeout refusals

The same-keys rule above ("same keys ... plus the matching `repair_route`")
and the no-caller-unsupplied-values rule ("No other caller-unsupplied value
enters the document") are amended, not silently violated, for one identity:

- An `analysis_timeout` refusal of `ripr check` renders at the new refusal
  generation `schema_version: "0.3"` and carries one additional key,
  `input_context`. Every other identity of every command — including
  `analysis_timeout` refusals without a stashed projection (library callers
  that never registered the capture) — keeps generation `0.2` and the exact
  pre-amendment key set. Scope-guard envelopes stay frozen at `0.2` per
  #4861, and sibling commands (`diff`, `rerun`, `context`, `agent brief`,
  `agent packet`) keep their own versions byte-for-byte: none gains
  `input_context`.
- `input_context` has three fixed keys: `requested`, `resolved`, and
  `recovery`. `requested` echoes only what the invocation already
  established — root, the requested input source (`committed`,
  `working-tree`, `supplied-diff`, `git-candidate`), base, diff file, the
  `--path`/`--exclude` scope, and the analysis effort mode under the key
  `analysis_mode` (the source mode and the analysis mode are different
  quantities and never substitute for one another). `resolved` carries the
  base/head/merge-base revision facts and the scoped intake file counts,
  each as `{available: true, value}` or `{available: false, reason}`.
  `reason` is `not_published_before_deadline` when the fact's own producing
  stage had not completed when the budget fired (an early timeout; a
  progress heartbeat alone never establishes discovery),
  `not_computed_for_source` when that stage completed but never computes the
  fact for that input source (for example a merge base on a committed-history
  diff, or any base fact for a supplied diff), and `published_after_deadline`
  when the publish landed at or after the deadline stamp — a producer
  finishing inside the cancel grace must not read as available context.
- Only producers that completed before the deadline may populate a fact:
  the diff loader publishes the resolved base/commit/merge-base after its
  load returns, and the scoped intake publishes its counts after the scope
  partition. The check budget owner stamps the deadline instant when the
  budget starts and registers the capture slot on the worker thread with
  that stamp, so a grace-window publish is masked `published_after_deadline`
  per fact rather than blended into the available set; every other surface
  (LSP refresh, library callers) runs with no registered capture, so
  publishing is a no-op. No new Git discovery, analysis, or cleanup runs
  after the deadline to enrich the refusal, and no run pays for capture
  when no budget is registered.
- `recovery` offers at most one bounded next action. For a replayable input
  it is `retry_with_larger_budget`: a shell-safe literal command rebuilt
  from the invocation authority that actually ran — the raw root (never the
  `display_path` echo, which escapes `%` and would name another directory),
  an explicit base, an explicit `--worktree` / `--committed` / `--diff` /
  candidate selection, the scope patterns, the effective analysis settings
  that change the finding set (the effective `--mode`, including draft,
  `--no-unchanged-tests` when unchanged tests were excluded, and
  the explicit `--perl-facts` / `--suppression-policy` paths) so the retry
  reproduces the requested analysis, the output selection (the explicit
  `--format VALUE` spelling when one was given, otherwise `--json` when the
  run was JSON), and a finite doubled budget (`--timeout 2*N` for an expired
  `N`-second budget; "disable all time limits" is never the continuation).
  An explicit `--output-dir` destination is never repeated: the refused
  output set occupies that directory and custody rejects an existing
  destination, so `note` names the occupied destination and the caller names
  a new one. For a stdin diff (`--diff -`) no literal
  command is offered: the consumed bytes cannot be replayed, so the action
  is `save_stdin_diff_to_named_file` with `command: null` and the note names
  the honest continuation. `rereads` names what a retry re-reads (`base-ref`
  and `head-ref` for the committed `<base>...HEAD` range, `worktree-bytes`
  with the base/head merge-base re-resolution for a working-tree diff,
  `diff-file-bytes`, or none for immutable Git candidate objects) and `note`
  states that a live-ref or worktree retry is not pinned to the timed-out
  run's snapshot. The recovery is advisory: execution stays with the
  caller.
- Enrichment changes no verdict semantics: exit stays `2`, the typed
  `analysis_timeout` identity, repair route `analysis/timeout`, empty
  findings, the zeroed analysis summary, and `downstream_consumable: false`
  are unchanged, and no `exposed`/clean/repair-ready inference follows from
  input metadata.
- Strict readers: generation `0.2` documents keep the closed key set of the
  pre-amendment contract; generation `0.3` documents keep that key set plus
  the required `input_context` key. The published machine schemas split the
  same way: `schemas/ripr/check.schema.json` keeps pinning generation `0.2`
  and `schemas/ripr/check-timeout-refusal.schema.json` pins `0.3` with
  `input_context` required, so every emitted refusal has a published schema
  that admits it.

## Test Mapping (Amendment #10796)

The [traceability slice](../../.ripr/traceability.d/10796-timeout-refusal-input-context.toml)
lists the input-facts, budget, refusal, output and CLI controls for this amendment.

## Implementation Mapping (Amendment #10796)

- `crates/ripr/src/analysis/input_facts.rs` (`PublishedInputFacts`, `CapturedInputFacts` with the post-deadline mask, capture slot, publish/take)
- `crates/ripr/src/analysis/pipeline.rs` (loader and scoped-intake publish sites)
- `crates/ripr/src/app/timeout_refusal.rs` (`RequestedSourceContext`, `TimeoutRefusalProjection`, stash)
- `crates/ripr/src/app.rs::CHECK_REFUSAL_SCHEMA_VERSION`
- `crates/ripr/src/cli/commands/check_budget.rs` (deadline stamp, worker capture registration)
- `crates/ripr/src/cli/commands/check.rs` (`stash_timeout_refusal`, human recovery prose)
- `crates/ripr/src/output/limited_check.rs::render_check_failure_json` (generation 0.3 and `input_context`)

## Amendment (#11790, #11584): structured remedy

Every refusal in the envelope also carries its next step as data. Each
`run_limitations[0]` entry of a refusal envelope (generations `0.2` and `0.3`)
gains an optional `remedy` object, additive and without a schema-version bump:
`kind` (`command`, `mcp_tool`, `flags` or an explicit `none`), the stated
`command`, `alternatives`, `tool`, `flags`, `fill_in` placeholders and
`retry_stated` (what the message says, not a promise an unchanged retry succeeds), `env`, `conditional` (the only stated step is a fallback) and `prose_has_more` (the command omits flags or environment prerequisites the prose states). The prose in `message` is the source of truth: the owner
(`RefusalRemedy::from_prose`, derived once in `CoreError::refusal_notice`)
copies only a backticked `ripr ...` command, an MCP tool name or a flag the
message states, and yields `none` otherwise. The renderer serializes it and
never re-derives it. The frozen scope-guard documents are unchanged.
`schemas/ripr/check.schema.json` and
`schemas/ripr/check-timeout-refusal.schema.json` admit the field; the field is
described in [OUTPUT_SCHEMA](../OUTPUT_SCHEMA.md#structured-refusal-remedy-remedy-additive-11790-11584).

Controls: `domain::refusal_remedy` unit and source-corpus tests (every
message-shaped literal yields a grounded remedy or an explicit `none`, and a
stated command is never dropped), `output::limited_check` envelope tests
(positive and negative), and `tests/refusal_next_step.rs`
(`json_refusals_carry_a_grounded_structured_remedy`, the built binary).

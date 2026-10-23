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
  as `check --json` for every post-argv-parse failure, keyed with the refusing
  command's own `schema_version` (`"0.1"` for diff and agent brief,
  `"ripr-targeted-rerun-v1"` for rerun, `"1.0"` for context, `"0.5"` for agent
  packet). `mode` appears only on commands that accept `--mode` (check, diff,
  context); `scope` is `"diff"` for diff-scoped commands and `"repo"` for
  agent brief and agent packet; agent brief echoes the caller-supplied
  `--base` when its working set selected one. Exit stays `2`, prose stays on
  stderr, and argv usage errors stay prose-only with empty stdout.
- The closed identity vocabulary includes five members. The first four make no
  claim that analysis ran; `analysis_timeout` (#5369) means the wall-clock
  budget expired before the budgeted check analysis returned:

| Identity | Repair route | Meaning |
|---|---|---|
| `check_artifact_unusable` | `analysis/check-artifact` | A named `ripr context --from` check artifact is unreadable, malformed, foreign, or identity-mismatched. |
| `gap_ledger_unusable` | `analysis/gap-ledger` | A named `ripr rerun --gap-ledger` file cannot be read or parsed. |
| `seam_id_unknown` | `analysis/seam-inventory` | A named `ripr agent packet --seam-id` matches no current seam. |
| `diff_input_unsupported` | `analysis/diff-input` | An `agent brief --diff` or `--base` working set carries a shared-parser limitation, admission refusal, or truncated textual section; brief selection did not run. Empty valid patches retain their successful brief behavior. |
| `analysis_timeout` | `analysis/timeout` | The `ripr check` wall-clock budget (`--timeout SECS`, `RIPR_CHECK_TIMEOUT`) ran out before analysis finished (#5369). No findings are produced and the result is never a clean pass. |

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

- `crates/ripr/src/core_error.rs::CheckFailureKind::{CheckArtifactUnusable, GapLedgerUnusable, SeamIdUnknown, DiffInputUnsupported, AnalysisTimeout}`
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

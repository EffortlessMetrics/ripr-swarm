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

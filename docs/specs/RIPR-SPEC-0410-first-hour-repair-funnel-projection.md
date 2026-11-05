# RIPR-SPEC-0410: First-hour repair funnel projection

Status: proposed

Owner: test-infra

Created: 2026-11-02

Linked issues:

- #6308 (this slice)
- #1693 (parent programme)
- #5286 (first-run walk receipts)
- #5299 (dx-scoreboard)
- #1702 (governed real repair attempts)

Support-tier impact:

- None. The projection reads committed receipt fragments and fixture corpora;
  it executes no repair, calls no provider, and makes no support promotion.
  [docs/status/SUPPORT_TIERS.md](../status/SUPPORT_TIERS.md)

Policy impact:

- None beyond writing advisory reports under `target/ripr/reports`.

## Problem

Install, first-run, RepairCard, and governed repair-attempt receipts exist,
but no versioned funnel joins them into one denominator for time and effort
to honestly verified improvement or a useful terminal limitation (#6308).

## Behavior

`cargo xtask repair-funnel-report` evaluates `fixtures/repair_funnel/controls/corpus.json`
through `first_hour_repair_funnel_inputs.v1` → `first_hour_repair_funnel.v1`:

- Stages appear only when an authoritative receipt fragment supplies them;
  missing stages are not inferred from prose or issue state.
- `time_to_verified_improvement` requires both project verification executed
  at declared strength and current static movement improved or closed.
- Project verification counts only when the attempt declares a non-empty
  `verify_command`; a bare `pass` leaves the case at `verification_not_run`.
- Static movement reads the receipt vocabulary, including
  `receipt_movement_resolved` as closure.
- Stage offsets share one zero (journey start). Durations include install
  time; a timed install is the evidence gate for every duration.
- Excluding a case from the denominator needs an explicit `exclusion_reason`;
  a silent exclusion is `control_6`.
- Rows projected from the committed controls corpus are marked
  `planted_control` and never feed `repair.*` metrics. Scoreboard ingest counts
  only `governed_real_repository_attempt` rows that are selected and carry no
  control violations; otherwise the metrics stay `not_measured`.
- An empty controls corpus, or one that plants no negative control, fails.
- Cohorts (`deterministic_fixture`, `installed_deterministic_journey`,
  `blind_operator_journey`, `governed_real_repository_attempt`) stay separate.
- Planted controls reject verified improvement without verification, static
  improvement without verification, fixture-as-real claims, blind success
  with hidden assistance, and denominator drops.
- JSON and Markdown render from one normalized report struct.

`cargo xtask dx-scoreboard --boards repair --ingest` accepts
`first_hour_repair_funnel.v1` via `ingest:repair-funnel` metrics declared in
`benchmarks/dx_scoreboard/scoreboards.toml`. Zero selected cases remain
`not_measured`, never zero duration or success.

## Retained installed ordinary input

`cargo xtask repair-funnel-report --input <manifest.json>` admits the existing
#11919/#12404 strict installed ordinary-loop producer. With no arguments the
command continues to check only the separate planted controls corpus.

The input envelope is `retained_installed_ordinary_manifest.v1`, with exactly
`schema_version`, `cohort`, and `bundles`. Cohort must be
`installed_deterministic_journey`; each bundle has a relative `directory` and
`files` mapping every consumed relative filename to its `sha256:<hex>` digest.
Unknown envelope fields, traversal, escaping symlinks and digest disagreement
refuse. Paths resolve relative to the manifest. A bundle is bounded to 1,024
files / 64 MiB and the manifest to 1 MiB. Both paths admit regular files and
cap actual reads at the remaining byte budget plus one byte for overflow
detection; metadata lengths cannot establish that limit. Native exit status and
its recorded success flag must agree, and a retained static improvement requires
a nonzero analyzed probe as well as its matching finding.

The adapter understands the historical producer's explicit legacy field shape:
wheel schema 2, check schema 0.2, admitted installed identity, invocation,
runner, unchanged before/after subject and source hashes, raw command records,
result records and stdout/stderr hashes. These older producer fragments do not
have their own version tag; the input envelope versions this exact adapter.
A receipt with another shape is unsupported. This is custody and consistency
admission, not an independent verifier attestation or an assertion that supplied
hashes can authenticate a maliciously fabricated bundle. Hostile concurrent
pre-open filesystem namespace mutation is outside this custody contract.

Native verification requires the existing exact Cargo command, selector,
nonzero selected/executed denominator, raw libtest result and native status.
A human `pass`, completion boolean or later summary cannot supply it. Static
movement requires complete source-matched before/after checks for the same
canonical item and probe; source files, manifest and lockfile must match the
retained subject digests. A wrong-base quiet report cannot supply movement.

`journey/native-after.json` and `journey/after.json` may be absent. Their absent
stages remain absent, even if another summary says the fixture completed.
Zero selection, failed verification and missing after state remain visible.
Exact duplicate views of one candidate/root/base/head/item count once;
conflicting duplicate views refuse rather than discard the unfavorable view.

The normalized report keeps `first_hour_repair_funnel.v1` and the existing
cohort vocabulary. Its identity fields retain each admitted source digest,
manifest digest, candidate/binary/root/base/head/item, missing stages and
non-claims. JSON and Markdown render those same fields. Existing `project_case`
derives native/static stages and exit from admitted ordinary facts; this does
not reconstruct a RepairAttempt. There is no repair-start, RepairCard or
terminal-attempt receipt. Legacy command elapsed times supply no common stage
clock or install timing; all duration metrics remain `not_measured`.
Installed deterministic results contribute no real `repair.*` samples.

`fixtures/repair_funnel/installed-41383028929.json` pins an exact-byte subset
of the genuine retained installed artifact. It is regression evidence for this
adapter and is not a new installed run, governed attempt or personal use.
See [fixture provenance](../../fixtures/repair_funnel/SPEC.md) for the immutable
source and imported-byte boundary.

## Required Evidence

- `fixtures/repair_funnel/controls/corpus.json`
- `cargo test -p xtask --bin xtask repair_funnel`
- `cargo xtask repair-funnel-report`
- `cargo xtask repair-funnel-report --input fixtures/repair_funnel/installed-41383028929.json`

## Non-Goals

- No new real-repository attempt execution, installed journey, or blind operator run.
- No support-tier or release verdict from funnel metrics alone.
- No automatic product optimization from this slice.

## Acceptance Examples

- Governed case with install + passing verification + improved static movement
  records `verified_improvement_or_closure` and measured install-to-verified ms.
- Verification pass with unchanged static movement exits `honest_static_limitation`
  without a verified-improvement stage.
- Incompatible clocks leave duration metrics `not_measured`.

## Test Mapping

- `xtask/src/reports/repair_funnel.rs::tests::*`
- `xtask/src/reports/repair_funnel/installed.rs::tests::*`
- `fixtures/repair_funnel/controls/corpus.json` (planted controls)

## Implementation Mapping

- `xtask/src/reports/repair_funnel.rs`
- `xtask/src/reports/dx_scoreboard.rs` (`repair_funnel_to_input`)
- `benchmarks/dx_scoreboard/scoreboards.toml` (`repair.*` metrics)
- `xtask/src/command.rs` / `xtask/src/dispatch.rs` (`repair-funnel-report`)

## Metrics

- `repair.time_to_verified_improvement_ms` (advisory, runner-dependent)
- `repair.time_to_first_honest_result_ms` (advisory, runner-dependent)
- `repair.selected_case_count` (population visibility)


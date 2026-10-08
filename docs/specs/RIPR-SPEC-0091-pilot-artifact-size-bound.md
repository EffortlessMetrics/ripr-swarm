# RIPR-SPEC-0091: Pilot Artifact Size Bound

Status: proposed

Owner: product / swarm

Created: 2026-06-13

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #1170 — `ripr pilot` writes ~400 MB artifacts for a one-file change
- #5861 — repo-exposure guidance must name the pilot budget that actually fired

Linked PRs:

- None yet

Support-tier impact:

- No tier change. This spec adds a size bound to the two large pilot artifacts
  (`repo-exposure.json`, `agent-seam-packets.json`). It does not change the
  analysis logic, pass/fail authority, or what the analyzer classifies.
- The bound is additive: when applied it inserts a `limitations[]` disclosure
  into both artifacts naming the cap, the controlling env var, and a repair
  route. No new fields are added to the `check.json` shape. No schema version
  bump. Claim boundaries and tier governance remain governed by the canonical
  ledger in [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml`.
- No new crates, binaries, dependencies, parsers, runtime executors, or LSP
  servers introduced by this spec.

## Problem

`ripr pilot` scans every seam in the entire workspace and writes the full
classified seam list to two large JSON artifacts:

- `repo-exposure.json` — one entry per seam with reach/activate/propagate/
  observe/discriminate evidence and observed values.
- `agent-seam-packets.json` — one packet per seam with full evidence records.

For large Rust workspaces (tens of thousands of seams) these files balloon to
hundreds of megabytes per run. A one-file change that triggers a deep-mode
full-repo scan has been observed to write ~205 MB + ~189 MB ≈ 394 MB in a
single pilot invocation. At this size the artifacts are impractical to open,
diff, store in CI, or pass to downstream tools.

The existing `RIPR_REPO_EXPOSURE_SEAM_LIMIT` (default 10,000) bounds the
repo-exposure inventory pass but the pilot command discarded that `SeamLimitInfo`
result and then passed `None` to `render_agent_seam_packets_json`, leaving
`agent-seam-packets.json` entirely unbounded. The root fix requires:

1. A tighter default budget for both pilot artifacts.
2. Honest fail-closed disclosure in both artifacts when the budget is applied.

## Behavior

### Budget cap

A new constant `DEFAULT_PILOT_SEAM_BUDGET = 2_000` is introduced in
`analysis::seam_inventory`. After the workspace inventory completes, any
classified seam list longer than the budget is truncated to the budget length
before rendering. The truncation keeps every actionable seam on a line changed
in the current change (#5324), then fills the remaining budget from the front of
the inventory order; the kept set stays in inventory order. When the current
change has more actionable seams than the budget, the first budget-many of them
in inventory order are kept. The disclosure still reports the budget as the
analyzed count.

### Environment variable control

`RIPR_PILOT_SEAM_BUDGET` controls the budget:

- Unset → use `DEFAULT_PILOT_SEAM_BUDGET` (2,000). `SeamLimitSource::Default`.
- Set to a positive integer N → use N. `SeamLimitSource::Configured`.
- Set to `0` → no pilot budget (opt-out). Both artifact renderers receive
  the inventory-retained seam list.

The budget is distinct from `RIPR_REPO_EXPOSURE_SEAM_LIMIT` (which bounds the
inventory pass). When both caps apply the pilot budget fires on the already-
capped slice and the tighter cap's `SeamLimitInfo` is disclosed.

### Disclosure

When the pilot budget is applied, `repo-exposure.json` includes:

```json
"run_status": "seam_limit_applied",
"limitations": [
  {
    "category": "pilot_seam_budget_applied",
    "seams_analyzed": <N>,
    "seams_total": <M>,
    "limit_source": "default" | "configured",
    "control": "RIPR_PILOT_SEAM_BUDGET",
    "repair_route": "Set RIPR_PILOT_SEAM_BUDGET=0 to disable the pilot artifact budget."
                  | "Set RIPR_PILOT_SEAM_BUDGET=0 to disable the pilot artifact budget, or raise it to render more seams in the pilot artifacts."
  }
]
```

For a pilot cut, `agent-seam-packets.json` uses the same `run_status`,
limitation category, counts, `limit_source` and control, with its existing
packet-specific `repair_route`. This repo-exposure guidance change preserves
those packet routes. When the pilot budget fired, the current packet routes are:

- Default: ``Set RIPR_PILOT_SEAM_BUDGET=0 to render packets for all seams, or scope the run to a change with `ripr check --base <REV>` (or `ripr check --diff <PATH>`).``
- Configured: ``Remove or raise RIPR_PILOT_SEAM_BUDGET to render packets for more seams, or scope the run to a change with `ripr check --base <REV>` (or `ripr check --diff <PATH>`).``

Packet inventory-only attribution remains tracked separately in #7186 and is
outside this repo-exposure repair.

When neither cap is applied, `"run_status": "complete"` is emitted with no
cap limitation (mirroring the existing `repo-exposure.json` contract from
RIPR-SPEC-0074). An inventory-only cut keeps repo-exposure's
`repo_seam_limit_applied` category and `RIPR_REPO_EXPOSURE_SEAM_LIMIT` control.
The renderer receives the identity of the cap that fired; a configured but
inactive pilot budget must not relabel inventory guidance.

For repo-exposure JSON, the default pilot repair route is
`Set RIPR_PILOT_SEAM_BUDGET=0 to disable the pilot artifact budget.`;
the configured route is
`Set RIPR_PILOT_SEAM_BUDGET=0 to disable the pilot artifact budget, or raise it to render more seams in the pilot artifacts.`.
Markdown names the same control in its partial-scan disclosure. These
repo-exposure routes describe removal of the pilot artifact budget only. If
the inventory cap also fired, recovering inventory-excluded seams separately
requires `RIPR_REPO_EXPOSURE_SEAM_LIMIT=0`; disabling the pilot budget alone
does not recover those seams.

The `pilot-summary.json` and `pilot-summary.md` artifacts are NOT modified.
They already reflect the top-N seams from the pilot summary logic, which is
governed separately.

### Non-claims

- This spec does NOT change the exit code or gate authority.
- The pilot budget is a presentation bound: it does not reduce the
  classified inventory, only the artifact input. The separate inventory cap
  may bound that inventory.
- This spec does NOT imply that the uncapped seams are unimportant. The repair
  routes identify the applied cap. Full output requires disabling every cap
  that fired.

## Non-Goals

- Capping `pilot-summary.json` or `pilot-summary.md`.
- Capping `check.json` or `human.txt` (`ripr check` output).
- Changing `RIPR_REPO_EXPOSURE_SEAM_LIMIT` semantics.
- Streaming output or incremental artifact writes.
- Per-seam evidence field truncation (observed_values, missing_discriminators,
  evidence_record).

## Required Evidence

- `SeamLimitInfo { analyzed: usize, total: usize, source: SeamLimitSource }`
  already exists in `analysis::seam_inventory`.
- `render_repo_exposure_json(classified, limit_info)` already accepts
  `Option<&SeamLimitInfo>` and emits `run_status` / `limitations[]` per
  RIPR-SPEC-0074.
- `render_agent_seam_packets_json(classified, limit_info)` — NEW parameter
  mirrors the repo-exposure renderer pattern.

## Inputs

| Input | Required? | Purpose |
| --- | --- | --- |
| `RIPR_PILOT_SEAM_BUDGET` env var | no | Override the default 2,000 seam budget |
| `classified: &[ClassifiedSeam]` (post-inventory) | yes | Slice to cap before rendering |
| `SeamLimitInfo` from inventory pass | no | Carry forward if the inventory cap fired first |

## Outputs

| Output | Schema impact | Notes |
| --- | --- | --- |
| `repo-exposure.json` `run_status` | Additive | `complete` or `seam_limit_applied` |
| `repo-exposure.json` `limitations[]` | Additive | Includes a disclosure when a cap is applied |
| `agent-seam-packets.json` `run_status` | NEW field | `complete` or `seam_limit_applied` |
| `agent-seam-packets.json` `limitations[]` | NEW field | Includes a disclosure when a cap is applied |
| `pilot-summary.json` | None | Unchanged |
| `check.json` | None | Unchanged |

## Acceptance Examples

Unless stated otherwise, these examples assume the inventory cap does not
fire. Packet totals may be lower than the rendering input because the packet
renderer retains its existing admission filter.

1. **Default budget**: workspace with 5,000 seams — repo-exposure contains
   2,000 seams; both pilot artifacts disclose `run_status: "seam_limit_applied"`,
   `limitations[0].seams_analyzed: 2000`,
   `limitations[0].seams_total: 5000`, `limit_source: "default"`.
2. **Opt-out**: `RIPR_PILOT_SEAM_BUDGET=0` and
   `RIPR_REPO_EXPOSURE_SEAM_LIMIT=0` — repo-exposure contains all seams; both
   artifacts disclose `run_status: "complete"` with no cap limitation.
3. **Custom budget**: `RIPR_PILOT_SEAM_BUDGET=500` with 800 seams —
   repo-exposure contains 500 seams; both artifacts disclose
   `limit_source: "configured"`.
4. **Small workspace**: workspace with 100 seams, default budget 2,000 —
   repo-exposure contains all 100 seams; both artifacts disclose
   `run_status: "complete"`.
5. **Budget via help**: `ripr pilot --help` output names `RIPR_PILOT_SEAM_BUDGET`
   with its default, opt-out value, and disclosure explanation.

## Test Mapping

- `crates/ripr/tests/cli_smoke.rs::pilot_snapshot_truncated_by_the_seam_budget_is_not_a_verify_baseline` — built CLI configured/both/inventory-only/uncapped/replay controls.
- `crates/ripr/src/output/repo_exposure.rs::tests::pilot_limit_disclosure_names_the_applied_cap` — default/configured pilot JSON and Markdown literals, plus uncapped nonempty output.
- `crates/ripr/src/output/repo_exposure.rs::tests::inventory_limit_routes_remain_unchanged` — default/configured inventory repair routes.
- `crates/ripr/src/app/pr_summary/json.rs::tests::pilot_budget_disclosure_is_preserved_in_pr_summary` — the summary consumer preserves the repair route and names the pilot presentation cap.

- `crates/ripr/src/analysis/seam_inventory.rs::tests::pilot_seam_budget_default_constant_is_smaller_than_repo_exposure_cap`
- `crates/ripr/src/analysis/seam_inventory.rs::tests::pilot_seam_budget_env_zero_parses_as_unbounded`
- `crates/ripr/src/analysis/seam_inventory.rs::tests::apply_pilot_seam_budget_inner_truncates_when_above_limit`
- `crates/ripr/src/analysis/seam_inventory.rs::tests::apply_pilot_seam_budget_inner_returns_none_when_at_or_below_limit`
- `crates/ripr/src/output/agent_seam_packets.rs::tests::no_limit_info_emits_run_status_complete`
- `crates/ripr/src/output/agent_seam_packets.rs::tests::limit_info_emits_run_status_seam_limit_applied_and_disclosure`
- `crates/ripr/src/output/agent_seam_packets.rs::tests::limit_info_configured_source_emits_configured_repair_route`

## Implementation Mapping

- `crates/ripr/src/analysis/seam_inventory.rs` — adds `PILOT_SEAM_BUDGET_ENV`,
  `DEFAULT_PILOT_SEAM_BUDGET`, `apply_pilot_seam_budget`, `apply_pilot_seam_budget_inner`,
  and `pilot_seam_budget` helper.
- `crates/ripr/src/output/agent_seam_packets.rs` — adds `limit_info: Option<&SeamLimitInfo>`
  parameter to `render_agent_seam_packets_json`; emits `run_status` and `limitations[]`
  mirroring the repo-exposure renderer pattern.
- `crates/ripr/src/output/repo_exposure.rs` — `RepoExposureLimit` preserves
  inventory versus pilot cap identity while sharing the bounded document writer.
- `crates/ripr/src/cli/commands/pilot.rs` — threads `SeamLimitInfo` from the
  inventory result through `apply_pilot_seam_budget`; selects repo-exposure's
  cap context from whether the pilot budget actually truncated the population.
- `crates/ripr/src/cli/help/core.rs` — adds `RIPR_PILOT_SEAM_BUDGET` documentation
  to `PILOT_HELP`.

## Metrics

- Gate: all 7 new tests pass (4 in `seam_inventory.rs`, 3 in `agent_seam_packets.rs`).
- Promote to accepted when a large-workspace pilot run confirms artifact sizes are
  bounded by the 2,000-seam default.

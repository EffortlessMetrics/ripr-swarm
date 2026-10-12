# PR Plan

The automatic PR Plan placeholder is retired under #4987. It ran on five PR
activity events, checked out full history, recorded a best-effort changed-file
list, echoed ledger presence, and wrote a constant advisory summary. It had no
decision output or downstream qualification consumer.

This removes modest hosted fan-out; it does not establish dramatic savings or
complete #4987's broader workflow inventory and maturity work. Required Ready
qualification is unchanged. Existing `cargo xtask pr-ready`, `check-fast`,
`precommit`, and `check-pr` own local candidate shaping.

## Local advisory diagnostic

For an optional inspection, run this Bash recipe from the repository root.
Set `base` and `head` explicitly to the refs or immutable SHAs you intend to
compare; fetch missing refs first. The three-dot diff lists the head's changes
since the merge base, matching the former diagnostic's comparison semantics.

```bash
(
  set -euo pipefail
  base=origin/main
  head=HEAD
  base_sha=$(git rev-parse --verify "${base}^{commit}")
  head_sha=$(git rev-parse --verify "${head}^{commit}")
  printf 'Base: %s\nHead: %s\nChanged files:\n' "$base_sha" "$head_sha"
  git diff --name-only "$base_sha...$head_sha"
  printf '\nCI economics ledgers:\n'
  ls policy/ci-budget.toml policy/ci-lane-whitelist.toml policy/ci-risk-packs.toml
)
```

Missing refs, a missing merge base, Git errors, or absent ledgers exit nonzero;
an empty successful diff means no changed files. Ledger presence does not
validate policy contents. This diagnostic writes no forecast, selects no lanes,
and supplies no required check or readiness decision. It has no hosted workflow
or artifact upload.

## What it should do (target state)

Given the diff between the PR and its base, the PR Plan should:

1. Read the changed-file list.
2. Match those paths to risk packs in `policy/ci-risk-packs.toml`.
3. Resolve the union of `lanes` across matched packs.
4. Look up `base_lem` per lane in `policy/ci-lane-whitelist.toml`.
5. Apply runner multipliers from `policy/ci-budget.toml`.
6. Emit `target/ci/ci-plan.json` and a step-summary table.

The forecast is intentionally *coarse*: per-lane base LEM x runner multiplier,
summed across selected lanes. Once `ci-actuals.json` data exists, the forecast
can be replaced by learned per-lane estimates.

The numeric planner (`cargo xtask ci plan`) and `target/ci/ci-plan.json` are
planned contracts, not installed functionality. A future implementation must
establish decision value and choose its maturity boundary before earning any
automatic execution; retirement does not relocate the placeholder to Ready.

## Output schema (target)

```json
{
  "schema_version": 1,
  "repo": "ripr",
  "base": "<base SHA>",
  "head": "<head SHA>",
  "labels": ["full-ci", "ripr-waive"],
  "risk_packs": ["analysis_engine", "policy_xtask"],
  "selected_lanes": [
    { "id": "rust_fast_gate", "base_lem": 8, "runner_multiplier": 1.0 },
    { "id": "check_no_panic_family", "base_lem": 1, "runner_multiplier": 1.0 }
  ],
  "estimated_lem": 9,
  "band": "small",
  "warnings": []
}
```

`band` is one of `small | medium | large | release` (the band ids from
`[[budget_band]]` in `policy/ci-budget.toml`, also documented in
`docs/CI.md`'s Verification Economics section).

`warnings` is filled by the soft budget guard when the forecast crosses an
advisory threshold.

## Posture

| Stage                                    | Posture                |
| ---------------------------------------- | ---------------------- |
| Optional local diagnostic (current)       | changed files / presence only |
| Future: `xtask ci plan`                   | planned numeric advisory |
| Future: `ci-actuals.json` upload           | planned numeric advisory |
| Future: soft budget guard                | planned; enforcement remains off |

## Override and acknowledgement

Future forecast effects of labels documented in `docs/CI.md` (labels section):

- `full-ci` expects release-band forecast; the guard suppresses the
  warning.
- `release-check` uses the release-band mapping and runs the release-surface
  lanes that are currently wired. Today, that is the legacy Rust package list
  and publish dry-run proof.
- `ci-budget-ack` records that the author acknowledges an over-budget forecast
  at the `large` band (no budget effect).

## Why advisory first

A future blocking forecast on day one would either be too loose to catch real
overspend or too tight and reject ordinary PRs. Running the forecast in
advisory mode for at least two weeks gives the learned-estimate path
enough data to base a credible threshold on, and lets reviewers calibrate
which warnings they ignore.

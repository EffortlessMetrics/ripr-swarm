# RIPR-SPEC-0281: Pilot ranking refuted-count growth allowance

Status: proposed

Owner: test-infra

Created: 2026-10-20

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #7167 (the Pilot Ranking gate and its open design question)

Linked PRs:

- #8974 (this spec and the `pick_growth_allowance` rule)
- #8940 (re-blessed the baseline after #8572 added three refuted picks)

Support-tier impact:

- None. This is repository automation (`xtask dx-scoreboard`), not product
  analysis. Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- No baseline schema change and no workflow change. The metric flag lives in
  `benchmarks/dx_scoreboard/scoreboards.toml`.
- Pilot ranking itself is RIPR-SPEC-0237's. This spec owns only how the
  advisory `ranking` scoreboard compares the pooled refuted counts.

## Problem

The `ranking` scoreboard gates `ranking.pilot_refuted_top5` and
`ranking.pilot_refuted_top10` as absolute, lower-is-better pooled counts with
zero tolerance. A larger judged set adds picks of every verdict, so the counts
rise when the answer key judges more picks even while precision improves, and
a pull request whose merge ref predates a baseline refresh fails on the old
counts. Run 39393100959 on #8903 failed `refuted_top5 11 -> 14` and
`refuted_top10 16 -> 19` against the pre-#8940 baseline while precision rose
from 0.0833 to 0.1765 and from 0.0588 to 0.2692, and while main itself passed
at the same values.

## Behavior

### Rule 1: a flagged count may rise by the picks the run admitted

A metric with `pick_growth_allowance = true` may worsen by its normal margin
plus, per repository sample, the number of picks the run counted beyond the
baseline's. The pick count is read from the `refuted of N picks` wording in
both samples' evidence. The flag is valid only on a lower-is-better ingested
metric; any other use is a configuration error.

### Rule 2: no growth, no margin

When the pick count is flat or smaller, the margin is zero: one more refuted
pick fails exactly as before. A baseline or run whose evidence lacks the pick
count, or whose sample did not complete, gets no margin.

### Rule 3: each repository decides

For a flagged metric with repository samples completed in both reports, the
verdict is the set of per-repository regressions. The displayed worst values
and margin are informational. An unchanged repository does not revoke another
repository's margin, and one repository's growth does not excuse another's
increase.

### Rule 4: other gates are untouched

Precision, scored share, distinct-function share, pick count and the
confirmed counts keep their own floors. They judge the mix of the added picks,
so growth made of refuted picks that lowers precision past its floor still
fails on the precision rows.

## Decisions

1. **Margin by admitted picks, not by rate.** Adopted: allow growth equal to
   the picks admitted. Rejected: refuted share of picks, which still trips on
   #8572-style growth, and refuted share of judged picks, which only repeats
   precision.
2. **Evidence wording, not a new baseline field.** Adopted: parse the pick
   count both baselines already carry, so no baseline edit is needed.
   Rejected: a pick-identity baseline, which cannot be produced without a
   new scoring run and is the follow-up for a matched-set join.

## Required Evidence

- A pre-#8940 baseline against #8572-style growth passes; four more refuted
  top-5 picks for three more picks, one more refuted pick at a flat pick count
  and a shrinking pick count with more refuted fail.
- A refreshed baseline with three more all-refuted picks fails on top-10
  precision alone.
- A two-repository sample set passes when only the growing repository grows
  and fails for a flipped pick, borrowed growth and a legacy baseline.
- The flag is rejected on a higher-is-better metric.

## Non-Goals

- A pick-level matched-set join, which needs the baseline to record each
  pick's identity (#7167).
- The answer-key caveats of #6696 and #6913, and the October 8 attribution.
- Refreshing the baseline or editing `pilot-ranking.yml`.
- Any claim about runtime mutation outcomes or ranking sufficiency.

## Acceptance Examples

1. Top 5 goes from 1 confirmed and 11 refuted of 21 picks to 3 and 14 of 24:
   the refuted count rose by 3 for 3 admitted picks and passes.
2. Top 5 at 3 and 15 of 24 picks: 4 more refuted for 3 more picks, so
   `ranking.pilot_refuted_top5` fails.
3. Top 10 at 1 and 17 of 32 picks, the baseline's pick count: fails.
4. Repositories a and b at 0 and 10 refuted; b grows to 12 of 22 picks from
   20 and a is unchanged: passes. b at 11 of 20: fails.
5. A baseline sample whose evidence has no pick count: b at 12 of 22 fails.

## Test Mapping

- `xtask/src/reports/dx_scoreboard/tests.rs`: examples 1 to 3, the dilution
  case, and the configuration check.
- `xtask/src/reports/dx_scoreboard/tests.rs`: examples 4 and 5.

## Implementation Mapping

- `benchmarks/dx_scoreboard/scoreboards.toml`: `pick_growth_allowance` on the
  two refuted metrics.
- `xtask/src/reports/dx_scoreboard.rs`: `MetricDef::pick_growth_allowance`,
  `evidence_picks`, `repo_pick_growth`, `pick_growth`, `repo_regressions`,
  `compare_with_baseline`.
- `benchmarks/dx_scoreboard/README.md`: the ranking lane note.

## Metrics

- `unit_test_pass_rate`: the mapped tests pass. Target: 100 percent.

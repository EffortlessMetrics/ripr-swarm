# ripr dogfood report

Status: pass

Mode: advisory

This report runs `ripr check --mode fast` against stable in-repo fixture diffs. It records current product output for review. The findings stay advisory, but the command exits non-zero whenever this status is `warn`.

## Summary

- `boundary_gap`: 1 finding(s), 1 stop-reason field(s), 174 ms
- `weak_error_oracle`: 2 finding(s), 2 stop-reason field(s), 44 ms

## Runs

### `boundary_gap`

- Root: `fixtures/boundary_gap/input`
- Diff: `fixtures/boundary_gap/diff.patch`
- Actual outputs: `target/ripr/dogfood/boundary_gap`
- Findings: 1
- Exposure classes:
  - `exposed`: 0
  - `infection_unknown`: 0
  - `no_static_path`: 0
  - `propagation_unknown`: 0
  - `reachable_unrevealed`: 0
  - `static_unknown`: 0
  - `weakly_exposed`: 1
- Errors: none

### `weak_error_oracle`

- Root: `fixtures/weak_error_oracle/input`
- Diff: `fixtures/weak_error_oracle/diff.patch`
- Actual outputs: `target/ripr/dogfood/weak_error_oracle`
- Findings: 2
- Exposure classes:
  - `exposed`: 0
  - `infection_unknown`: 0
  - `no_static_path`: 0
  - `propagation_unknown`: 0
  - `reachable_unrevealed`: 0
  - `static_unknown`: 0
  - `weakly_exposed`: 2
- Errors: none

## First Useful Action Receipts

Each case renders through `ripr first-action` from committed inputs and compares the produced `first-useful-action.{json,md}` to the checked goldens for the documented Campaign 22 routes. They are advisory projections over existing artifacts; they do not rerun hidden analysis, edit source, generate tests, call providers, run mutation testing, invent policy, or change CI blocking.

- Default CI blocking: no
- Rendered cases: 6/6
- Receipt outputs: `fixtures/boundary_gap/expected/first-useful-action/<case>/first-useful-action.{json,md}`

| Case | Status | Action | Audience | Selected | Static movement |
| --- | --- | --- | --- | --- | --- |
| `actionable` | `actionable` | `write_focused_test` | `developer` | yes | `unknown` |
| `baseline-only` | `baseline_only` | `acknowledge_baseline` | `reviewer` | yes | `unknown` |
| `stale` | `stale` | `refresh_evidence` | `developer` | yes | `unknown` |
| `missing-required-artifact` | `missing_required_artifact` | `generate_missing_artifact` | `agent` | no | `unknown` |
| `unchanged-after-attempt` | `missing_required_artifact` | `generate_missing_artifact` | `agent` | no | `unknown` |
| `no-actionable-seam` | `no_actionable_seam` | `no_action` | `developer` | no | `unknown` |

### First Action `actionable`

- Status: `actionable`
- Expected status: `actionable`
- Action: `write_focused_test`
- Expected action: `write_focused_test`
- Audience: `developer`
- Expected audience: `developer`
- Selected seam: true
- Expected selected seam: true
- Static movement: `unknown`
- Expected static movement: `unknown`
- Receipt JSON: `fixtures/boundary_gap/expected/first-useful-action/actionable/first-useful-action.json`
- Receipt Markdown: `fixtures/boundary_gap/expected/first-useful-action/actionable/first-useful-action.md`
- Expected directory: `fixtures/boundary_gap/expected/first-useful-action/actionable`
- Errors: none

### First Action `baseline-only`

- Status: `baseline_only`
- Expected status: `baseline_only`
- Action: `acknowledge_baseline`
- Expected action: `acknowledge_baseline`
- Audience: `reviewer`
- Expected audience: `reviewer`
- Selected seam: true
- Expected selected seam: true
- Static movement: `unknown`
- Expected static movement: `unknown`
- Receipt JSON: `fixtures/boundary_gap/expected/first-useful-action/baseline-only/first-useful-action.json`
- Receipt Markdown: `fixtures/boundary_gap/expected/first-useful-action/baseline-only/first-useful-action.md`
- Expected directory: `fixtures/boundary_gap/expected/first-useful-action/baseline-only`
- Errors: none

### First Action `stale`

- Status: `stale`
- Expected status: `stale`
- Action: `refresh_evidence`
- Expected action: `refresh_evidence`
- Audience: `developer`
- Expected audience: `developer`
- Selected seam: true
- Expected selected seam: true
- Static movement: `unknown`
- Expected static movement: `unknown`
- Receipt JSON: `fixtures/boundary_gap/expected/first-useful-action/stale/first-useful-action.json`
- Receipt Markdown: `fixtures/boundary_gap/expected/first-useful-action/stale/first-useful-action.md`
- Expected directory: `fixtures/boundary_gap/expected/first-useful-action/stale`
- Errors: none

### First Action `missing-required-artifact`

- Status: `missing_required_artifact`
- Expected status: `missing_required_artifact`
- Action: `generate_missing_artifact`
- Expected action: `generate_missing_artifact`
- Audience: `agent`
- Expected audience: `agent`
- Selected seam: false
- Expected selected seam: false
- Static movement: `unknown`
- Expected static movement: `unknown`
- Receipt JSON: `fixtures/boundary_gap/expected/first-useful-action/missing-required-artifact/first-useful-action.json`
- Receipt Markdown: `fixtures/boundary_gap/expected/first-useful-action/missing-required-artifact/first-useful-action.md`
- Expected directory: `fixtures/boundary_gap/expected/first-useful-action/missing-required-artifact`
- Errors: none

### First Action `unchanged-after-attempt`

- Status: `missing_required_artifact`
- Expected status: `missing_required_artifact`
- Action: `generate_missing_artifact`
- Expected action: `generate_missing_artifact`
- Audience: `agent`
- Expected audience: `agent`
- Selected seam: false
- Expected selected seam: false
- Static movement: `unknown`
- Expected static movement: `unknown`
- Receipt JSON: `fixtures/boundary_gap/expected/first-useful-action/unchanged-after-attempt/first-useful-action.json`
- Receipt Markdown: `fixtures/boundary_gap/expected/first-useful-action/unchanged-after-attempt/first-useful-action.md`
- Expected directory: `fixtures/boundary_gap/expected/first-useful-action/unchanged-after-attempt`
- Errors: none

### First Action `no-actionable-seam`

- Status: `no_actionable_seam`
- Expected status: `no_actionable_seam`
- Action: `no_action`
- Expected action: `no_action`
- Audience: `developer`
- Expected audience: `developer`
- Selected seam: false
- Expected selected seam: false
- Static movement: `unknown`
- Expected static movement: `unknown`
- Receipt JSON: `fixtures/boundary_gap/expected/first-useful-action/no-actionable-seam/first-useful-action.json`
- Receipt Markdown: `fixtures/boundary_gap/expected/first-useful-action/no-actionable-seam/first-useful-action.md`
- Expected directory: `fixtures/boundary_gap/expected/first-useful-action/no-actionable-seam`
- Errors: none

## First Successful PR Receipts

These receipts validate checked `start-here.{json,md}` fixture outputs for the first successful PR path. They record that the first screen selects a repairable Rust gap or a clear no-action/blocked state while preserving advisory limits and gate-authority separation.

- Default CI blocking: no
- Evidence source: committed declarations only; no producer runs in this family (#4267). The in-process renderer is compared to these fixtures by the crate test `first_successful_pr_fixture_corpus_matches_expected_outputs`.
- Receipt outputs: `fixtures/first_successful_pr/<case>/expected/start-here.{json,md}`

| Metric | Value |
| --- | ---: |
| `first_run_packets_total` | 6 |
| `first_run_top_gap_selected_total` | 4 |
| `first_run_no_action_total` | 1 |
| `first_run_blocked_total` | 1 |
| `first_run_missing_artifact_total` | 0 |
| `first_run_stale_artifact_total` | 0 |
| `first_run_wrong_root_total` | 0 |
| `first_run_malformed_artifact_total` | 0 |
| `first_run_timeout_total` | 0 |

| Case | Status | State | Top gap | Verify | Next |
| --- | --- | --- | --- | --- | --- |
| `boundary-gap` | `actionable` | `top_gap` | `MissingBoundaryAssertion` | `cargo xtask fixtures boundary_gap` | none |
| `output-contract-gap` | `actionable` | `top_gap` | `MissingOutputContract` | `cargo xtask goldens check` | none |
| `python-preview-gap` | `actionable` | `top_gap` | `MissingBoundaryAssertion` | `pytest tests/test_pricing.py::test_calculate_discount_smoke` | none |
| `typescript-preview-gap` | `actionable` | `top_gap` | `MissingBoundaryAssertion` | `npx --no-install jest tests/discount.test.ts` | none |
| `empty-diff` | `no_action` | `empty_diff` | `none` | none | none |
| `blocked-ledger` | `blocked` | `blocked_artifact` | `none` | none | `ripr check --root <cwd>/fixtures/first_successful_pr/blocked-ledger --mode instant --format repo-exposure-json > <cwd>/fixtures/first_successful_pr/blocked-ledger/target/ripr/reports/repo-exposure.json && ripr reports gap-ledger --root <cwd>/fixtures/first_successful_pr/blocked-ledger --repo-exposure <cwd>/fixtures/first_successful_pr/blocked-ledger/target/ripr/reports/repo-exposure.json --out <cwd>/fixtures/first_successful_pr/blocked-ledger/inputs/reports/gap-decision-ledger.json --out-md <cwd>/fixtures/first_successful_pr/blocked-ledger/inputs/reports/gap-decision-ledger.md` |

### First PR `boundary-gap`

- Status: `actionable`
- Expected status: `actionable`
- State: `top_gap`
- Expected state: `top_gap`
- Top gap kind: `MissingBoundaryAssertion`
- Receipt JSON: `fixtures/first_successful_pr/boundary-gap/expected/start-here.json`
- Receipt Markdown: `fixtures/first_successful_pr/boundary-gap/expected/start-here.md`
- Expected directory: `fixtures/first_successful_pr/boundary-gap/expected`
- Description: A repairable Rust boundary gap becomes the top first-run repair.
- Errors: none

### First PR `output-contract-gap`

- Status: `actionable`
- Expected status: `actionable`
- State: `top_gap`
- Expected state: `top_gap`
- Top gap kind: `MissingOutputContract`
- Receipt JSON: `fixtures/first_successful_pr/output-contract-gap/expected/start-here.json`
- Receipt Markdown: `fixtures/first_successful_pr/output-contract-gap/expected/start-here.md`
- Expected directory: `fixtures/first_successful_pr/output-contract-gap/expected`
- Description: A presentation/output text gap routes to output golden repair.
- Errors: none

### First PR `python-preview-gap`

- Status: `actionable`
- Expected status: `actionable`
- State: `top_gap`
- Expected state: `top_gap`
- Top gap kind: `MissingBoundaryAssertion`
- Receipt JSON: `fixtures/first_successful_pr/python-preview-gap/expected/start-here.json`
- Receipt Markdown: `fixtures/first_successful_pr/python-preview-gap/expected/start-here.md`
- Expected directory: `fixtures/first_successful_pr/python-preview-gap/expected`
- Description: A repairable Python preview gap becomes a preview-limited first-run repair.
- Errors: none

### First PR `typescript-preview-gap`

- Status: `actionable`
- Expected status: `actionable`
- State: `top_gap`
- Expected state: `top_gap`
- Top gap kind: `MissingBoundaryAssertion`
- Receipt JSON: `fixtures/first_successful_pr/typescript-preview-gap/expected/start-here.json`
- Receipt Markdown: `fixtures/first_successful_pr/typescript-preview-gap/expected/start-here.md`
- Expected directory: `fixtures/first_successful_pr/typescript-preview-gap/expected`
- Description: A repairable TypeScript preview gap becomes a preview-limited first-run repair.
- Errors: none

### First PR `empty-diff`

- Status: `no_action`
- Expected status: `no_action`
- State: `empty_diff`
- Expected state: `empty_diff`
- Top gap kind: `none`
- Receipt JSON: `fixtures/first_successful_pr/empty-diff/expected/start-here.json`
- Receipt Markdown: `fixtures/first_successful_pr/empty-diff/expected/start-here.md`
- Expected directory: `fixtures/first_successful_pr/empty-diff/expected`
- Description: An empty diff emits schema-valid no-action output.
- Errors: none

### First PR `blocked-ledger`

- Status: `blocked`
- Expected status: `blocked`
- State: `blocked_artifact`
- Expected state: `blocked_artifact`
- Top gap kind: `none`
- Receipt JSON: `fixtures/first_successful_pr/blocked-ledger/expected/start-here.json`
- Receipt Markdown: `fixtures/first_successful_pr/blocked-ledger/expected/start-here.md`
- Expected directory: `fixtures/first_successful_pr/blocked-ledger/expected`
- Description: A blocked ledger stays blocked and points to regeneration.
- Errors: none

## PR Review Front Panel Receipts

Each case renders through `ripr pr-review front-panel` from its corpus inputs and compares the produced `pr-review-front-panel.{json,md}` to the checked goldens for the documented Campaign 24 reviewer routes. They are advisory projections over explicit existing artifacts; they do not rerun hidden analysis, edit source, generate tests, call providers, run mutation testing, invent policy, publish inline comments, or change CI blocking.

- Default CI blocking: no
- Rendered cases: 13/13
- Receipt outputs: `fixtures/boundary_gap/expected/pr-review-front-panel/<case>/pr-review-front-panel.{json,md}`

| Case | Status | Top issue | Policy | Placement | Movement | Coverage/grip | New | Resolved | Blocking | Warnings |
| --- | --- | --- | --- | --- | --- | --- | ---: | ---: | ---: | ---: |
| `advisory_only` | `advisory` | `no_actionable_seam` | `none` | `not_available` | `not_available` | `not_available` | 0 | 0 | 0 | 0 |
| `python_heuristic_only_no_action` | `advisory` | `heuristic_only` | `none` | `not_available` | `not_available` | `not_available` | 0 | 0 | 0 | 0 |
| `actionable` | `advisory` | `actionable` | `new_policy_eligible` | `changed_line` | `unknown` | `not_available` | 1 | 0 | 0 | 0 |
| `repair_start` | `advisory` | `actionable` | `new_policy_eligible` | `changed_line` | `unknown` | `not_available` | 1 | 0 | 0 | 0 |
| `summary_only` | `advisory` | `summary_only` | `new_policy_eligible` | `summary_only` | `unknown` | `not_available` | 1 | 0 | 0 | 1 |
| `acknowledged` | `acknowledged` | `actionable` | `waived` | `changed_line` | `unknown` | `not_available` | 0 | 0 | 0 | 0 |
| `suppressed` | `advisory` | `baseline_only` | `suppressed` | `not_available` | `not_available` | `not_available` | 0 | 0 | 0 | 0 |
| `baseline_resolved` | `advisory` | `already_improved` | `baseline` | `not_available` | `resolved` | `not_available` | 1 | 3 | 0 | 0 |
| `blocked` | `blocked` | `actionable` | `blocking` | `changed_line` | `unknown` | `not_available` | 1 | 0 | 1 | 0 |
| `missing_proof` | `incomplete` | `missing_required_input` | `none` | `not_available` | `unknown` | `not_available` | 0 | 0 | 0 | 1 |
| `coverage_flat_grip_improved` | `advisory` | `already_improved` | `none` | `changed_line` | `improved` | `flat_coverage_grip_improved` | 0 | 3 | 0 | 0 |
| `mixed_health_keeps_repair_visible` | `advisory` | `unchanged_after_attempt` | `none` | `changed_line` | `unchanged` | `flat_coverage_grip_improved` | 0 | 0 | 0 | 0 |
| `stale_same_path_receipt_fails_closed` | `advisory` | `unchanged_after_attempt` | `none` | `changed_line` | `unchanged` | `not_available` | 0 | 0 | 0 | 0 |

### Front Panel `advisory_only`

- Status: `advisory`
- Expected status: `advisory`
- Top issue state: `no_actionable_seam`
- Expected top issue state: `no_actionable_seam`
- Policy state: `none`
- Expected policy state: `none`
- Placement: `not_available`
- Expected placement: `not_available`
- Movement: `not_available`
- Expected movement: `not_available`
- Coverage/grip: `not_available`
- Expected coverage/grip: `not_available`
- Counts: new 0, resolved 0, blocking 0, warnings 0
- Expected counts: new 0, resolved 0, blocking 0, warnings 0
- Receipt JSON: `fixtures/boundary_gap/expected/pr-review-front-panel/advisory-only/pr-review-front-panel.json`
- Receipt Markdown: `fixtures/boundary_gap/expected/pr-review-front-panel/advisory-only/pr-review-front-panel.md`
- Reason: Default generated CI should remain advisory and readable when there is no safe next action.
- Errors: none

### Front Panel `python_heuristic_only_no_action`

- Status: `advisory`
- Expected status: `advisory`
- Top issue state: `heuristic_only`
- Expected top issue state: `heuristic_only`
- Policy state: `none`
- Expected policy state: `none`
- Placement: `not_available`
- Expected placement: `not_available`
- Movement: `not_available`
- Expected movement: `not_available`
- Coverage/grip: `not_available`
- Expected coverage/grip: `not_available`
- Counts: new 0, resolved 0, blocking 0, warnings 0
- Expected counts: new 0, resolved 0, blocking 0, warnings 0
- Receipt JSON: `fixtures/boundary_gap/expected/pr-review-front-panel/python-heuristic-only-no-action/pr-review-front-panel.json`
- Receipt Markdown: `fixtures/boundary_gap/expected/pr-review-front-panel/python-heuristic-only-no-action/pr-review-front-panel.md`
- Reason: The front panel should surface Python no-action states without emitting a repair packet or implying a missing-proof work order.
- Errors: none

### Front Panel `actionable`

- Status: `advisory`
- Expected status: `advisory`
- Top issue state: `actionable`
- Expected top issue state: `actionable`
- Policy state: `new_policy_eligible`
- Expected policy state: `new_policy_eligible`
- Placement: `changed_line`
- Expected placement: `changed_line`
- Movement: `unknown`
- Expected movement: `unknown`
- Coverage/grip: `not_available`
- Expected coverage/grip: `not_available`
- Counts: new 1, resolved 0, blocking 0, warnings 0
- Expected counts: new 1, resolved 0, blocking 0, warnings 0
- Receipt JSON: `fixtures/boundary_gap/expected/pr-review-front-panel/actionable/pr-review-front-panel.json`
- Receipt Markdown: `fixtures/boundary_gap/expected/pr-review-front-panel/actionable/pr-review-front-panel.md`
- Reason: The front panel should show the top focused-test action first when a PR-local seam is line-placeable.
- Errors: none

### Front Panel `repair_start`

- Status: `advisory`
- Expected status: `advisory`
- Top issue state: `actionable`
- Expected top issue state: `actionable`
- Policy state: `new_policy_eligible`
- Expected policy state: `new_policy_eligible`
- Placement: `changed_line`
- Expected placement: `changed_line`
- Movement: `unknown`
- Expected movement: `unknown`
- Coverage/grip: `not_available`
- Expected coverage/grip: `not_available`
- Counts: new 1, resolved 0, blocking 0, warnings 0
- Expected counts: new 1, resolved 0, blocking 0, warnings 0
- Receipt JSON: `fixtures/boundary_gap/expected/pr-review-front-panel/repair-start/pr-review-front-panel.json`
- Receipt Markdown: `fixtures/boundary_gap/expected/pr-review-front-panel/repair-start/pr-review-front-panel.md`
- Reason: When the review card carries the repair start (named only past the fail-closed repair-packet flip), the front panel leads with that exact command instead of synthesizing a workflow start from the seam id.
- Errors: none

### Front Panel `summary_only`

- Status: `advisory`
- Expected status: `advisory`
- Top issue state: `summary_only`
- Expected top issue state: `summary_only`
- Policy state: `new_policy_eligible`
- Expected policy state: `new_policy_eligible`
- Placement: `summary_only`
- Expected placement: `summary_only`
- Movement: `unknown`
- Expected movement: `unknown`
- Coverage/grip: `not_available`
- Expected coverage/grip: `not_available`
- Counts: new 1, resolved 0, blocking 0, warnings 1
- Expected counts: new 1, resolved 0, blocking 0, warnings 1
- Receipt JSON: `fixtures/boundary_gap/expected/pr-review-front-panel/summary-only/pr-review-front-panel.json`
- Receipt Markdown: `fixtures/boundary_gap/expected/pr-review-front-panel/summary-only/pr-review-front-panel.md`
- Reason: Unsafe placement should stay visible as summary-only guidance instead of producing a misleading inline annotation.
- Errors: none

### Front Panel `acknowledged`

- Status: `acknowledged`
- Expected status: `acknowledged`
- Top issue state: `actionable`
- Expected top issue state: `actionable`
- Policy state: `waived`
- Expected policy state: `waived`
- Placement: `changed_line`
- Expected placement: `changed_line`
- Movement: `unknown`
- Expected movement: `unknown`
- Coverage/grip: `not_available`
- Expected coverage/grip: `not_available`
- Counts: new 0, resolved 0, blocking 0, warnings 0
- Expected counts: new 0, resolved 0, blocking 0, warnings 0
- Receipt JSON: `fixtures/boundary_gap/expected/pr-review-front-panel/acknowledged/pr-review-front-panel.json`
- Receipt Markdown: `fixtures/boundary_gap/expected/pr-review-front-panel/acknowledged/pr-review-front-panel.md`
- Reason: Waivers should be visible acknowledgements, not hidden success.
- Errors: none

### Front Panel `suppressed`

- Status: `advisory`
- Expected status: `advisory`
- Top issue state: `baseline_only`
- Expected top issue state: `baseline_only`
- Policy state: `suppressed`
- Expected policy state: `suppressed`
- Placement: `not_available`
- Expected placement: `not_available`
- Movement: `not_available`
- Expected movement: `not_available`
- Coverage/grip: `not_available`
- Expected coverage/grip: `not_available`
- Counts: new 0, resolved 0, blocking 0, warnings 0
- Expected counts: new 0, resolved 0, blocking 0, warnings 0
- Receipt JSON: `fixtures/boundary_gap/expected/pr-review-front-panel/suppressed/pr-review-front-panel.json`
- Receipt Markdown: `fixtures/boundary_gap/expected/pr-review-front-panel/suppressed/pr-review-front-panel.md`
- Reason: Suppressions are durable policy exceptions and must stay visible in PR review.
- Errors: none

### Front Panel `baseline_resolved`

- Status: `advisory`
- Expected status: `advisory`
- Top issue state: `already_improved`
- Expected top issue state: `already_improved`
- Policy state: `baseline`
- Expected policy state: `baseline`
- Placement: `not_available`
- Expected placement: `not_available`
- Movement: `resolved`
- Expected movement: `resolved`
- Coverage/grip: `not_available`
- Expected coverage/grip: `not_available`
- Counts: new 1, resolved 3, blocking 0, warnings 0
- Expected counts: new 1, resolved 3, blocking 0, warnings 0
- Receipt JSON: `fixtures/boundary_gap/expected/pr-review-front-panel/baseline-resolved/pr-review-front-panel.json`
- Receipt Markdown: `fixtures/boundary_gap/expected/pr-review-front-panel/baseline-resolved/pr-review-front-panel.md`
- Reason: The front panel should show improvement and not only punish new debt.
- Errors: none

### Front Panel `blocked`

- Status: `blocked`
- Expected status: `blocked`
- Top issue state: `actionable`
- Expected top issue state: `actionable`
- Policy state: `blocking`
- Expected policy state: `blocking`
- Placement: `changed_line`
- Expected placement: `changed_line`
- Movement: `unknown`
- Expected movement: `unknown`
- Coverage/grip: `not_available`
- Expected coverage/grip: `not_available`
- Counts: new 1, resolved 0, blocking 1, warnings 0
- Expected counts: new 1, resolved 0, blocking 1, warnings 0
- Receipt JSON: `fixtures/boundary_gap/expected/pr-review-front-panel/blocked/pr-review-front-panel.json`
- Receipt Markdown: `fixtures/boundary_gap/expected/pr-review-front-panel/blocked/pr-review-front-panel.md`
- Reason: A blocked run should still route a repair action and point to gate-decision as pass/fail authority.
- Errors: none

### Front Panel `missing_proof`

- Status: `incomplete`
- Expected status: `incomplete`
- Top issue state: `missing_required_input`
- Expected top issue state: `missing_required_input`
- Policy state: `none`
- Expected policy state: `none`
- Placement: `not_available`
- Expected placement: `not_available`
- Movement: `unknown`
- Expected movement: `unknown`
- Coverage/grip: `not_available`
- Expected coverage/grip: `not_available`
- Counts: new 0, resolved 0, blocking 0, warnings 1
- Expected counts: new 0, resolved 0, blocking 0, warnings 1
- Receipt JSON: `fixtures/boundary_gap/expected/pr-review-front-panel/missing-proof/pr-review-front-panel.json`
- Receipt Markdown: `fixtures/boundary_gap/expected/pr-review-front-panel/missing-proof/pr-review-front-panel.md`
- Reason: Missing proof should tell users to regenerate an explicit artifact instead of inferring a front-panel story.
- Errors: none

### Front Panel `coverage_flat_grip_improved`

- Status: `advisory`
- Expected status: `advisory`
- Top issue state: `already_improved`
- Expected top issue state: `already_improved`
- Policy state: `none`
- Expected policy state: `none`
- Placement: `changed_line`
- Expected placement: `changed_line`
- Movement: `improved`
- Expected movement: `improved`
- Coverage/grip: `flat_coverage_grip_improved`
- Expected coverage/grip: `flat_coverage_grip_improved`
- Counts: new 0, resolved 3, blocking 0, warnings 0
- Expected counts: new 0, resolved 3, blocking 0, warnings 0
- Receipt JSON: `fixtures/boundary_gap/expected/pr-review-front-panel/coverage-flat-grip-improved/pr-review-front-panel.json`
- Receipt Markdown: `fixtures/boundary_gap/expected/pr-review-front-panel/coverage-flat-grip-improved/pr-review-front-panel.md`
- Reason: The front panel should teach that behavioral grip can improve even when line coverage does not move.
- Errors: none

### Front Panel `mixed_health_keeps_repair_visible`

- Status: `advisory`
- Expected status: `advisory`
- Top issue state: `unchanged_after_attempt`
- Expected top issue state: `unchanged_after_attempt`
- Policy state: `none`
- Expected policy state: `none`
- Placement: `changed_line`
- Expected placement: `changed_line`
- Movement: `unchanged`
- Expected movement: `unchanged`
- Coverage/grip: `flat_coverage_grip_improved`
- Expected coverage/grip: `flat_coverage_grip_improved`
- Counts: new 0, resolved 0, blocking 0, warnings 0
- Expected counts: new 0, resolved 0, blocking 0, warnings 0
- Receipt JSON: `fixtures/boundary_gap/expected/pr-review-front-panel/mixed-health/pr-review-front-panel.json`
- Receipt Markdown: `fixtures/boundary_gap/expected/pr-review-front-panel/mixed-health/pr-review-front-panel.md`
- Reason: A repair-bearing proof must outrank aggregate improvement so unchanged and unknown work cannot collapse to a no-repair panel.
- Errors: none

### Front Panel `stale_same_path_receipt_fails_closed`

- Status: `advisory`
- Expected status: `advisory`
- Top issue state: `unchanged_after_attempt`
- Expected top issue state: `unchanged_after_attempt`
- Policy state: `none`
- Expected policy state: `none`
- Placement: `changed_line`
- Expected placement: `changed_line`
- Movement: `unchanged`
- Expected movement: `unchanged`
- Coverage/grip: `not_available`
- Expected coverage/grip: `not_available`
- Counts: new 0, resolved 0, blocking 0, warnings 0
- Expected counts: new 0, resolved 0, blocking 0, warnings 0
- Receipt JSON: `fixtures/boundary_gap/expected/pr-review-front-panel/stale-same-path-receipt/pr-review-front-panel.json`
- Receipt Markdown: `fixtures/boundary_gap/expected/pr-review-front-panel/stale-same-path-receipt/pr-review-front-panel.md`
- Reason: Same-path provenance cannot overwrite selected-proof authority; a regenerated receipt must match seam, movement, and available classes before fallback admission.
- Errors: none

## Report Packet Index Receipts

These receipts render each `report-packet-index` corpus case through the shipped `ripr reports index` route and check what the renderer produced. They verify reviewer-first grouping, missing-surface counts, start-here discovery, gate-authority visibility, and advisory limits, and compare the rendered JSON and Markdown to the committed goldens, without rerunning hidden analysis or changing pass/fail authority.

- Default CI blocking: no
- Rendered cases: 7 of 7
- Packet inputs: `fixtures/boundary_gap/expected/report-packet-index/<case>/packet`
- Compared goldens: `fixtures/boundary_gap/expected/report-packet-index/<case>/index.{json,md}`

| Case | Status | Missing | Warnings | Failures | Start here | Gate authority | Groups |
| --- | --- | ---: | ---: | ---: | --- | --- | --- |
| `complete_packet` | `warn` | 0 | 0 | 0 | yes | yes | `start_here, pr_review_story, repair_agent_handoff, evidence_movement, policy_gates, calibration, validation_receipts, sarif_badges` |
| `sparse_advisory` | `warn` | 2 | 2 | 0 | yes | no | `start_here, pr_review_story, repair_agent_handoff, calibration, validation_receipts` |
| `missing_front_panel` | `warn` | 1 | 1 | 0 | no | no | `start_here, pr_review_story, repair_agent_handoff` |
| `blocked_gate` | `fail` | 0 | 0 | 1 | yes | yes | `start_here, pr_review_story, repair_agent_handoff, policy_gates` |
| `missing_assistant_proof` | `warn` | 1 | 1 | 0 | yes | no | `start_here, pr_review_story, repair_agent_handoff` |
| `missing_receipts` | `warn` | 2 | 2 | 0 | yes | no | `start_here, pr_review_story, repair_agent_handoff, validation_receipts` |
| `coverage_grip_present` | `warn` | 0 | 0 | 0 | yes | no | `start_here, pr_review_story, repair_agent_handoff, calibration, validation_receipts` |

### Report Packet Index `complete_packet`

- Rendered: true
- Packet input: `fixtures/boundary_gap/expected/report-packet-index/complete-packet/packet`
- Render command: `ripr reports index --root . --reports-dir target/ripr/reports --review-dir target/ripr/review --receipts-dir target/ripr/receipts --workflow-dir target/ripr/workflow --agent-dir target/ripr/agent --pilot-dir target/ripr/pilot --ci-dir target/ci --out target/ripr/reports/index.json --out-md target/ripr/reports/index.md`
- Status: `warn`
- Expected status: `warn`
- Counts: missing 0, warnings 0, failures 0
- Expected counts: missing 0, warnings 0, failures 0
- Start here available: true (expected true)
- Gate authority present: true (expected true)
- Required groups: `start_here, pr_review_story, repair_agent_handoff, evidence_movement, policy_gates, calibration, validation_receipts, sarif_badges`
- Actual groups: `start_here, pr_review_story, repair_agent_handoff, evidence_movement, policy_gates, calibration, validation_receipts, sarif_badges`
- Receipt JSON: `target/ripr/dogfood/report-packet-index/complete_packet/target/ripr/reports/index.json`
- Receipt Markdown: `target/ripr/dogfood/report-packet-index/complete_packet/target/ripr/reports/index.md`
- Expected report: `fixtures/boundary_gap/expected/report-packet-index/complete-packet/index.json`
- Expected Markdown: `fixtures/boundary_gap/expected/report-packet-index/complete-packet/index.md`
- Actual outputs: `target/ripr/dogfood/report-packet-index/complete_packet`
- Reason: all reviewer-first packet surfaces are present: A complete packet should tell reviewers to start at the PR front panel while preserving gate-decision authority.
- Errors: none

### Report Packet Index `sparse_advisory`

- Rendered: true
- Packet input: `fixtures/boundary_gap/expected/report-packet-index/sparse-advisory/packet`
- Render command: `ripr reports index --root . --reports-dir target/ripr/reports --review-dir target/ripr/review --receipts-dir target/ripr/receipts --workflow-dir target/ripr/workflow --agent-dir target/ripr/agent --pilot-dir target/ripr/pilot --ci-dir target/ci --out target/ripr/reports/index.json --out-md target/ripr/reports/index.md`
- Status: `warn`
- Expected status: `warn`
- Counts: missing 2, warnings 2, failures 0
- Expected counts: missing 2, warnings 2, failures 0
- Start here available: true (expected true)
- Gate authority present: false (expected false)
- Required groups: `start_here, pr_review_story, repair_agent_handoff, calibration, validation_receipts`
- Actual groups: `start_here, pr_review_story, repair_agent_handoff, calibration, validation_receipts`
- Receipt JSON: `target/ripr/dogfood/report-packet-index/sparse_advisory/target/ripr/reports/index.json`
- Receipt Markdown: `target/ripr/dogfood/report-packet-index/sparse_advisory/target/ripr/reports/index.md`
- Expected report: `fixtures/boundary_gap/expected/report-packet-index/sparse-advisory/index.json`
- Expected Markdown: `fixtures/boundary_gap/expected/report-packet-index/sparse-advisory/index.md`
- Actual outputs: `target/ripr/dogfood/report-packet-index/sparse_advisory`
- Reason: advisory packet with only PR guidance and local validation reports: Sparse adoption should stay advisory and explain which optional surfaces can be regenerated.
- Errors: none

### Report Packet Index `missing_front_panel`

- Rendered: true
- Packet input: `fixtures/boundary_gap/expected/report-packet-index/missing-front-panel/packet`
- Render command: `ripr reports index --root . --reports-dir target/ripr/reports --review-dir target/ripr/review --receipts-dir target/ripr/receipts --workflow-dir target/ripr/workflow --agent-dir target/ripr/agent --pilot-dir target/ripr/pilot --ci-dir target/ci --out target/ripr/reports/index.json --out-md target/ripr/reports/index.md`
- Status: `warn`
- Expected status: `warn`
- Counts: missing 1, warnings 1, failures 0
- Expected counts: missing 1, warnings 1, failures 0
- Start here available: false (expected false)
- Gate authority present: false (expected false)
- Required groups: `start_here, pr_review_story, repair_agent_handoff`
- Actual groups: `start_here, pr_review_story, repair_agent_handoff`
- Receipt JSON: `target/ripr/dogfood/report-packet-index/missing_front_panel/target/ripr/reports/index.json`
- Receipt Markdown: `target/ripr/dogfood/report-packet-index/missing_front_panel/target/ripr/reports/index.md`
- Expected report: `fixtures/boundary_gap/expected/report-packet-index/missing-front-panel/index.json`
- Expected Markdown: `fixtures/boundary_gap/expected/report-packet-index/missing-front-panel/index.md`
- Actual outputs: `target/ripr/dogfood/report-packet-index/missing_front_panel`
- Reason: packet has supporting artifacts but no first-screen front panel: Missing start-here front panel should be visible instead of forcing artifact archaeology.
- Errors: none

### Report Packet Index `blocked_gate`

- Rendered: true
- Packet input: `fixtures/boundary_gap/expected/report-packet-index/blocked-gate/packet`
- Render command: `ripr reports index --root . --reports-dir target/ripr/reports --review-dir target/ripr/review --receipts-dir target/ripr/receipts --workflow-dir target/ripr/workflow --agent-dir target/ripr/agent --pilot-dir target/ripr/pilot --ci-dir target/ci --out target/ripr/reports/index.json --out-md target/ripr/reports/index.md`
- Status: `fail`
- Expected status: `fail`
- Counts: missing 0, warnings 0, failures 1
- Expected counts: missing 0, warnings 0, failures 1
- Start here available: true (expected true)
- Gate authority present: true (expected true)
- Required groups: `start_here, pr_review_story, repair_agent_handoff, policy_gates`
- Actual groups: `start_here, pr_review_story, repair_agent_handoff, policy_gates`
- Receipt JSON: `target/ripr/dogfood/report-packet-index/blocked_gate/target/ripr/reports/index.json`
- Receipt Markdown: `target/ripr/dogfood/report-packet-index/blocked_gate/target/ripr/reports/index.md`
- Expected report: `fixtures/boundary_gap/expected/report-packet-index/blocked-gate/index.json`
- Expected Markdown: `fixtures/boundary_gap/expected/report-packet-index/blocked-gate/index.md`
- Actual outputs: `target/ripr/dogfood/report-packet-index/blocked_gate`
- Reason: configured gate blocked while packet remains advisory: The index may report a blocked gate state but must point to gate-decision as pass/fail authority.
- Errors: none

### Report Packet Index `missing_assistant_proof`

- Rendered: true
- Packet input: `fixtures/boundary_gap/expected/report-packet-index/missing-assistant-proof/packet`
- Render command: `ripr reports index --root . --reports-dir target/ripr/reports --review-dir target/ripr/review --receipts-dir target/ripr/receipts --workflow-dir target/ripr/workflow --agent-dir target/ripr/agent --pilot-dir target/ripr/pilot --ci-dir target/ci --out target/ripr/reports/index.json --out-md target/ripr/reports/index.md`
- Status: `warn`
- Expected status: `warn`
- Counts: missing 1, warnings 1, failures 0
- Expected counts: missing 1, warnings 1, failures 0
- Start here available: true (expected true)
- Gate authority present: false (expected false)
- Required groups: `start_here, pr_review_story, repair_agent_handoff`
- Actual groups: `start_here, pr_review_story, repair_agent_handoff`
- Receipt JSON: `target/ripr/dogfood/report-packet-index/missing_assistant_proof/target/ripr/reports/index.json`
- Receipt Markdown: `target/ripr/dogfood/report-packet-index/missing_assistant_proof/target/ripr/reports/index.md`
- Expected report: `fixtures/boundary_gap/expected/report-packet-index/missing-assistant-proof/index.json`
- Expected Markdown: `fixtures/boundary_gap/expected/report-packet-index/missing-assistant-proof/index.md`
- Actual outputs: `target/ripr/dogfood/report-packet-index/missing_assistant_proof`
- Reason: PR guidance exists but assistant proof was not produced: Missing assistant proof should remain visible as a repair or handoff gap.
- Errors: none

### Report Packet Index `missing_receipts`

- Rendered: true
- Packet input: `fixtures/boundary_gap/expected/report-packet-index/missing-receipts/packet`
- Render command: `ripr reports index --root . --reports-dir target/ripr/reports --review-dir target/ripr/review --receipts-dir target/ripr/receipts --workflow-dir target/ripr/workflow --agent-dir target/ripr/agent --pilot-dir target/ripr/pilot --ci-dir target/ci --out target/ripr/reports/index.json --out-md target/ripr/reports/index.md`
- Status: `warn`
- Expected status: `warn`
- Counts: missing 2, warnings 2, failures 0
- Expected counts: missing 2, warnings 2, failures 0
- Start here available: true (expected true)
- Gate authority present: false (expected false)
- Required groups: `start_here, pr_review_story, validation_receipts`
- Actual groups: `start_here, pr_review_story, repair_agent_handoff, validation_receipts`
- Receipt JSON: `target/ripr/dogfood/report-packet-index/missing_receipts/target/ripr/reports/index.json`
- Receipt Markdown: `target/ripr/dogfood/report-packet-index/missing_receipts/target/ripr/reports/index.md`
- Expected report: `fixtures/boundary_gap/expected/report-packet-index/missing-receipts/index.json`
- Expected Markdown: `fixtures/boundary_gap/expected/report-packet-index/missing-receipts/index.md`
- Actual outputs: `target/ripr/dogfood/report-packet-index/missing_receipts`
- Reason: review story exists but validation receipts are absent: Missing receipts should route users to validation commands without changing pass/fail authority.
- Errors: none

### Report Packet Index `coverage_grip_present`

- Rendered: true
- Packet input: `fixtures/boundary_gap/expected/report-packet-index/coverage-grip-present/packet`
- Render command: `ripr reports index --root . --reports-dir target/ripr/reports --review-dir target/ripr/review --receipts-dir target/ripr/receipts --workflow-dir target/ripr/workflow --agent-dir target/ripr/agent --pilot-dir target/ripr/pilot --ci-dir target/ci --out target/ripr/reports/index.json --out-md target/ripr/reports/index.md`
- Status: `warn`
- Expected status: `warn`
- Counts: missing 0, warnings 0, failures 0
- Expected counts: missing 0, warnings 0, failures 0
- Start here available: true (expected true)
- Gate authority present: false (expected false)
- Required groups: `start_here, pr_review_story, calibration`
- Actual groups: `start_here, pr_review_story, repair_agent_handoff, calibration, validation_receipts`
- Receipt JSON: `target/ripr/dogfood/report-packet-index/coverage_grip_present/target/ripr/reports/index.json`
- Receipt Markdown: `target/ripr/dogfood/report-packet-index/coverage_grip_present/target/ripr/reports/index.md`
- Expected report: `fixtures/boundary_gap/expected/report-packet-index/coverage-grip-present/index.json`
- Expected Markdown: `fixtures/boundary_gap/expected/report-packet-index/coverage-grip-present/index.md`
- Actual outputs: `target/ripr/dogfood/report-packet-index/coverage_grip_present`
- Reason: coverage/grip frontier is present and grouped as calibration context: Coverage/grip context should be easy to find but must not be presented as runtime confirmation.
- Errors: none

## Generated CI Cockpit Receipts

These receipts validate the generated GitHub workflow cockpit surface. They check that the job summary starts with reviewer-first guidance, missing cockpit surfaces name regeneration commands, uploaded artifacts include the review packet, gate authority stays separate, and generated CI remains advisory by default.

- Default CI blocking: no
- Default inline comments: off
- Language grouping: checked for configured preview-language adapters; Rust-only config stays unchanged

| Case | Start here | Repair commands | Gate boundary | Advisory default | Artifact upload | Language grouping |
| --- | --- | ---: | --- | --- | --- | --- |
| `generated-pr-ci-review-workflow` | yes | 4/4 | yes | yes | yes | `checked` |

### Generated CI `generated-pr-ci-review-workflow`

- Command: `cargo run --quiet -p ripr -- init --ci github --dry-run && cargo run --quiet -p ripr -- reports ci-summary --root <generated-ci-cockpit-fixture> --base-ref main`
- Duration: 293 ms
- Start here: true
- Regeneration commands: 4 of 4
- Gate authority boundary: true
- Advisory default: true
- Artifact upload: true
- Language grouping: `checked`
- Errors: none

## Language Preview Receipts

These receipts run checked TypeScript and Python preview fixtures through `ripr check --mode fast`. They prove preview labels, structured static limits, disabled-language behavior, and no cross-language related-test routing without changing analyzer truth, editor routing, CI blocking, provider calls, source edits, generated tests, or mutation execution.

- Default CI blocking: no
- Preview adapters: opt-in through `[languages]`
- Receipt outputs: `target/ripr/dogfood/language-preview/<case>/check.json` and `human.txt`

| Case | Language | Enabled | Findings | Preview | Missing preview status | Related tests | Classes | Static limits |
| --- | --- | --- | ---: | ---: | ---: | ---: | --- | --- |
| `typescript_mocked_module_limit` | `typescript` | yes | 1 | 1 | 0 | 1 | `weakly_exposed` | `mocked_module` |
| `javascript_js_preview` | `javascript` | yes | 1 | 1 | 0 | 1 | `weakly_exposed` | `` |
| `python_missing_import_graph_limit` | `python` | yes | 1 | 1 | 0 | 1 | `static_unknown` | `missing_import_graph` |
| `python_mixed_language_no_cross_route` | `python` | yes | 1 | 1 | 0 | 0 | `no_static_path` | `` |
| `python_disabled` | `python` | no | 0 | 0 | 0 | 0 | `` | `` |

### Language Preview `typescript_mocked_module_limit`

- Language: `typescript`
- Preview enabled: true
- Root: `fixtures/typescript_mocked_module_limit/input`
- Diff: `fixtures/typescript_mocked_module_limit/diff.patch`
- Actual outputs: `target/ripr/dogfood/language-preview/typescript_mocked_module_limit`
- JSON receipt: `target/ripr/dogfood/language-preview/typescript_mocked_module_limit/check.json`
- Human receipt: `target/ripr/dogfood/language-preview/typescript_mocked_module_limit/human.txt`
- Duration: 23 ms
- Findings: 1 (expected 1)
- Preview findings: 1 (expected 1)
- Missing preview status: 0 (expected 0)
- Related tests: 1 (expected 1)
- Classifications: `weakly_exposed` (expected `weakly_exposed`)
- Static limits: `mocked_module` (expected `mocked_module`)
- Reason: TypeScript preview finding keeps preview metadata and mocked-module static limit.
- Errors: none

### Language Preview `javascript_js_preview`

- Language: `javascript`
- Preview enabled: true
- Root: `fixtures/javascript_js_preview/input`
- Diff: `fixtures/javascript_js_preview/diff.patch`
- Actual outputs: `target/ripr/dogfood/language-preview/javascript_js_preview`
- JSON receipt: `target/ripr/dogfood/language-preview/javascript_js_preview/check.json`
- Human receipt: `target/ripr/dogfood/language-preview/javascript_js_preview/human.txt`
- Duration: 20 ms
- Findings: 1 (expected 1)
- Preview findings: 1 (expected 1)
- Missing preview status: 0 (expected 0)
- Related tests: 1 (expected 1)
- Classifications: `weakly_exposed` (expected `weakly_exposed`)
- Static limits: `` (expected ``)
- Reason: JavaScript preview finding keeps separate JavaScript preview metadata through the TypeScript-family adapter.
- Errors: none

### Language Preview `python_missing_import_graph_limit`

- Language: `python`
- Preview enabled: true
- Root: `fixtures/python_missing_import_graph_limit/input`
- Diff: `fixtures/python_missing_import_graph_limit/diff.patch`
- Actual outputs: `target/ripr/dogfood/language-preview/python_missing_import_graph_limit`
- JSON receipt: `target/ripr/dogfood/language-preview/python_missing_import_graph_limit/check.json`
- Human receipt: `target/ripr/dogfood/language-preview/python_missing_import_graph_limit/human.txt`
- Duration: 15 ms
- Findings: 1 (expected 1)
- Preview findings: 1 (expected 1)
- Missing preview status: 0 (expected 0)
- Related tests: 1 (expected 1)
- Classifications: `static_unknown` (expected `static_unknown`)
- Static limits: `missing_import_graph` (expected `missing_import_graph`)
- Reason: Python preview finding keeps preview metadata and missing-import-graph static limit.
- Errors: none

### Language Preview `python_mixed_language_no_cross_route`

- Language: `python`
- Preview enabled: true
- Root: `fixtures/python_mixed_language_no_cross_route/input`
- Diff: `fixtures/python_mixed_language_no_cross_route/diff.patch`
- Actual outputs: `target/ripr/dogfood/language-preview/python_mixed_language_no_cross_route`
- JSON receipt: `target/ripr/dogfood/language-preview/python_mixed_language_no_cross_route/check.json`
- Human receipt: `target/ripr/dogfood/language-preview/python_mixed_language_no_cross_route/human.txt`
- Duration: 18 ms
- Findings: 1 (expected 1)
- Preview findings: 1 (expected 1)
- Missing preview status: 0 (expected 0)
- Related tests: 0 (expected 0)
- Classifications: `no_static_path` (expected `no_static_path`)
- Static limits: `` (expected ``)
- Reason: Mixed-language fixture must not use a TypeScript test as Python related-test evidence.
- Errors: none

### Language Preview `python_disabled`

- Language: `python`
- Preview enabled: false
- Root: `fixtures/python_disabled/input`
- Diff: `fixtures/python_disabled/diff.patch`
- Actual outputs: `target/ripr/dogfood/language-preview/python_disabled`
- JSON receipt: `target/ripr/dogfood/language-preview/python_disabled/check.json`
- Human receipt: `target/ripr/dogfood/language-preview/python_disabled/human.txt`
- Duration: 11 ms
- Findings: 0 (expected 0)
- Preview findings: 0 (expected 0)
- Missing preview status: 0 (expected 0)
- Related tests: 0 (expected 0)
- Classifications: `` (expected ``)
- Static limits: `` (expected ``)
- Reason: Rust-default language config must not emit disabled Python preview findings.
- Errors: none

## TypeScript Preview Repair-Loop Receipts

These receipts pin TypeScript-family preview repair-loop evidence against checked fixture outputs. They record useful advisory routes, weak-oracle downgrades, static limitations, skipped incomplete-packet cases, and checked complete-packet receipts without claiming TypeScript parity or support-tier promotion.

- Default CI blocking: no
- Preview authority: advisory
- Repair packets: advisory only when `repair_packet_ready` is true
- Receipt input: `fixtures/typescript-preview-repair-loop/corpus.json`
- Cases: 9; TypeScript: 7; JavaScript: 2; static limitations: 1; weak-oracle downgrades: 4; skipped: 1; packet-ready: 1

| Case | Language | Owner | Probe | Oracle | Gap state | Actionability | Outcome | Packet ready |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `typescript_boundary_predicate_proof` | `typescript` | `applyDiscount` | `predicate` | `unknown`/`unknown` | `advisory` | `incomplete_repair_packet` | `proof_improved` | no |
| `typescript_smoke_return_weak_oracle` | `typescript` | `isReady` | `predicate` | `smoke_only`/`smoke` | `advisory` | `incomplete_repair_packet` | `weak_oracle_downgraded` | no |
| `typescript_snapshot_return_weak_oracle` | `typescript` | `renderSummary` | `return_value` | `snapshot`/`medium` | `advisory` | `incomplete_repair_packet` | `weak_oracle_downgraded` | no |
| `typescript_async_rejection_broad_error` | `typescript` | `loadProfile` | `error_path` | `broad_error`/`weak` | `advisory` | `incomplete_repair_packet` | `weak_oracle_downgraded` | no |
| `javascript_mock_interaction_preview` | `javascript` | `notify` | `side_effect` | `smoke_only`/`smoke` | `advisory` | `incomplete_repair_packet` | `intentionally_skipped` | no |
| `typescript_static_class_method_smoke_oracle` | `typescript` | `build` | `return_value` | `smoke_only`/`smoke` | `advisory` | `incomplete_repair_packet` | `weak_oracle_downgraded` | no |
| `typescript_mocked_module_static_limit` | `typescript` | `applyDiscount` | `predicate` | `exact_value`/`strong` | `static_limitation` | `mocked_module` | `static_limitation_recorded` | no |
| `javascript_already_observed_unchanged` | `javascript` | `applyDiscount` | `predicate` | `exact_value`/`strong` | `advisory` | `incomplete_repair_packet` | `unanchored_relation_holds_advisory` | no |
| `typescript_complete_boundary_packet_closed` | `typescript` | `applyDiscount` | `predicate` | `relational_check`/`weak` | `actionable` | `complete_repair_packet` | `resolved` | yes |

### TypeScript Preview Repair-Loop `typescript_boundary_predicate_proof`

- Source fixture: `fixtures/typescript_boundary_gap` / `probe:src_discount.ts:typescript_preview:2396aec1`
- Language: `typescript` preview
- Classification: `weakly_exposed`
- Changed owner: `applyDiscount`
- Probe/oracle: `predicate` / `unknown` `unknown`
- Gap/actionability: `advisory` / `incomplete_repair_packet`
- Static limit: `none`
- Repair packet ready: false
- Expected observer shape: `exact equality-boundary assertion for amount == threshold`
- Verify route: `npm test -- discount` (not_run)
- Receipt route: `ripr agent receipt --source fixtures/typescript_boundary_gap --out target/ripr/reports/typescript-boundary-preview-receipt.json` (fixture_receipt_recorded)
- Why not actionable: TypeScript preview has owner, related-test, oracle, and probe evidence but lacks a complete repair packet contract
- Repair route: project canonical TypeScript repair packet fields only after verify, receipt, evidence refs, and edit boundaries are available
- Operator note: This records the useful first-use route: owner and related tests are known, the missing boundary discriminator is named, and the receipt still denies complete repair-packet authority.
- Must not change: `production behavior, public API, snapshot format, preview support tier, default gate or badge policy`
- Non-claims: `no provider calls, no source edits, no generated tests, no runtime Jest/Vitest execution, no mutation execution, no default gates, no public badge contribution, no baseline authority, no RIPR Zero authority, no support-tier promotion`
- Reason: A boundary predicate advisory is the primary TypeScript preview repair-loop signal, but it is not yet a complete repair packet.
- Receipt validation: pass

### TypeScript Preview Repair-Loop `typescript_smoke_return_weak_oracle`

- Source fixture: `fixtures/typescript_jest_vitest_assertion_facts` / `probe:src_signals.ts:typescript_preview:54d687f5`
- Language: `typescript` preview
- Classification: `weakly_exposed`
- Changed owner: `isReady`
- Probe/oracle: `predicate` / `smoke_only` `smoke`
- Gap/actionability: `advisory` / `incomplete_repair_packet`
- Static limit: `none`
- Repair packet ready: false
- Expected observer shape: `exact assertion on isReady at the count == 1 boundary instead of toBeTruthy smoke evidence`
- Verify route: `npm test -- signals` (not_run)
- Receipt route: `ripr agent receipt --source fixtures/typescript_jest_vitest_assertion_facts --out target/ripr/reports/typescript-smoke-return-preview-receipt.json` (not_run_preview_route)
- Why not actionable: TypeScript preview has owner, related-test, oracle, and probe evidence but lacks a complete repair packet contract
- Repair route: project canonical TypeScript repair packet fields only after verify, receipt, evidence refs, and edit boundaries are available
- Operator note: Smoke-only evidence on a returned comparison stays weak and records the boundary observer shape without becoming actionability proof.
- Must not change: `production behavior, public API, snapshot format, preview support tier, default gate or badge policy`
- Non-claims: `no provider calls, no source edits, no generated tests, no runtime Jest/Vitest execution, no mutation execution, no default gates, no public badge contribution, no baseline authority, no RIPR Zero authority, no support-tier promotion`
- Reason: The repair-loop receipt proves smoke assertions are downgraded rather than promoted into strong TypeScript proof.
- Receipt validation: pass

### TypeScript Preview Repair-Loop `typescript_snapshot_return_weak_oracle`

- Source fixture: `fixtures/typescript_jest_vitest_assertion_facts` / `probe:src_signals.ts:typescript_preview:8ca5702a`
- Language: `typescript` preview
- Classification: `weakly_exposed`
- Changed owner: `renderSummary`
- Probe/oracle: `return_value` / `snapshot` `medium`
- Gap/actionability: `advisory` / `incomplete_repair_packet`
- Static limit: `none`
- Repair packet ready: false
- Expected observer shape: `exact return-value assertion for return value == `summary:${status.trim()}` without relying only on a snapshot`
- Verify route: `npm test -- signals` (not_run)
- Receipt route: `ripr agent receipt --source fixtures/typescript_jest_vitest_assertion_facts --out target/ripr/reports/typescript-snapshot-return-preview-receipt.json` (not_run_preview_route)
- Why not actionable: TypeScript preview has owner, related-test, oracle, and probe evidence but lacks a complete repair packet contract
- Repair route: project canonical TypeScript repair packet fields only after verify, receipt, evidence refs, and edit boundaries are available
- Operator note: Snapshot evidence remains useful context but is not treated as a high-confidence repair packet.
- Must not change: `production behavior, public API, snapshot format, preview support tier, default gate or badge policy`
- Non-claims: `no provider calls, no source edits, no generated tests, no runtime Jest/Vitest execution, no mutation execution, no default gates, no public badge contribution, no baseline authority, no RIPR Zero authority, no support-tier promotion`
- Reason: Snapshot-only TypeScript evidence must remain advisory until a precise observer shape and packet contract are available.
- Receipt validation: pass

### TypeScript Preview Repair-Loop `typescript_async_rejection_broad_error`

- Source fixture: `fixtures/typescript_awaited_promise_reject` / `probe:src_profile.ts:typescript_preview:9da98202`
- Language: `typescript` preview
- Classification: `weakly_exposed`
- Changed owner: `loadProfile`
- Probe/oracle: `error_path` / `broad_error` `weak`
- Gap/actionability: `advisory` / `incomplete_repair_packet`
- Static limit: `none`
- Repair packet ready: false
- Expected observer shape: `rejection assertion that checks Error payload matching missing id`
- Verify route: `npm test -- profile` (not_run)
- Receipt route: `ripr agent receipt --source fixtures/typescript_awaited_promise_reject --out target/ripr/reports/typescript-async-error-preview-receipt.json` (not_run_preview_route)
- Why not actionable: TypeScript preview has owner, related-test, oracle, and probe evidence but lacks a complete repair packet contract
- Repair route: project canonical TypeScript repair packet fields only after verify, receipt, evidence refs, and edit boundaries are available
- Operator note: Broad rejection evidence records the error-path observer shape but does not become exact error proof.
- Must not change: `production behavior, public API, snapshot format, preview support tier, default gate or badge policy`
- Non-claims: `no provider calls, no source edits, no generated tests, no runtime Jest/Vitest execution, no mutation execution, no default gates, no public badge contribution, no baseline authority, no RIPR Zero authority, no support-tier promotion`
- Reason: Async rejection checks are useful preview evidence, but broad error assertions stay weak until payload and packet fields are complete.
- Receipt validation: pass

### TypeScript Preview Repair-Loop `javascript_mock_interaction_preview`

- Source fixture: `fixtures/typescript_probe_facts` / `probe:src_notify.js:typescript_preview:2f0e6fff`
- Language: `javascript` preview
- Classification: `weakly_exposed`
- Changed owner: `notify`
- Probe/oracle: `side_effect` / `smoke_only` `smoke`
- Gap/actionability: `advisory` / `incomplete_repair_packet`
- Static limit: `none`
- Repair packet ready: false
- Expected observer shape: `mock interaction assertion that mockSend is called with payload`
- Verify route: `npm test -- notify` (not_run)
- Receipt route: `ripr agent receipt --source fixtures/typescript_probe_facts --out target/ripr/reports/javascript-mock-interaction-preview-receipt.json` (not_run_preview_route)
- Why not actionable: TypeScript preview has owner, related-test, oracle, and probe evidence but lacks a complete repair packet contract
- Repair route: project canonical TypeScript repair packet fields only after verify, receipt, evidence refs, and edit boundaries are available
- Operator note: JavaScript stays separately labeled while sharing the TypeScript-family preview adapter; no mock repair packet is issued.
- Must not change: `production behavior, public API, snapshot format, preview support tier, default gate or badge policy`
- Non-claims: `no provider calls, no source edits, no generated tests, no runtime Jest/Vitest execution, no mutation execution, no default gates, no public badge contribution, no baseline authority, no RIPR Zero authority, no support-tier promotion`
- Reason: The JS side-effect route is intentionally skipped because the current preview can name the missing observer but not a complete agent-safe packet.
- Receipt validation: pass

### TypeScript Preview Repair-Loop `typescript_static_class_method_smoke_oracle`

- Source fixture: `fixtures/typescript_owner_kinds` / `probe:src_owners.ts:typescript_preview:3416313c`
- Language: `typescript` preview
- Classification: `weakly_exposed`
- Changed owner: `build`
- Probe/oracle: `return_value` / `smoke_only` `smoke`
- Gap/actionability: `advisory` / `incomplete_repair_packet`
- Static limit: `none`
- Repair packet ready: false
- Expected observer shape: `exact return-value assertion for Cart.build() output instead of toBeDefined smoke evidence`
- Verify route: `npm test -- owners` (not_run)
- Receipt route: `ripr agent receipt --source fixtures/typescript_owner_kinds --out target/ripr/reports/typescript-static-class-method-preview-receipt.json` (not_run_preview_route)
- Why not actionable: TypeScript preview has owner, related-test, oracle, and probe evidence but lacks a complete repair packet contract
- Repair route: project canonical TypeScript repair packet fields only after verify, receipt, evidence refs, and edit boundaries are available
- Operator note: Direct static class-method evidence can relate the smoke test to the changed owner, but the preview card still withholds a repair packet until strict fields exist.
- Must not change: `production behavior, public API, snapshot format, preview support tier, default gate or badge policy`
- Non-claims: `no provider calls, no source edits, no generated tests, no runtime Jest/Vitest execution, no mutation execution, no default gates, no public badge contribution, no baseline authority, no RIPR Zero authority, no support-tier promotion`
- Reason: The class-method route proves newer bounded relation evidence is useful while smoke-only observation remains weak and non-actionable.
- Receipt validation: pass

### TypeScript Preview Repair-Loop `typescript_mocked_module_static_limit`

- Source fixture: `fixtures/typescript_mocked_module_limit` / `probe:src_discount.ts:typescript_preview:2396aec1`
- Language: `typescript` preview
- Classification: `weakly_exposed`
- Changed owner: `applyDiscount`
- Probe/oracle: `predicate` / `exact_value` `strong`
- Gap/actionability: `static_limitation` / `mocked_module`
- Static limit: `mocked_module`
- Repair packet ready: false
- Expected observer shape: `no repair observer until mocked module substitution is resolved`
- Verify route: `npm test -- discount` (not_applicable)
- Receipt route: `ripr agent receipt --source fixtures/typescript_mocked_module_limit --out target/ripr/reports/typescript-mocked-module-preview-receipt.json` (static_limitation_no_repair_packet)
- Why not actionable: static limit `mocked_module` prevents bounded TypeScript repair guidance
- Repair route: add mock-shape support or validate the real substitution under test before issuing a repair packet
- Operator note: Even strong exact-value test evidence stays non-actionable when the mocked module makes the substitution under test opaque.
- Must not change: `production behavior, public API, snapshot format, preview support tier, default gate or badge policy`
- Non-claims: `no provider calls, no source edits, no generated tests, no runtime Jest/Vitest execution, no mutation execution, no default gates, no public badge contribution, no baseline authority, no RIPR Zero authority, no support-tier promotion`
- Reason: The mocked-module case is the false-actionable guard: exact assertions do not override a named static limitation. The test bare-calls applyDiscount with no import anchor (#4103 shape 1), so exposure holds at weakly_exposed while the exact-value oracle classification (exact_value/strong) stays readable; oracle classification is independent of relation credit.
- Receipt validation: pass

### TypeScript Preview Repair-Loop `javascript_already_observed_unchanged`

- Source fixture: `fixtures/javascript_js_preview` / `probe:src_discount.js:typescript_preview:8152c8c0`
- Language: `javascript` preview
- Classification: `weakly_exposed`
- Changed owner: `applyDiscount`
- Probe/oracle: `predicate` / `exact_value` `strong`
- Gap/actionability: `advisory` / `incomplete_repair_packet`
- Static limit: `none`
- Repair packet ready: false
- Expected observer shape: `no new test repair; existing exact-value assertion remains advisory context`
- Verify route: `npm test -- discount` (not_run)
- Receipt route: `ripr agent receipt --source fixtures/javascript_js_preview --out target/ripr/reports/javascript-already-observed-preview-receipt.json` (unchanged_no_repair_packet)
- Why not actionable: TypeScript preview has owner, related-test, oracle, and probe evidence but lacks a complete repair packet contract
- Repair route: project canonical TypeScript repair packet fields only after verify, receipt, evidence refs, and edit boundaries are available
- Operator note: Unanchored JavaScript owner-call evidence keeps its exact-value oracle classification and stays non-actionable rather than being queued as repair work.
- Must not change: `production behavior, public API, snapshot format, preview support tier, default gate or badge policy`
- Non-claims: `no provider calls, no source edits, no generated tests, no runtime Jest/Vitest execution, no mutation execution, no default gates, no public badge contribution, no baseline authority, no RIPR Zero authority, no support-tier promotion`
- Reason: The unchanged case proves separately labeled JavaScript preview evidence stays useful without creating repair work. The test bare-calls applyDiscount with no import anchor (#4103 shape 1), so exposure credit is withheld at weakly_exposed and the gap is advisory with an incomplete packet contract, while the exact-value oracle classification (exact_value/strong) survives.
- Receipt validation: pass

### TypeScript Preview Repair-Loop `typescript_complete_boundary_packet_closed`

- Source fixture: `fixtures/ts_repair_packet_complete` / `probe:src_discount.ts:typescript_preview:2396aec1`
- Language: `typescript` preview
- Classification: `weakly_exposed`
- Changed owner: `applyDiscount`
- Probe/oracle: `predicate` / `relational_check` `weak`
- Gap/actionability: `actionable` / `complete_repair_packet`
- Static limit: `none`
- Repair packet ready: true
- Expected observer shape: `test-only exact boundary assertion for amount == threshold using expect(result).toBe(50)`
- Verify route: `npx --no-install jest tests/discount.test.ts` (pass)
- Receipt route: `ripr outcome --before fixtures/first_successful_pr/typescript-preview-gap/inputs/reports/before-check.json --after fixtures/first_successful_pr/typescript-preview-gap/inputs/reports/after-check.json --format json` (closed_advisory_outcome_receipt)
- Why not actionable: complete repair packet; package root, runner, owner, oracle target, verify, receipt, and edit cage all resolved; delegatable as advisory preview evidence
- Repair route: add a focused assertion for the missing discriminator `amount == threshold` in the related test, shaped like `expect(result).toBe(expected)`
- Operator note: This records the current complete TypeScript preview packet loop: the packet is delegatable as advisory evidence, the verify command is concrete, and the closed receipt proves static gap movement without support-tier promotion.
- Must not change: `src/discount.ts, production TypeScript files, public API, preview support tier, default gate or badge policy, public badge, baseline, or RIPR Zero authority, provider integration, mutation execution`
- Non-claims: `no provider calls, no source edits, no generated tests, no runtime Jest/Vitest execution, no mutation execution, no default gates, no public badge contribution, no baseline authority, no RIPR Zero authority, no support-tier promotion`
- Reason: The complete TypeScript preview packet is checked as advisory route evidence only: it has a concrete verify command and closed static outcome receipt, but it still does not create gate, badge, baseline, RIPR Zero, runtime, mutation, provider, or support-tier authority.
- Receipt validation: pass

## TypeScript False-Actionable Audit

This advisory audit is computed from the checked TypeScript-family preview false-actionable corpus. It measures whether rows that must remain non-actionable have accidentally become repair-packet-ready, actionable, or complete-packet-shaped. It does not rerun analysis, execute TypeScript tests, edit source, generate tests, call providers, run mutation testing, change gates, contribute badge/baseline/RIPR Zero authority, or promote support tiers.

- Default CI blocking: no
- Preview authority: advisory
- Audit input: `fixtures/typescript-preview-false-actionable-audit/corpus.json`
- Quality gate: `pass` - All checked TypeScript-family preview audit rows remain non-actionable
- False actionable: 0 / 15 checked rows
- Repair-packet-ready violations: 0; actionable gap-state violations: 0; complete-packet category violations: 0; preview-boundary violations: 0

| Case | Language | Disposition | Gap state | Actionability | False actionable |
| --- | --- | --- | --- | --- | --- |
| `mock_interaction_without_payload_proof` | `typescript` | `candidate_future_support` | `advisory` | `incomplete_repair_packet` | no |
| `broad_throw_or_rejects_without_payload` | `typescript` | `candidate_future_support` | `advisory` | `incomplete_repair_packet` | no |
| `snapshot_only_weak_oracle` | `typescript` | `safe_advisory` | `advisory` | `incomplete_repair_packet` | no |
| `smoke_only_truthiness` | `typescript` | `safe_advisory` | `advisory` | `incomplete_repair_packet` | no |
| `heuristic_related_test_link` | `typescript` | `must_remain_non_actionable` | `advisory` | `ambiguous_related_test` | no |
| `owner_name_in_test_title_only` | `typescript` | `must_remain_non_actionable` | `advisory` | `missing_context` | no |
| `method_receiver_ambiguity` | `typescript` | `candidate_future_support` | `advisory` | `incomplete_repair_packet` | no |
| `class_method_static_call_incomplete_packet` | `typescript` | `candidate_future_support` | `advisory` | `incomplete_repair_packet` | no |
| `module_initializer_ambiguity` | `typescript` | `candidate_future_support` | `advisory` | `missing_target_shape` | no |
| `oracle_helper_gated_named_limitation` | `typescript` | `candidate_future_support` | `advisory` | `incomplete_repair_packet` | no |
| `table_case_named_limitation` | `typescript` | `candidate_future_support` | `already_observed` | `strong_oracle_observed` | no |
| `mocked_module_limit` | `typescript` | `named_static_limitation` | `static_limitation` | `mocked_module` | no |
| `decorator_indirection_limit` | `typescript` | `named_static_limitation` | `static_limitation` | `decorator_indirection` | no |
| `dynamic_dispatch_limit` | `typescript` | `named_static_limitation` | `static_limitation` | `dynamic_dispatch` | no |
| `exposed_with_dynamic_assertion_named_limitation` | `typescript` | `candidate_future_support` | `advisory` | `incomplete_repair_packet` | no |

## Bun UB Cross-Language Witness Receipts

These receipts pin the calibrated Bun Blob stable-byte review loop against existing TypeScript/Bun preview route evidence. They record whether the #31648-style seam is TypeScript-discriminated, missing a TypeScript discriminator, or token-only, while staying preview/advisory and packet-ineligible.

- Default CI blocking: no
- Preview authority: advisory
- Runtime execution: none
- Repair packets: none
- Receipt input: `fixtures/bun-ub-cross-language-dogfood/corpus.json`
- Cases: 9; TS-discriminated: 3; missing discriminator: 1; mention-only: 1; bridge-unknown: 1; named limitation: 2; packet-ready: 0

| Case | Source | Route quality | State | Action | Suggested file | Packet ready |
| --- | --- | --- | --- | --- | --- | --- |
| `bun_blob_31648_known_good` | `bun_blob_shared_and_resizable_present` | `bun_blob_complete_ts_discriminated_advisory` | `rust_ungripped_ts_discriminated` | `no_missing_bridge_discriminator` | `not_applicable` | no |
| `bun_array_buffer_copy_to_unshared_live_receipt` | `bun_array_buffer_copy_to_unshared_configured_bridge_advisory` | `bun_array_buffer_copy_to_unshared_configured_bridge_advisory` | `rust_ungripped_ts_discriminated` | `no_missing_bridge_discriminator` | `not_applicable` | no |
| `bun_markdown_resizable_array_buffer_live_receipt` | `bun_markdown_resizable_array_buffer_configured_bridge_advisory` | `bun_markdown_resizable_array_buffer_configured_bridge_advisory` | `rust_ungripped_ts_discriminated` | `no_missing_bridge_discriminator` | `not_applicable` | no |
| `bun_blob_stripped_resizable` | `bun_blob_resizable_missing` | `bun_blob_missing_resizable_oracle_limitation` | `rust_ungripped_ts_missing_discriminator` | `suggest_resizable_array_buffer_blob_case` | `test/js/web/fetch/blob.test.ts` | no |
| `bun_blob_mention_only` | `bun_blob_max_byte_length_mention_not_observer` | `bun_blob_mention_not_observer_limitation` | `ts_mention_not_observer` | `reject_token_mention` | `not_applicable` | no |
| `bun_blob_bridge_unknown_live_receipt` | `bun_blob_bridge_unknown_without_hint` | `bun_blob_bridge_unknown_limitation` | `bridge_unknown` | `inspect_or_add_bridge_evidence` | `not_applicable` | no |
| `bun_node_fs_scalar_write_manifest_only_receipt` | `bun_node_fs_scalar_write_manifest_only_profile` | `bun_node_fs_scalar_write_manifest_only_profile` | `named_static_limitation` | `inspect_node_fs_bridge_oracle` | `test/js/node/fs/fs.test.ts` | no |
| `bun_write_helper_gated_manifest_only_receipt` | `bun_write_helper_gated_manifest_only_profile` | `bun_write_helper_gated_manifest_only_profile` | `named_static_limitation` | `wait_for_bun_write_helper_then_inspect_bridge` | `not_applicable` | no |
| `bun_ffi_negative_offset_panic_boundary` | `bun_ffi_negative_offset_panic_boundary_limitation` | `bun_ffi_negative_offset_panic_boundary_limitation` | `public_reachable_panic_boundary_unrevealed` | `keep_panic_boundary_limitation` | `not_applicable` | no |

### Bun UB Cross-Language `bun_blob_31648_known_good`

- Rust seam: `src/jsc/Blob.rs` / `Blob::from_js_without_defer_gc` / `array_buffer.shared \|\| array_buffer.resizable`
- TypeScript test file: `test/js/web/fetch/blob.test.ts`
- Expected/observed state: `rust_ungripped_ts_discriminated` / `rust_ungripped_ts_discriminated`
- Missing discriminators: ``
- Missing graph legs: ``
- Suggested test file: `not_applicable`
- Manual verdict: `agrees`
- Operator action: `no_missing_bridge_discriminator`
- Review before: Rust-only review would manually grep test/js for SharedArrayBuffer, maxByteLength, Blob inputs, and stable-byte assertions.
- Review after: TypeScript/Bun preview credits the configured Blob route as discriminated and keeps the result advisory.
- Bridge verdict: `configured_binding_edge_credited`
- Placement verdict: `not_needed`
- Proof mode: `advisory_cross_language_preview`
- Receipt state: `closed`
- Authority boundary: `preview_advisory_only`
- Non-claims: `no source edits, no generated tests, no runtime Bun execution, no mutation execution, no default gates, no public badge contribution, no baseline authority, no RIPR Zero authority, no support-tier promotion, no public repair packet, no full cross-language proof`
- Reason: The dogfood receipt records that the old manual grep step is replaced by a credited, configured TypeScript witness while keeping the authority boundary narrow.
- Receipt validation: pass

### Bun UB Cross-Language `bun_array_buffer_copy_to_unshared_live_receipt`

- Rust seam: `src/jsc/array_buffer.rs` / `copy_to_unshared` / `SharedArrayBuffer and resizable ArrayBuffer copy semantics`
- TypeScript test file: `test/js/web/fetch/blob.test.ts`
- Expected/observed state: `rust_ungripped_ts_discriminated` / `rust_ungripped_ts_discriminated`
- Missing discriminators: ``
- Missing graph legs: ``
- Suggested test file: `not_applicable`
- Manual verdict: `agrees`
- Operator action: `no_missing_bridge_discriminator`
- Review before: Manual review had to connect the copy_to_unshared Rust seam back through Blob ArrayBuffer tests before trusting the witness.
- Review after: TypeScript/Bun preview records the configured copy_to_unshared bridge, Blob callsite, and stable-byte oracle as an advisory witness.
- Bridge verdict: `configured_binding_edge_credited`
- Placement verdict: `not_needed`
- Proof mode: `advisory_cross_language_preview`
- Receipt state: `closed`
- Authority boundary: `preview_advisory_only`
- Non-claims: `no provider calls, no source edits, no generated tests, no runtime Bun execution, no mutation execution, no default gates, no public badge contribution, no baseline authority, no RIPR Zero authority, no support-tier promotion, no public repair packet, no full cross-language proof`
- Reason: The live-shaped copy_to_unshared receipt proves the router can credit a configured non-Blob Rust seam through the existing TypeScript Blob witness without suggesting a Rust test or public repair packet.
- Receipt validation: pass

### Bun UB Cross-Language `bun_markdown_resizable_array_buffer_live_receipt`

- Rust seam: `src/runtime/api/MarkdownObject.rs` / `MarkdownObject::to_string` / `self.0.resizable && !self.0.shared`
- TypeScript test file: `test/js/bun/md/md-edge-cases.test.ts`
- Expected/observed state: `rust_ungripped_ts_discriminated` / `rust_ungripped_ts_discriminated`
- Missing discriminators: ``
- Missing graph legs: ``
- Suggested test file: `not_applicable`
- Manual verdict: `agrees`
- Operator action: `no_missing_bridge_discriminator`
- Review before: Manual review had to connect the MarkdownObject Rust boundary to Bun.markdown TypeScript edge-case tests before avoiding duplicate Rust remediation.
- Review after: TypeScript/Bun preview records the configured MarkdownObject bridge, resizable ArrayBuffer discriminator, and markdown output oracle as an advisory witness.
- Bridge verdict: `configured_binding_edge_credited`
- Placement verdict: `not_needed`
- Proof mode: `advisory_cross_language_preview`
- Receipt state: `closed`
- Authority boundary: `preview_advisory_only`
- Non-claims: `no provider calls, no source edits, no generated tests, no runtime Bun execution, no mutation execution, no default gates, no public badge contribution, no baseline authority, no RIPR Zero authority, no support-tier promotion, no public repair packet, no full cross-language proof`
- Reason: The live-shaped MarkdownObject receipt proves the router can credit a different Bun TypeScript surface through a configured Rust owner while preserving advisory-only authority.
- Receipt validation: pass

### Bun UB Cross-Language `bun_blob_stripped_resizable`

- Rust seam: `src/jsc/Blob.rs` / `Blob::from_js_without_defer_gc` / `array_buffer.shared \|\| array_buffer.resizable`
- TypeScript test file: `test/js/web/fetch/blob.test.ts`
- Expected/observed state: `rust_ungripped_ts_missing_discriminator` / `rust_ungripped_ts_missing_discriminator`
- Missing discriminators: `resizable_array_buffer`
- Missing graph legs: `boundary_discriminator:resizable_array_buffer`
- Suggested test file: `test/js/web/fetch/blob.test.ts`
- Manual verdict: `agrees`
- Operator action: `suggest_resizable_array_buffer_blob_case`
- Review before: Rust-only review would report the seam ungripped without showing that the missing branch belongs in the TypeScript Blob observer route.
- Review after: TypeScript/Bun preview names the missing resizable ArrayBuffer discriminator and points at the existing Blob test file.
- Bridge verdict: `configured_binding_edge_credited`
- Placement verdict: `suggested_test_file:test/js/web/fetch/blob.test.ts`
- Proof mode: `missing_discriminator_advisory`
- Receipt state: `closed`
- Authority boundary: `preview_advisory_only`
- Non-claims: `no source edits, no generated tests, no runtime Bun execution, no mutation execution, no default gates, no public badge contribution, no baseline authority, no RIPR Zero authority, no support-tier promotion, no public repair packet, no full cross-language proof`
- Reason: The dogfood receipt proves the useful failure mode: the tool suggests the TypeScript Blob placement for the missing resizable branch instead of sending the operator to a Rust test.
- Receipt validation: pass

### Bun UB Cross-Language `bun_blob_mention_only`

- Rust seam: `src/jsc/Blob.rs` / `Blob::from_js_without_defer_gc` / `array_buffer.shared \|\| array_buffer.resizable`
- TypeScript test file: `test/js/web/fetch/blob.test.ts`
- Expected/observed state: `ts_mention_not_observer` / `ts_mention_not_observer`
- Missing discriminators: ``
- Missing graph legs: `external_callsite:view_backed_blob_input, external_oracle:stable_byte_copy`
- Suggested test file: `not_applicable`
- Manual verdict: `agrees`
- Operator action: `reject_token_mention`
- Review before: Manual review had to distinguish maxByteLength text from a real resizable ArrayBuffer Blob observer.
- Review after: TypeScript/Bun preview keeps the token mention as a limitation and does not credit it as bridge evidence.
- Bridge verdict: `configured_binding_edge_not_enough_for_token_mention`
- Placement verdict: `not_applicable`
- Proof mode: `mention_not_observer_advisory`
- Receipt state: `closed`
- Authority boundary: `preview_advisory_only`
- Non-claims: `no source edits, no generated tests, no runtime Bun execution, no mutation execution, no default gates, no public badge contribution, no baseline authority, no RIPR Zero authority, no support-tier promotion, no public repair packet, no full cross-language proof`
- Reason: The dogfood receipt pins the false-positive control: maxByteLength text is not an observer without Blob input and stable-byte assertion evidence.
- Receipt validation: pass

### Bun UB Cross-Language `bun_blob_bridge_unknown_live_receipt`

- Rust seam: `src/jsc/Blob.rs` / `Blob::from_js_without_defer_gc` / `array_buffer.shared \|\| array_buffer.resizable`
- TypeScript test file: `test/js/web/fetch/blob.test.ts`
- Expected/observed state: `bridge_unknown` / `bridge_unknown`
- Missing discriminators: ``
- Missing graph legs: `binding_or_ffi_edge`
- Suggested test file: `not_applicable`
- Manual verdict: `agrees`
- Operator action: `inspect_or_add_bridge_evidence`
- Review before: Manual review could see complete TypeScript Blob facts but still had to decide whether they could be credited to the Rust owner.
- Review after: TypeScript/Bun preview keeps the complete TypeScript witness as bridge_unknown until a binding or FFI edge is credited.
- Bridge verdict: `binding_or_ffi_edge_missing`
- Placement verdict: `not_applicable`
- Proof mode: `bridge_unknown`
- Receipt state: `closed`
- Authority boundary: `preview_advisory_only`
- Non-claims: `no provider calls, no source edits, no generated tests, no runtime Bun execution, no mutation execution, no default gates, no public badge contribution, no baseline authority, no RIPR Zero authority, no support-tier promotion, no public repair packet, no full cross-language proof`
- Reason: The bridge_unknown receipt records the safe stop condition: inspect bridge evidence before adding tests or crediting the TypeScript witness to the Rust seam.
- Receipt validation: pass

### Bun UB Cross-Language `bun_node_fs_scalar_write_manifest_only_receipt`

- Rust seam: `unresolved:node-fs-scalar-write-rust-seam` / `node:fs scalar write sink` / `JS-owned bytes must be copied before native write scalar sinks`
- TypeScript test file: `test/js/node/fs/fs.test.ts`
- Expected/observed state: `named_static_limitation` / `named_static_limitation`
- Missing discriminators: ``
- Missing graph legs: `binding_or_ffi_edge:node_fs_scalar_write, external_oracle:stable_byte_scalar_write`
- Suggested test file: `test/js/node/fs/fs.test.ts`
- Manual verdict: `agrees`
- Operator action: `inspect_node_fs_bridge_oracle`
- Review before: Manual review treated node:fs scalar write as a future stable-byte surface without a durable receipt for why it was not actionable yet.
- Review after: TypeScript/Bun preview records node:fs scalar write as a manifest-only named limitation with typed witness path, observable red/green proof mode, and missing bridge/oracle legs.
- Bridge verdict: `manifest_only_bridge_not_credited`
- Placement verdict: `typed_witness_path:test/js/node/fs/fs.test.ts;not_actionable`
- Proof mode: `observable_red_green`
- Receipt state: `closed`
- Authority boundary: `preview_advisory_only`
- Non-claims: `no provider calls, no source edits, no generated tests, no runtime Bun execution, no mutation execution, no default gates, no public badge contribution, no baseline authority, no RIPR Zero authority, no support-tier promotion, no public repair packet, no full cross-language proof`
- Reason: The node:fs receipt proves that manifest-only profile intake is visible to operators without becoming a test-edit packet or wrong-language placement recommendation.
- Receipt validation: pass

### Bun UB Cross-Language `bun_write_helper_gated_manifest_only_receipt`

- Rust seam: `unresolved:bun-write-stable-byte-rust-seam` / `Bun.write stable-byte sink` / `JS-owned bytes must not cross Bun.write native sinks without a helper`
- TypeScript test file: `test/js/bun/write.test.ts`
- Expected/observed state: `named_static_limitation` / `named_static_limitation`
- Missing discriminators: ``
- Missing graph legs: `binding_or_ffi_edge:bun_write_sink, helper:bun_write_fixture_helper, external_oracle:stable_byte_write`
- Suggested test file: `not_applicable`
- Manual verdict: `agrees`
- Operator action: `wait_for_bun_write_helper_then_inspect_bridge`
- Review before: Manual review could lose the Bun.write stable-byte route because the helper dependency made it look like an ordinary absent test case.
- Review after: TypeScript/Bun preview records Bun.write as helper-gated and manifest-only, with no placement until the helper, bridge, and oracle evidence are credited.
- Bridge verdict: `helper_gated_manifest_only_bridge_not_credited`
- Placement verdict: `not_applicable`
- Proof mode: `helper_gated`
- Receipt state: `closed`
- Authority boundary: `preview_advisory_only`
- Non-claims: `no provider calls, no source edits, no generated tests, no runtime Bun execution, no mutation execution, no default gates, no public badge contribution, no baseline authority, no RIPR Zero authority, no support-tier promotion, no public repair packet, no full cross-language proof`
- Reason: The Bun.write receipt keeps helper-gated stable-byte work visible without suggesting a test file or repair packet before the helper and bridge evidence exist.
- Receipt validation: pass

### Bun UB Cross-Language `bun_ffi_negative_offset_panic_boundary`

- Rust seam: `src/bun.js/bindings/FFIObject.rs` / `FFIObject::read` / `usize::try_from(to_int32()).expect("int cast")`
- TypeScript test file: `unresolved:typescript-test-surface`
- Expected/observed state: `public_reachable_panic_boundary_unrevealed` / `public_reachable_panic_boundary_unrevealed`
- Missing discriminators: `negative_offset`
- Missing graph legs: `external_oracle:negative_offset_panic_boundary, safe_external_observer_target`
- Suggested test file: `not_applicable`
- Manual verdict: `agrees`
- Operator action: `keep_panic_boundary_limitation`
- Review before: Rust-only review could identify the public FFI read entrypoint but still had to reverse-map whether TypeScript observed the negative-offset panic boundary.
- Review after: The dogfood receipt records the #950 route as a named static limitation with unresolved oracle and safe observer target evidence.
- Bridge verdict: `ffi_binding_edge_heuristic`
- Placement verdict: `not_applicable`
- Proof mode: `panic_boundary_limitation_advisory`
- Receipt state: `closed`
- Authority boundary: `preview_advisory_only`
- Non-claims: `no source edits, no generated tests, no runtime Bun execution, no mutation execution, no default gates, no public badge contribution, no baseline authority, no RIPR Zero authority, no support-tier promotion, no public repair packet, no full cross-language proof, no verify command, no receipt command, no allowed edit surface`
- Reason: The #950 Bun FFI receipt proves the useful no-action outcome: public reachability plus a Rust panic boundary is still a limitation until a safe external negative-offset panic oracle and observer target are resolved.
- Receipt validation: pass

## Editor Gap Cockpit Receipts

These receipts validate checked `fixtures/editor_gap_cockpit` projections for the local repair cockpit. They verify actionable Rust repair routing, preview static-limit ordering, disabled-language no-diagnostic state, wrong-root and stale fail-closed behavior, and no-action refresh-only behavior without changing analyzer truth, source files, generated tests, provider calls, mutation execution, policy, gates, or PR comments.

- Default CI blocking: no
- Evidence source: committed declarations only; no LSP or VS Code producer runs in this family, and no test produces these fixtures (#4267).
- Editor behavior: saved-workspace and projection-only
- Receipt outputs: `fixtures/editor_gap_cockpit/<case>/expected/*`

| Case | State | Language | Diagnostics | Fail closed | Actions | Static limit |
| --- | --- | --- | ---: | --- | --- | --- |
| `rust_actionable` | `actionable` | `rust` | 1 | no | `copy_repair_packet, open_related_test, copy_verify_command, copy_receipt_command, refresh` | `` |
| `typescript_preview_static_limit` | `actionable` | `typescript` | 1 | no | `copy_repair_packet, copy_static_limit_note, refresh` | `mocked_module` |
| `python_preview_static_limit` | `actionable` | `python` | 1 | no | `copy_repair_packet, copy_static_limit_note, refresh` | `missing_import_graph` |
| `disabled_language` | `disabled_language` | `python` | 0 | yes | `refresh` | `` |
| `wrong_root` | `wrong_root` | `not_projected` | 0 | yes | `refresh` | `` |
| `stale_artifact` | `stale_artifact` | `not_projected` | 0 | yes | `refresh` | `` |
| `no_actionable_gap` | `no_actionable_gap` | `rust` | 0 | yes | `refresh` | `` |

### Editor Gap Cockpit `rust_actionable`

- State: `actionable`
- Expected state: `actionable`
- Language: `rust` (expected `rust`)
- Language status: `stable` (expected `stable`)
- Diagnostics: 1 projected, 1 actual, expected 1
- Fail closed: false (expected false)
- Actions: `copy_repair_packet, open_related_test, copy_verify_command, copy_receipt_command, refresh` (expected `copy_repair_packet, open_related_test, copy_verify_command, copy_receipt_command, refresh`; actual count 5)
- Static limit: `` (expected ``)
- Static limit before action: false
- Projection JSON: `fixtures/editor_gap_cockpit/rust_actionable/expected/gap-projection.json`
- Diagnostics JSON: `fixtures/editor_gap_cockpit/rust_actionable/expected/lsp-diagnostics.json`
- Hover: `fixtures/editor_gap_cockpit/rust_actionable/expected/lsp-hover.md`
- Code actions JSON: `fixtures/editor_gap_cockpit/rust_actionable/expected/lsp-code-actions.json`
- VS Code status JSON: `fixtures/editor_gap_cockpit/rust_actionable/expected/vscode-status.json`
- Expected directory: `fixtures/editor_gap_cockpit/rust_actionable/expected`
- Reason: Rust stable gap projects a related test, repair packet, verify command, and receipt command.
- Errors: none

### Editor Gap Cockpit `typescript_preview_static_limit`

- State: `actionable`
- Expected state: `actionable`
- Language: `typescript` (expected `typescript`)
- Language status: `preview` (expected `preview`)
- Diagnostics: 1 projected, 1 actual, expected 1
- Fail closed: false (expected false)
- Actions: `copy_repair_packet, copy_static_limit_note, refresh` (expected `copy_repair_packet, copy_static_limit_note, refresh`; actual count 3)
- Static limit: `mocked_module` (expected `mocked_module`)
- Static limit before action: true
- Projection JSON: `fixtures/editor_gap_cockpit/typescript_preview_static_limit/expected/gap-projection.json`
- Diagnostics JSON: `fixtures/editor_gap_cockpit/typescript_preview_static_limit/expected/lsp-diagnostics.json`
- Hover: `fixtures/editor_gap_cockpit/typescript_preview_static_limit/expected/lsp-hover.md`
- Code actions JSON: `fixtures/editor_gap_cockpit/typescript_preview_static_limit/expected/lsp-code-actions.json`
- VS Code status JSON: `fixtures/editor_gap_cockpit/typescript_preview_static_limit/expected/vscode-status.json`
- Expected directory: `fixtures/editor_gap_cockpit/typescript_preview_static_limit/expected`
- Reason: TypeScript preview gap keeps static-limit evidence visible before action language.
- Errors: none

### Editor Gap Cockpit `python_preview_static_limit`

- State: `actionable`
- Expected state: `actionable`
- Language: `python` (expected `python`)
- Language status: `preview` (expected `preview`)
- Diagnostics: 1 projected, 1 actual, expected 1
- Fail closed: false (expected false)
- Actions: `copy_repair_packet, copy_static_limit_note, refresh` (expected `copy_repair_packet, copy_static_limit_note, refresh`; actual count 3)
- Static limit: `missing_import_graph` (expected `missing_import_graph`)
- Static limit before action: true
- Projection JSON: `fixtures/editor_gap_cockpit/python_preview_static_limit/expected/gap-projection.json`
- Diagnostics JSON: `fixtures/editor_gap_cockpit/python_preview_static_limit/expected/lsp-diagnostics.json`
- Hover: `fixtures/editor_gap_cockpit/python_preview_static_limit/expected/lsp-hover.md`
- Code actions JSON: `fixtures/editor_gap_cockpit/python_preview_static_limit/expected/lsp-code-actions.json`
- VS Code status JSON: `fixtures/editor_gap_cockpit/python_preview_static_limit/expected/vscode-status.json`
- Expected directory: `fixtures/editor_gap_cockpit/python_preview_static_limit/expected`
- Reason: Python preview gap keeps missing-import-graph limits visible before action language.
- Errors: none

### Editor Gap Cockpit `disabled_language`

- State: `disabled_language`
- Expected state: `disabled_language`
- Language: `python` (expected `python`)
- Language status: `preview` (expected `preview`)
- Diagnostics: 0 projected, 0 actual, expected 0
- Fail closed: true (expected true)
- Actions: `refresh` (expected `refresh`; actual count 1)
- Static limit: `` (expected ``)
- Static limit before action: false
- Projection JSON: `fixtures/editor_gap_cockpit/disabled_language/expected/gap-projection.json`
- Diagnostics JSON: `fixtures/editor_gap_cockpit/disabled_language/expected/lsp-diagnostics.json`
- Hover: `fixtures/editor_gap_cockpit/disabled_language/expected/lsp-hover.md`
- Code actions JSON: `fixtures/editor_gap_cockpit/disabled_language/expected/lsp-code-actions.json`
- VS Code status JSON: `fixtures/editor_gap_cockpit/disabled_language/expected/vscode-status.json`
- Expected directory: `fixtures/editor_gap_cockpit/disabled_language/expected`
- Reason: Disabled preview language produces status and refresh only, not diagnostics or repair packets.
- Errors: none

### Editor Gap Cockpit `wrong_root`

- State: `wrong_root`
- Expected state: `wrong_root`
- Language: `not_projected` (expected `not_projected`)
- Language status: `not_projected` (expected `not_projected`)
- Diagnostics: 0 projected, 0 actual, expected 0
- Fail closed: true (expected true)
- Actions: `refresh` (expected `refresh`; actual count 1)
- Static limit: `` (expected ``)
- Static limit before action: false
- Projection JSON: `fixtures/editor_gap_cockpit/wrong_root/expected/gap-projection.json`
- Diagnostics JSON: `fixtures/editor_gap_cockpit/wrong_root/expected/lsp-diagnostics.json`
- Hover: `fixtures/editor_gap_cockpit/wrong_root/expected/lsp-hover.md`
- Code actions JSON: `fixtures/editor_gap_cockpit/wrong_root/expected/lsp-code-actions.json`
- VS Code status JSON: `fixtures/editor_gap_cockpit/wrong_root/expected/vscode-status.json`
- Expected directory: `fixtures/editor_gap_cockpit/wrong_root/expected`
- Reason: Wrong-root artifacts fail closed and leave refresh as the only safe action.
- Errors: none

### Editor Gap Cockpit `stale_artifact`

- State: `stale_artifact`
- Expected state: `stale_artifact`
- Language: `not_projected` (expected `not_projected`)
- Language status: `not_projected` (expected `not_projected`)
- Diagnostics: 0 projected, 0 actual, expected 0
- Fail closed: true (expected true)
- Actions: `refresh` (expected `refresh`; actual count 1)
- Static limit: `` (expected ``)
- Static limit before action: false
- Projection JSON: `fixtures/editor_gap_cockpit/stale_artifact/expected/gap-projection.json`
- Diagnostics JSON: `fixtures/editor_gap_cockpit/stale_artifact/expected/lsp-diagnostics.json`
- Hover: `fixtures/editor_gap_cockpit/stale_artifact/expected/lsp-hover.md`
- Code actions JSON: `fixtures/editor_gap_cockpit/stale_artifact/expected/lsp-code-actions.json`
- VS Code status JSON: `fixtures/editor_gap_cockpit/stale_artifact/expected/vscode-status.json`
- Expected directory: `fixtures/editor_gap_cockpit/stale_artifact/expected`
- Reason: Stale artifacts suppress diagnostics and repair actions until refreshed.
- Errors: none

### Editor Gap Cockpit `no_actionable_gap`

- State: `no_actionable_gap`
- Expected state: `no_actionable_gap`
- Language: `rust` (expected `rust`)
- Language status: `stable` (expected `stable`)
- Diagnostics: 0 projected, 0 actual, expected 0
- Fail closed: true (expected true)
- Actions: `refresh` (expected `refresh`; actual count 1)
- Static limit: `` (expected ``)
- Static limit before action: false
- Projection JSON: `fixtures/editor_gap_cockpit/no_actionable_gap/expected/gap-projection.json`
- Diagnostics JSON: `fixtures/editor_gap_cockpit/no_actionable_gap/expected/lsp-diagnostics.json`
- Hover: `fixtures/editor_gap_cockpit/no_actionable_gap/expected/lsp-hover.md`
- Code actions JSON: `fixtures/editor_gap_cockpit/no_actionable_gap/expected/lsp-code-actions.json`
- VS Code status JSON: `fixtures/editor_gap_cockpit/no_actionable_gap/expected/vscode-status.json`
- Expected directory: `fixtures/editor_gap_cockpit/no_actionable_gap/expected`
- Reason: No-action state explains that no local repair packet should be projected.
- Errors: none

## Editor First-PR Bridge Receipts

These receipts validate checked `fixtures/editor_first_pr_bridge` projections for the local repair -> first-pr handoff. They verify packet-missing, repairable, no-action, stale, wrong-root, malformed, receipt-improved, and receipt-unchanged states without creating first-pr packets, publishing PR comments, composing generated CI summaries, editing source, generating tests, calling providers, running mutation, or deciding gates.

- Default CI blocking: no
- Editor behavior: saved-workspace read-only projection over existing first-pr artifacts
- Receipt outputs: `fixtures/editor_first_pr_bridge/<case>/expected/*`

| Case | Packet state | Diagnostics | Fail closed | Safe actions | Suppressed actions | Receipt movement |
| --- | --- | ---: | --- | --- | --- | --- |
| `setup_ok` | `found` | 0 | no | `open_packet, copy_summary, copy_regeneration_guidance` | `copy_repair_packet, copy_verify_command, copy_receipt_command` | `` |
| `packet_missing` | `missing` | 0 | yes | `copy_regeneration_guidance` | `open_packet, copy_summary, copy_repair_packet, copy_verify_command, copy_receipt_command` | `` |
| `packet_found_repairable` | `top_repairable_gap` | 1 | no | `open_packet, copy_summary, copy_repair_packet, copy_verify_command, copy_receipt_command, copy_regeneration_guidance` | `` | `` |
| `packet_no_action` | `no_action` | 0 | no | `open_packet, copy_summary, copy_regeneration_guidance` | `copy_repair_packet, copy_verify_command, copy_receipt_command` | `` |
| `packet_stale` | `stale` | 0 | yes | `copy_regeneration_guidance` | `open_packet, copy_summary, copy_repair_packet, copy_verify_command, copy_receipt_command` | `` |
| `packet_wrong_root` | `wrong_root` | 0 | yes | `copy_regeneration_guidance` | `open_packet, copy_summary, copy_repair_packet, copy_verify_command, copy_receipt_command` | `` |
| `packet_malformed` | `malformed` | 0 | yes | `copy_regeneration_guidance` | `open_packet, copy_summary, copy_repair_packet, copy_verify_command, copy_receipt_command` | `` |
| `receipt_improved_packet_ready` | `top_repairable_gap` | 1 | no | `open_packet, copy_summary, copy_repair_packet, copy_verify_command, copy_receipt_command, copy_regeneration_guidance` | `` | `improved` |
| `receipt_unchanged_packet_ready` | `top_repairable_gap` | 1 | no | `open_packet, copy_summary, copy_repair_packet, copy_verify_command, copy_receipt_command, copy_regeneration_guidance` | `` | `unchanged` |

### Editor First-PR Bridge `setup_ok`

- Packet state: `found` (expected `found`)
- Diagnostics: 0 (expected 0)
- Fail closed: false (expected false)
- Safe actions: `open_packet, copy_summary, copy_regeneration_guidance` (expected `open_packet, copy_summary, copy_regeneration_guidance`)
- Suppressed actions: `copy_repair_packet, copy_verify_command, copy_receipt_command` (expected `copy_repair_packet, copy_verify_command, copy_receipt_command`)
- First-pr action commands: `open_packet, copy_summary, copy_regeneration_guidance`
- All action commands: `ripr.openFirstPrPacket, ripr.copyFirstPrSummary, ripr.copyFirstPrRegenerationGuidance, ripr.refresh`
- Receipt movement: `` (expected ``)
- Non-claims: runtime=false, mutation=false, policy_gate=false, pr_ready=false
- First-pr status JSON: `fixtures/editor_first_pr_bridge/setup_ok/expected/first-pr-status.json`
- Diagnostics JSON: `fixtures/editor_first_pr_bridge/setup_ok/expected/lsp-diagnostics.json`
- Code actions JSON: `fixtures/editor_first_pr_bridge/setup_ok/expected/lsp-code-actions.json`
- VS Code status JSON: `fixtures/editor_first_pr_bridge/setup_ok/expected/vscode-status.json`
- Setup diagnosis: `fixtures/editor_first_pr_bridge/setup_ok/expected/setup-diagnosis.md`
- Expected directory: `fixtures/editor_first_pr_bridge/setup_ok/expected`
- Reason: Setup diagnosis finds a packet and exposes summary/open actions without diagnostic-scoped repair commands.
- Errors: none

### Editor First-PR Bridge `packet_missing`

- Packet state: `missing` (expected `missing`)
- Diagnostics: 0 (expected 0)
- Fail closed: true (expected true)
- Safe actions: `copy_regeneration_guidance` (expected `copy_regeneration_guidance`)
- Suppressed actions: `open_packet, copy_summary, copy_repair_packet, copy_verify_command, copy_receipt_command` (expected `open_packet, copy_summary, copy_repair_packet, copy_verify_command, copy_receipt_command`)
- First-pr action commands: `copy_regeneration_guidance`
- All action commands: `ripr.copyFirstPrRegenerationGuidance, ripr.refresh`
- Receipt movement: `` (expected ``)
- Non-claims: runtime=false, mutation=false, policy_gate=false, pr_ready=false
- First-pr status JSON: `fixtures/editor_first_pr_bridge/packet_missing/expected/first-pr-status.json`
- Diagnostics JSON: `fixtures/editor_first_pr_bridge/packet_missing/expected/lsp-diagnostics.json`
- Code actions JSON: `fixtures/editor_first_pr_bridge/packet_missing/expected/lsp-code-actions.json`
- VS Code status JSON: `fixtures/editor_first_pr_bridge/packet_missing/expected/vscode-status.json`
- Setup diagnosis: `fixtures/editor_first_pr_bridge/packet_missing/expected/setup-diagnosis.md`
- Expected directory: `fixtures/editor_first_pr_bridge/packet_missing/expected`
- Reason: Missing first-pr packet fails closed and leaves regeneration guidance plus refresh.
- Errors: none

### Editor First-PR Bridge `packet_found_repairable`

- Packet state: `top_repairable_gap` (expected `top_repairable_gap`)
- Diagnostics: 1 (expected 1)
- Fail closed: false (expected false)
- Safe actions: `open_packet, copy_summary, copy_repair_packet, copy_verify_command, copy_receipt_command, copy_regeneration_guidance` (expected `open_packet, copy_summary, copy_repair_packet, copy_verify_command, copy_receipt_command, copy_regeneration_guidance`)
- Suppressed actions: `` (expected ``)
- First-pr action commands: `open_packet, copy_summary, copy_repair_packet, copy_verify_command, copy_receipt_command, copy_regeneration_guidance`
- All action commands: `ripr.openFirstPrPacket, ripr.copyFirstPrSummary, ripr.copyFirstPrRepairPacket, ripr.copyFirstPrVerifyCommand, ripr.copyFirstPrReceiptCommand, ripr.copyFirstPrRegenerationGuidance, ripr.refresh`
- Receipt movement: `` (expected ``)
- Non-claims: runtime=false, mutation=false, policy_gate=false, pr_ready=false
- First-pr status JSON: `fixtures/editor_first_pr_bridge/packet_found_repairable/expected/first-pr-status.json`
- Diagnostics JSON: `fixtures/editor_first_pr_bridge/packet_found_repairable/expected/lsp-diagnostics.json`
- Code actions JSON: `fixtures/editor_first_pr_bridge/packet_found_repairable/expected/lsp-code-actions.json`
- VS Code status JSON: `fixtures/editor_first_pr_bridge/packet_found_repairable/expected/vscode-status.json`
- Setup diagnosis: `fixtures/editor_first_pr_bridge/packet_found_repairable/expected/setup-diagnosis.md`
- Expected directory: `fixtures/editor_first_pr_bridge/packet_found_repairable/expected`
- Reason: Repairable first-pr packet exposes bounded open, summary, repair, verify, receipt, and regeneration actions.
- Errors: none

### Editor First-PR Bridge `packet_no_action`

- Packet state: `no_action` (expected `no_action`)
- Diagnostics: 0 (expected 0)
- Fail closed: false (expected false)
- Safe actions: `open_packet, copy_summary, copy_regeneration_guidance` (expected `open_packet, copy_summary, copy_regeneration_guidance`)
- Suppressed actions: `copy_repair_packet, copy_verify_command, copy_receipt_command` (expected `copy_repair_packet, copy_verify_command, copy_receipt_command`)
- First-pr action commands: `open_packet, copy_summary, copy_regeneration_guidance`
- All action commands: `ripr.openFirstPrPacket, ripr.copyFirstPrSummary, ripr.copyFirstPrRegenerationGuidance, ripr.refresh`
- Receipt movement: `` (expected ``)
- Non-claims: runtime=false, mutation=false, policy_gate=false, pr_ready=false
- First-pr status JSON: `fixtures/editor_first_pr_bridge/packet_no_action/expected/first-pr-status.json`
- Diagnostics JSON: `fixtures/editor_first_pr_bridge/packet_no_action/expected/lsp-diagnostics.json`
- Code actions JSON: `fixtures/editor_first_pr_bridge/packet_no_action/expected/lsp-code-actions.json`
- VS Code status JSON: `fixtures/editor_first_pr_bridge/packet_no_action/expected/vscode-status.json`
- Setup diagnosis: `fixtures/editor_first_pr_bridge/packet_no_action/expected/setup-diagnosis.md`
- Expected directory: `fixtures/editor_first_pr_bridge/packet_no_action/expected`
- Reason: No-action packet remains inspectable without diagnostic-scoped repair commands.
- Errors: none

### Editor First-PR Bridge `packet_stale`

- Packet state: `stale` (expected `stale`)
- Diagnostics: 0 (expected 0)
- Fail closed: true (expected true)
- Safe actions: `copy_regeneration_guidance` (expected `copy_regeneration_guidance`)
- Suppressed actions: `open_packet, copy_summary, copy_repair_packet, copy_verify_command, copy_receipt_command` (expected `open_packet, copy_summary, copy_repair_packet, copy_verify_command, copy_receipt_command`)
- First-pr action commands: `copy_regeneration_guidance`
- All action commands: `ripr.copyFirstPrRegenerationGuidance, ripr.refresh`
- Receipt movement: `` (expected ``)
- Non-claims: runtime=false, mutation=false, policy_gate=false, pr_ready=false
- First-pr status JSON: `fixtures/editor_first_pr_bridge/packet_stale/expected/first-pr-status.json`
- Diagnostics JSON: `fixtures/editor_first_pr_bridge/packet_stale/expected/lsp-diagnostics.json`
- Code actions JSON: `fixtures/editor_first_pr_bridge/packet_stale/expected/lsp-code-actions.json`
- VS Code status JSON: `fixtures/editor_first_pr_bridge/packet_stale/expected/vscode-status.json`
- Setup diagnosis: `fixtures/editor_first_pr_bridge/packet_stale/expected/setup-diagnosis.md`
- Expected directory: `fixtures/editor_first_pr_bridge/packet_stale/expected`
- Reason: Stale packet fails closed and requires regeneration before repair actions.
- Errors: none

### Editor First-PR Bridge `packet_wrong_root`

- Packet state: `wrong_root` (expected `wrong_root`)
- Diagnostics: 0 (expected 0)
- Fail closed: true (expected true)
- Safe actions: `copy_regeneration_guidance` (expected `copy_regeneration_guidance`)
- Suppressed actions: `open_packet, copy_summary, copy_repair_packet, copy_verify_command, copy_receipt_command` (expected `open_packet, copy_summary, copy_repair_packet, copy_verify_command, copy_receipt_command`)
- First-pr action commands: `copy_regeneration_guidance`
- All action commands: `ripr.copyFirstPrRegenerationGuidance, ripr.refresh`
- Receipt movement: `` (expected ``)
- Non-claims: runtime=false, mutation=false, policy_gate=false, pr_ready=false
- First-pr status JSON: `fixtures/editor_first_pr_bridge/packet_wrong_root/expected/first-pr-status.json`
- Diagnostics JSON: `fixtures/editor_first_pr_bridge/packet_wrong_root/expected/lsp-diagnostics.json`
- Code actions JSON: `fixtures/editor_first_pr_bridge/packet_wrong_root/expected/lsp-code-actions.json`
- VS Code status JSON: `fixtures/editor_first_pr_bridge/packet_wrong_root/expected/vscode-status.json`
- Setup diagnosis: `fixtures/editor_first_pr_bridge/packet_wrong_root/expected/setup-diagnosis.md`
- Expected directory: `fixtures/editor_first_pr_bridge/packet_wrong_root/expected`
- Reason: Wrong-root packet fails closed and suppresses open/copy repair actions.
- Errors: none

### Editor First-PR Bridge `packet_malformed`

- Packet state: `malformed` (expected `malformed`)
- Diagnostics: 0 (expected 0)
- Fail closed: true (expected true)
- Safe actions: `copy_regeneration_guidance` (expected `copy_regeneration_guidance`)
- Suppressed actions: `open_packet, copy_summary, copy_repair_packet, copy_verify_command, copy_receipt_command` (expected `open_packet, copy_summary, copy_repair_packet, copy_verify_command, copy_receipt_command`)
- First-pr action commands: `copy_regeneration_guidance`
- All action commands: `ripr.copyFirstPrRegenerationGuidance, ripr.refresh`
- Receipt movement: `` (expected ``)
- Non-claims: runtime=false, mutation=false, policy_gate=false, pr_ready=false
- First-pr status JSON: `fixtures/editor_first_pr_bridge/packet_malformed/expected/first-pr-status.json`
- Diagnostics JSON: `fixtures/editor_first_pr_bridge/packet_malformed/expected/lsp-diagnostics.json`
- Code actions JSON: `fixtures/editor_first_pr_bridge/packet_malformed/expected/lsp-code-actions.json`
- VS Code status JSON: `fixtures/editor_first_pr_bridge/packet_malformed/expected/vscode-status.json`
- Setup diagnosis: `fixtures/editor_first_pr_bridge/packet_malformed/expected/setup-diagnosis.md`
- Expected directory: `fixtures/editor_first_pr_bridge/packet_malformed/expected`
- Reason: Malformed packet fails closed and suppresses packet-derived actions.
- Errors: none

### Editor First-PR Bridge `receipt_improved_packet_ready`

- Packet state: `top_repairable_gap` (expected `top_repairable_gap`)
- Diagnostics: 1 (expected 1)
- Fail closed: false (expected false)
- Safe actions: `open_packet, copy_summary, copy_repair_packet, copy_verify_command, copy_receipt_command, copy_regeneration_guidance` (expected `open_packet, copy_summary, copy_repair_packet, copy_verify_command, copy_receipt_command, copy_regeneration_guidance`)
- Suppressed actions: `` (expected ``)
- First-pr action commands: `open_packet, copy_summary, copy_repair_packet, copy_verify_command, copy_receipt_command, copy_regeneration_guidance`
- All action commands: `ripr.openFirstPrPacket, ripr.copyFirstPrSummary, ripr.copyFirstPrRepairPacket, ripr.copyFirstPrVerifyCommand, ripr.copyFirstPrReceiptCommand, ripr.copyFirstPrRegenerationGuidance, ripr.refresh`
- Receipt movement: `improved` (expected `improved`)
- Non-claims: runtime=false, mutation=false, policy_gate=false, pr_ready=false
- First-pr status JSON: `fixtures/editor_first_pr_bridge/receipt_improved_packet_ready/expected/first-pr-status.json`
- Diagnostics JSON: `fixtures/editor_first_pr_bridge/receipt_improved_packet_ready/expected/lsp-diagnostics.json`
- Code actions JSON: `fixtures/editor_first_pr_bridge/receipt_improved_packet_ready/expected/lsp-code-actions.json`
- VS Code status JSON: `fixtures/editor_first_pr_bridge/receipt_improved_packet_ready/expected/vscode-status.json`
- Setup diagnosis: `fixtures/editor_first_pr_bridge/receipt_improved_packet_ready/expected/setup-diagnosis.md`
- Expected directory: `fixtures/editor_first_pr_bridge/receipt_improved_packet_ready/expected`
- Reason: Improved receipt is visible alongside the packet without claiming PR readiness.
- Errors: none

### Editor First-PR Bridge `receipt_unchanged_packet_ready`

- Packet state: `top_repairable_gap` (expected `top_repairable_gap`)
- Diagnostics: 1 (expected 1)
- Fail closed: false (expected false)
- Safe actions: `open_packet, copy_summary, copy_repair_packet, copy_verify_command, copy_receipt_command, copy_regeneration_guidance` (expected `open_packet, copy_summary, copy_repair_packet, copy_verify_command, copy_receipt_command, copy_regeneration_guidance`)
- Suppressed actions: `` (expected ``)
- First-pr action commands: `open_packet, copy_summary, copy_repair_packet, copy_verify_command, copy_receipt_command, copy_regeneration_guidance`
- All action commands: `ripr.openFirstPrPacket, ripr.copyFirstPrSummary, ripr.copyFirstPrRepairPacket, ripr.copyFirstPrVerifyCommand, ripr.copyFirstPrReceiptCommand, ripr.copyFirstPrRegenerationGuidance, ripr.refresh`
- Receipt movement: `unchanged` (expected `unchanged`)
- Non-claims: runtime=false, mutation=false, policy_gate=false, pr_ready=false
- First-pr status JSON: `fixtures/editor_first_pr_bridge/receipt_unchanged_packet_ready/expected/first-pr-status.json`
- Diagnostics JSON: `fixtures/editor_first_pr_bridge/receipt_unchanged_packet_ready/expected/lsp-diagnostics.json`
- Code actions JSON: `fixtures/editor_first_pr_bridge/receipt_unchanged_packet_ready/expected/lsp-code-actions.json`
- VS Code status JSON: `fixtures/editor_first_pr_bridge/receipt_unchanged_packet_ready/expected/vscode-status.json`
- Setup diagnosis: `fixtures/editor_first_pr_bridge/receipt_unchanged_packet_ready/expected/setup-diagnosis.md`
- Expected directory: `fixtures/editor_first_pr_bridge/receipt_unchanged_packet_ready/expected`
- Reason: Unchanged receipt stays visible and advisory alongside the first-pr packet.
- Errors: none

## Finding Alignment Receipts

These receipts validate real RIPR PR examples of the raw-finding -> canonical-item -> user-outcome model. They keep raw findings as supporting evidence, require canonical item counts and user outcomes, and check that actionable items have repair and verification routes while static limitations name analyzer repair routes. They do not change PR/CI rendering, LSP/editor behavior, gates, public scores, generated tests, provider calls, source edits, or mutation execution.

- Default CI blocking: no
- Receipt input: `fixtures/finding-alignment-dogfood/corpus.json`

| Case | PR | Gap ID | Class | Raw -> canonical | State | Actionability | Outcome | Repair | Verify | Static limitation |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- | --- | --- |
| `presentation_text_actionable_output_observer` | `EffortlessMetrics/ripr#947` | `gap:presentation_text_actionable_output_observer` | `presentation_text` | 2 -> 1 | `actionable` | `add_output_observer` | `actionable_gap` | `output_observer` | `cargo xtask evidence-quality-scorecard` | `none` |
| `presentation_text_already_observed_output` | `EffortlessMetrics/ripr#947` | `gap:presentation_text_already_observed_output` | `presentation_text` | 2 -> 1 | `already_observed` | `already_observed` | `no_action` | `no_action` | `cargo xtask evidence-quality-scorecard` | `none` |
| `config_policy_internal_metadata_no_action` | `EffortlessMetrics/ripr#1016` | `gap:config_policy_internal_metadata_no_action` | `config_or_policy_constant` | 2 -> 1 | `internal_only` | `no_action_internal` | `no_action` | `no_action` | `cargo xtask evidence-quality-scorecard` | `none` |
| `config_policy_rendered_label_unobserved` | `EffortlessMetrics/ripr#1016` | `gap:config_policy_rendered_label_unobserved` | `config_or_policy_constant` | 2 -> 1 | `actionable` | `add_output_observer` | `actionable_gap` | `output_observer` | `cargo xtask evidence-quality-scorecard` | `none` |
| `config_policy_behavior_selector_unobserved` | `EffortlessMetrics/ripr#1016` | `gap:config_policy_behavior_selector_unobserved` | `config_or_policy_constant` | 2 -> 1 | `actionable` | `add_behavior_discriminator` | `actionable_gap` | `behavior_discriminator` | `cargo xtask evidence-quality-scorecard` | `none` |
| `config_policy_behavior_selector_observed` | `EffortlessMetrics/ripr#1016` | `gap:config_policy_behavior_selector_observed` | `config_or_policy_constant` | 2 -> 1 | `already_observed` | `already_observed` | `no_action` | `no_action` | `cargo xtask evidence-quality-scorecard` | `none` |
| `config_policy_flow_unknown_limitation` | `EffortlessMetrics/ripr#1016` | `gap:config_policy_flow_unknown_limitation` | `config_or_policy_constant` | 2 -> 1 | `static_limitation` | `inspect_config_flow` | `static_limitation` | `inspect_config_flow` | `cargo xtask evidence-quality-scorecard` | `config_policy_flow_unknown` |
| `config_policy_opaque_lookup_limitation` | `EffortlessMetrics/ripr#1016` | `gap:config_policy_opaque_lookup_limitation` | `config_or_policy_constant` | 2 -> 1 | `static_limitation` | `inspect_config_flow` | `static_limitation` | `inspect_config_flow` | `cargo xtask evidence-quality-scorecard` | `opaque_config_lookup` |
| `config_policy_opaque_lookup_supported_observer` | `EffortlessMetrics/ripr-swarm#252` | `gap:config_policy_opaque_lookup_supported_observer` | `config_or_policy_constant` | 2 -> 1 | `actionable` | `add_output_observer` | `actionable_gap` | `output_observer` | `cargo xtask evidence-quality-scorecard` | `none` |
| `predicate_boundary_actionable_scorecard_lead` | `EffortlessMetrics/ripr-swarm#266` | `gap:predicate_boundary_actionable_scorecard_lead` | `predicate_boundary` | 1 -> 1 | `actionable` | `add_boundary_discriminator` | `actionable_gap` | `add_boundary_assertion` | `cargo xtask evidence-quality-trend` | `none` |
| `call_presence_runtime_static_only_trend` | `EffortlessMetrics/ripr-swarm#272` | `gap:call_presence_runtime_static_only_trend` | `call_presence` | 1 -> 1 | `static_limitation` | `expand_runtime_calibration` | `static_limitation` | `calibration_fixture_expansion` | `cargo xtask evidence-quality-trend` | `runtime_confidence_static_only` |
| `value_resolution_struct_literal_projection_zero_movement` | `EffortlessMetrics/ripr-swarm#292` | `gap:value_resolution_struct_literal_projection_zero_movement` | `predicate_boundary` | 1 -> 1 | `already_observed` | `already_observed` | `no_action` | `no_action` | `cargo xtask lane1-evidence-audit` | `none` |
| `call_presence_target_affinity_method_chain_backlog_subroute` | `EffortlessMetrics/ripr-swarm#710` | `gap:f7af85038d305337` | `call_presence` | 1 -> 1 | `static_limitation` | `static_limitation` | `static_limitation` | `analyzer_backlog` | `cargo xtask ripr-swarm readiness` | `activation_owner_call_absent_call_presence_target_affinity` |
| `limitation_backlog_nonclaims_candidate_values` | `EffortlessMetrics/ripr-swarm#716` | `gap:idx-offset-local` | `predicate_boundary` | 1 -> 1 | `static_limitation` | `static_limitation` | `static_limitation` | `analyzer_backlog` | `cargo test -p xtask static_limitation_backlog -- --test-threads=1` | `activation_boundary_input_unresolved` |

### Finding Alignment `presentation_text_actionable_output_observer`

- Source PR: `EffortlessMetrics/ripr#947`
- Canonical gap ID: `gap:presentation_text_actionable_output_observer`
- Evidence class: `presentation_text`
- Counts: 2 raw finding(s) -> 1 canonical item(s)
- Raw finding summary: A changed presentation-text declaration and literal align into one canonical output-observer item.
- Gap state: `actionable`
- Actionability: `add_output_observer`
- User outcome: `actionable_gap`
- Recommended repair: Add or update a help-output snapshot assertion for the changed presentation text.
- Before/after context: Before finding alignment, the declaration and literal could look like separate raw gaps; after alignment they are one actionable canonical output-observer repair.
- Repair kind: `output_observer` / target `help_output_snapshot`
- Verify command: `cargo xtask evidence-quality-scorecard`
- Static limitation: `none` via `none`
- Raw findings supporting-only: true
- Must not claim: `Do not count the declaration and literal as two user actions.; Do not infer actionability from raw static class.; Do not recommend mutation testing before output-observer work.`
- Reason: A supported user-visible help label without an observer should become one actionable canonical output-observer repair.
- Errors: none

### Finding Alignment `presentation_text_already_observed_output`

- Source PR: `EffortlessMetrics/ripr#947`
- Canonical gap ID: `gap:presentation_text_already_observed_output`
- Evidence class: `presentation_text`
- Counts: 2 raw finding(s) -> 1 canonical item(s)
- Raw finding summary: A changed presentation-text declaration and literal align into one canonical item with an existing output observer.
- Gap state: `already_observed`
- Actionability: `already_observed`
- User outcome: `no_action`
- Recommended repair: No new RIPR action; the existing golden observer covers the rendered output.
- Before/after context: Before alignment, raw text changes stayed visible but ambiguous; after alignment the existing golden keeps the canonical item no-action.
- Repair kind: `no_action` / target `golden`
- Verify command: `cargo xtask evidence-quality-scorecard`
- Static limitation: `none` via `none`
- Raw findings supporting-only: true
- Must not claim: `Do not emit an actionable gap when a supported output observer exists.; Do not hide the raw findings.; Do not infer actionability from raw static class.`
- Reason: A supported rendered-output observer should keep the canonical item visible as already observed, not user repair work.
- Errors: none

### Finding Alignment `config_policy_internal_metadata_no_action`

- Source PR: `EffortlessMetrics/ripr#1016`
- Canonical gap ID: `gap:config_policy_internal_metadata_no_action`
- Evidence class: `config_or_policy_constant`
- Counts: 2 raw finding(s) -> 1 canonical item(s)
- Raw finding summary: A changed policy metadata declaration and literal align into one internal-only canonical item.
- Gap state: `internal_only`
- Actionability: `no_action_internal`
- User outcome: `no_action`
- Recommended repair: No user test action; the constant is internal policy metadata in fixture-backed scope.
- Before/after context: Before config/policy alignment, internal metadata could resemble user-facing debt; after alignment it remains counted but no-action.
- Repair kind: `no_action` / target `none`
- Verify command: `cargo xtask evidence-quality-scorecard`
- Static limitation: `none` via `none`
- Raw findings supporting-only: true
- Must not claim: `Do not create user test debt from internal policy metadata alone.; Do not hide the raw declaration and literal findings.; Do not infer actionability from raw static class.`
- Reason: Internal policy metadata should align into one canonical no-action item instead of a user-facing gap.
- Errors: none

### Finding Alignment `config_policy_rendered_label_unobserved`

- Source PR: `EffortlessMetrics/ripr#1016`
- Canonical gap ID: `gap:config_policy_rendered_label_unobserved`
- Evidence class: `config_or_policy_constant`
- Counts: 2 raw finding(s) -> 1 canonical item(s)
- Raw finding summary: A rendered policy-label declaration and literal align into one canonical output-observer item.
- Gap state: `actionable`
- Actionability: `add_output_observer`
- User outcome: `actionable_gap`
- Recommended repair: Add or update a report-render, config-output, snapshot, or golden observer for the rendered policy label.
- Before/after context: Before structured repair-route checks, prose or class-local metadata could look sufficient; after the burn-down this actionable item requires a concrete repair and verify command.
- Repair kind: `output_observer` / target `report_render_or_golden`
- Verify command: `cargo xtask evidence-quality-scorecard`
- Static limitation: `none` via `none`
- Raw findings supporting-only: true
- Must not claim: `Do not count declaration and literal findings as separate user actions.; Do not infer actionability from raw static class.; Do not recommend mutation testing before output-observer work.`
- Reason: A rendered config or policy label with no supported observer should become one actionable output-observer item.
- Errors: none

### Finding Alignment `config_policy_behavior_selector_unobserved`

- Source PR: `EffortlessMetrics/ripr#1016`
- Canonical gap ID: `gap:config_policy_behavior_selector_unobserved`
- Evidence class: `config_or_policy_constant`
- Counts: 2 raw finding(s) -> 1 canonical item(s)
- Raw finding summary: A changed behavior selector declaration and literal align into one canonical behavior-discriminator item.
- Gap state: `actionable`
- Actionability: `add_behavior_discriminator`
- User outcome: `actionable_gap`
- Recommended repair: Add or update a validation or routing behavior discriminator for the changed selector.
- Before/after context: Before verify-command coverage, actionable selector examples could omit concrete movement proof; after the burn-down they carry the verify route explicitly.
- Repair kind: `behavior_discriminator` / target `validation_behavior`
- Verify command: `cargo xtask evidence-quality-scorecard`
- Static limitation: `none` via `none`
- Raw findings supporting-only: true
- Must not claim: `Do not treat behavior selectors as output labels.; Do not count declaration and literal findings as separate user actions.; Do not accept lexical-only tests as behavior discriminators.; Do not recommend mutation testing before behavior-discriminator work.`
- Reason: A supported validation or routing selector without a discriminator should become one actionable canonical behavior-discriminator repair.
- Errors: none

### Finding Alignment `config_policy_behavior_selector_observed`

- Source PR: `EffortlessMetrics/ripr#1016`
- Canonical gap ID: `gap:config_policy_behavior_selector_observed`
- Evidence class: `config_or_policy_constant`
- Counts: 2 raw finding(s) -> 1 canonical item(s)
- Raw finding summary: A behavior selector declaration and literal align into one canonical item with a supported discriminator.
- Gap state: `already_observed`
- Actionability: `already_observed`
- User outcome: `no_action`
- Recommended repair: No new RIPR action; the existing validation behavior test covers the selector.
- Before/after context: Before alignment, selector raw findings could overstate repair work; after alignment the existing behavior discriminator keeps the item already observed.
- Repair kind: `no_action` / target `validation_behavior`
- Verify command: `cargo xtask evidence-quality-scorecard`
- Static limitation: `none` via `none`
- Raw findings supporting-only: true
- Must not claim: `Do not emit an actionable behavior-discriminator gap when a supported behavior observer exists.; Do not hide the raw selector findings.; Do not infer observation from lexical mentions alone.`
- Reason: A supported validation or routing discriminator should keep the selector visible as already observed, not user repair work.
- Errors: none

### Finding Alignment `config_policy_flow_unknown_limitation`

- Source PR: `EffortlessMetrics/ripr#1016`
- Canonical gap ID: `gap:config_policy_flow_unknown_limitation`
- Evidence class: `config_or_policy_constant`
- Counts: 2 raw finding(s) -> 1 canonical item(s)
- Raw finding summary: A config constant declaration and literal align into one canonical item whose flow is not traceable by the current analyzer.
- Gap state: `static_limitation`
- Actionability: `inspect_config_flow`
- User outcome: `static_limitation`
- Recommended repair: Trace the config constant to a supported output, schema, validation, or behavior sink before claiming user test debt.
- Before/after context: Before named static limitations, generic unknowns could hide analyzer work; after the burn-down this remains a named analyzer repair route, not user test debt.
- Repair kind: `inspect_config_flow` / target `unknown`
- Verify command: `cargo xtask evidence-quality-scorecard`
- Static limitation: `config_policy_flow_unknown` via `trace_constant_to_output_schema_validation_or_behavior_sink`
- Raw findings supporting-only: true
- Must not claim: `Do not treat generic static_unknown as user test debt.; Do not hide the raw findings.; Do not recommend mutation testing before naming the static limitation.`
- Reason: Untraceable config flow should remain a named analyzer limitation with a repair route, not a user-facing test gap.
- Errors: none

### Finding Alignment `config_policy_opaque_lookup_limitation`

- Source PR: `EffortlessMetrics/ripr#1016`
- Canonical gap ID: `gap:config_policy_opaque_lookup_limitation`
- Evidence class: `config_or_policy_constant`
- Counts: 2 raw finding(s) -> 1 canonical item(s)
- Raw finding summary: A config constant declaration and literal align into one canonical item blocked by an opaque lookup path.
- Gap state: `static_limitation`
- Actionability: `inspect_config_flow`
- User outcome: `static_limitation`
- Recommended repair: Inspect the opaque config lookup path or add fixture-backed analyzer support before claiming output-observer debt.
- Before/after context: Before opaque lookup support, this class stayed static-only; after later fixture-backed support, only supported lookup shapes can leave this limitation bucket.
- Repair kind: `inspect_config_flow` / target `unknown`
- Verify command: `cargo xtask evidence-quality-scorecard`
- Static limitation: `opaque_config_lookup` via `add_fixture_backed_support_for_opaque_config_lookup`
- Raw findings supporting-only: true
- Must not claim: `Do not treat opaque lookup static_unknown as user test debt.; Do not infer user-visible config output through an opaque helper.; Do not hide the raw declaration and literal findings.`
- Reason: An opaque config lookup should stay a named static limitation until the analyzer can trace the lookup shape.
- Errors: none

### Finding Alignment `config_policy_opaque_lookup_supported_observer`

- Source PR: `EffortlessMetrics/ripr-swarm#252`
- Canonical gap ID: `gap:config_policy_opaque_lookup_supported_observer`
- Evidence class: `config_or_policy_constant`
- Counts: 2 raw finding(s) -> 1 canonical item(s)
- Raw finding summary: A fixture-backed opaque config lookup declaration and literal align into one canonical output-observer item.
- Gap state: `actionable`
- Actionability: `add_output_observer`
- User outcome: `actionable_gap`
- Recommended repair: Add or update a report-render, snapshot, or golden observer for the supported opaque lookup output.
- Before/after context: Before swarm #252, opaque config lookup examples stayed in `opaque_config_lookup`; after fixture-backed support, supported lookup shapes can become actionable output-observer repairs while unsupported lookup shapes stay limited.
- Repair kind: `output_observer` / target `report_render_or_golden`
- Verify command: `cargo xtask evidence-quality-scorecard`
- Static limitation: `none` via `none`
- Raw findings supporting-only: true
- Must not claim: `Do not promote all opaque config lookups out of static limitation.; Do not count declaration and literal raw findings as separate user actions.; Do not recommend mutation testing before output-observer work.`
- Reason: A supported opaque config lookup with a rendered output sink should become one actionable canonical output-observer repair while unsupported lookup shapes remain named limitations.
- Errors: none

### Finding Alignment `predicate_boundary_actionable_scorecard_lead`

- Source PR: `EffortlessMetrics/ripr-swarm#266`
- Canonical gap ID: `gap:predicate_boundary_actionable_scorecard_lead`
- Evidence class: `predicate_boundary`
- Counts: 1 raw finding(s) -> 1 canonical item(s)
- Raw finding summary: A changed predicate boundary becomes one actionable canonical gap with a boundary-value discriminator repair.
- Gap state: `actionable`
- Actionability: `add_boundary_discriminator`
- User outcome: `actionable_gap`
- Recommended repair: Add or update a focused boundary-value assertion for the changed predicate.
- Before/after context: Before swarm #266, trend output did not lead with actionable canonical gaps; after #266, actionable predicate-boundary gaps lead the scorecard and trend while raw findings remain diagnostic.
- Repair kind: `add_boundary_assertion` / target `boundary_value_test`
- Verify command: `cargo xtask evidence-quality-trend`
- Static limitation: `none` via `none`
- Raw findings supporting-only: true
- Must not claim: `Do not treat the raw predicate finding as the user-facing count.; Do not infer coverage adequacy from the static boundary repair route.; Do not recommend mutation testing before the boundary assertion work.`
- Reason: The top actionable class should be represented as one canonical repair packet with a concrete verify route, not as raw finding volume.
- Errors: none

### Finding Alignment `call_presence_runtime_static_only_trend`

- Source PR: `EffortlessMetrics/ripr-swarm#272`
- Canonical gap ID: `gap:call_presence_runtime_static_only_trend`
- Evidence class: `call_presence`
- Counts: 1 raw finding(s) -> 1 canonical item(s)
- Raw finding summary: A call-presence canonical item remains static-only in runtime confidence trend output.
- Gap state: `static_limitation`
- Actionability: `expand_runtime_calibration`
- User outcome: `static_limitation`
- Recommended repair: Add imported runtime calibration fixture support for the selected call-presence class before claiming runtime confidence.
- Before/after context: Before swarm #272, static-only runtime confidence classes were visible only in the scorecard table; after #272, the trend report names top static-only classes such as `call_presence` without creating user test debt.
- Repair kind: `calibration_fixture_expansion` / target `runtime_fixture_import`
- Verify command: `cargo xtask evidence-quality-trend`
- Static limitation: `runtime_confidence_static_only` via `calibration/runtime-fixtures-v3`
- Raw findings supporting-only: true
- Must not claim: `Do not treat static-only runtime confidence as mutation proof.; Do not create user test debt from calibration availability alone.; Do not change gate or badge authority from this advisory trend row.`
- Reason: The runtime-confidence trend should make static-only canonical evidence classes visible as calibration work, not user-facing repair gaps.
- Errors: none

### Finding Alignment `value_resolution_struct_literal_projection_zero_movement`

- Source PR: `EffortlessMetrics/ripr-swarm#292`
- Canonical gap ID: `gap:value_resolution_struct_literal_projection_zero_movement`
- Evidence class: `predicate_boundary`
- Counts: 1 raw finding(s) -> 1 canonical item(s)
- Raw finding summary: A same-test struct literal field projection with source-order and mutation guards is fixture-backed as already supported value-resolution evidence.
- Gap state: `already_observed`
- Actionability: `already_observed`
- User outcome: `no_action`
- Recommended repair: No new RIPR action for the selected sub-shape; the existing value-resolution path already records concrete activation values for the supported fixture-backed projection.
- Before/after context: Before swarm #292, the selected value-resolution sub-shape was fixture-backed but not recorded as a dogfood receipt; after #292, the audit delta records zero movement because the implementation already supported it while 315 `activation_value_unresolved` limitations remain analyzer work.
- Repair kind: `no_action` / target `boundary_value_test`
- Verify command: `cargo xtask lane1-evidence-audit`
- Static limitation: `none` via `none`
- Raw findings supporting-only: true
- Must not claim: `Do not claim the sampled Lane 1 scorecard improved from this zero-movement slice.; Do not promote helper-built, cross-file, generated, macro-expanded, shadowed, pattern-bound, non-literal, or opaque value flows out of named limitations.; Do not treat static value-resolution evidence as runtime, mutation, coverage, badge, or gate proof.`
- Reason: A supported value-resolution sub-shape should be recorded as no-action dogfood evidence when audit proof shows zero movement and remaining unsupported shapes stay named limitations.
- Errors: none

### Finding Alignment `call_presence_target_affinity_method_chain_backlog_subroute`

- Source PR: `EffortlessMetrics/ripr-swarm#710`
- Canonical gap ID: `gap:f7af85038d305337`
- Evidence class: `call_presence`
- Counts: 1 raw finding(s) -> 1 canonical item(s)
- Raw finding summary: A call-presence method-chain seam with target-token affinity remains a named owner-call limitation, not a repair packet.
- Gap state: `static_limitation`
- Actionability: `static_limitation`
- User outcome: `static_limitation`
- Recommended repair: Improve call-presence target-affinity owner-call tracing for method-chain expressions before emitting any public repair packet.
- Before/after context: Before swarm #710, the sampled top call-presence target-affinity backlog route used the generic call_presence_target_affinity_missing_owner_call subroute; after #710, refreshed readiness reports call_presence_target_affinity_method_chain_missing_owner_call while preserving zero actionable and zero swarm-ready packets.
- Repair kind: `analyzer_backlog` / target `owner_call_tracing_analyzer`
- Verify command: `cargo xtask ripr-swarm readiness`
- Static limitation: `activation_owner_call_absent_call_presence_target_affinity` via `analysis/call-presence-target-affinity-owner-call-tracing`
- Raw findings supporting-only: true
- Must not claim: `Do not treat this raw call-presence finding as user test debt.; Do not emit a public repair packet from target-token affinity alone.; Do not claim mutation, runtime, badge, LSP, or CI gate authority from this analyzer backlog refinement.`
- Reason: A top sampled limitation backlog route should drive a more precise analyzer work queue without crossing the actionability trust boundary.
- Errors: none

### Finding Alignment `limitation_backlog_nonclaims_candidate_values`

- Source PR: `EffortlessMetrics/ripr-swarm#716`
- Canonical gap ID: `gap:idx-offset-local`
- Evidence class: `predicate_boundary`
- Counts: 1 raw finding(s) -> 1 canonical item(s)
- Raw finding summary: An unresolved local/computed boundary operand remains a named limitation with explicit non-claims against invented candidate values.
- Gap state: `static_limitation`
- Actionability: `static_limitation`
- User outcome: `static_limitation`
- Recommended repair: Improve local/computed boundary operand resolution before emitting any candidate-value repair packet.
- Before/after context: Before swarm #716, generated limitation backlog packets denied public repair and swarm-ready status but only boundary-specific packets denied exact boundary candidate values; after #716, every generated limitation backlog packet includes `do not invent exact candidate values` while boundary packets keep the more specific boundary-value denial.
- Repair kind: `analyzer_backlog` / target `limitation_backlog_contract`
- Verify command: `cargo test -p xtask static_limitation_backlog -- --test-threads=1`
- Static limitation: `activation_boundary_input_unresolved` via `analysis/local-computed-boundary-operand-resolution`
- Raw findings supporting-only: true
- Must not claim: `Do not treat this limitation backlog item as user test debt.; Do not invent exact candidate values from static limitation samples.; Do not change badge, LSP, PR, CI, runtime, or mutation authority from this non-claim hardening.`
- Reason: A limitation backlog packet should provide analyzer direction while explicitly denying speculative candidate values until the analyzer can safely unlock actionability.
- Errors: none

## Surface Projection Alignment Receipts

These receipts validate that one receipt-backed canonical gap preserves identity, repair route, verify command, receipt command/state, and readiness `top_next_action` across swarm-attempt-ledger and swarm-readiness projections. Badge, LSP, PR, and CI remain advisory consumers of the canonical report state; this dogfood proof does not change their rendering, ranking, gate authority, source edits, generated tests, provider calls, or mutation execution.

- Default CI blocking: no
- Receipt input: `fixtures/surface-projection-alignment/corpus.json`

| Case | Gap ID | Packet | Repair | Outcome | Top next action | Attempted | Improved | Advisory consumers |
| --- | --- | --- | --- | --- | --- | ---: | ---: | --- |
| `receipt_improved_top_next_action_alignment` | `gap:boundary-discriminator-004` | `boundary-discriminator-004` | `add_boundary_assertion` | `evidence_improved` | `attempt_ready_packet` | 1 | 1 | `badge, lsp, pr_comment, ci` |
| `receipt_unchanged_route_quality_alignment` | `gap:output-observer-009` | `output-observer-009` | `add_output_observer` | `evidence_unchanged` | `improve_repair_route_quality` | 1 | 0 | `badge, lsp, pr_comment, ci` |
| `attempted_no_receipt_route_quality_alignment` | `gap:error-variant-012` | `error-variant-012` | `add_exact_error_variant_observer` | `attempted_no_receipt` | `collect_missing_attempt_receipts` | 1 | 0 | `badge, lsp, pr_comment, ci` |
| `static_limitation_backlog_alignment` | `gap:activation-owner-call-absent-assertion-target-affinity` | `limitation:activation-owner-call-absent-assertion-target-affinity:assertion-target-affinity-owner-call-tracing` | `sharpen_static_limitation_route` | `not_attempted` | `route_static_limitation_backlog` | 0 | 0 | `badge, lsp, pr_comment, ci` |

### Surface Projection Alignment `receipt_improved_top_next_action_alignment`

- Canonical gap ID: `gap:boundary-discriminator-004`
- Packet ID: `boundary-discriminator-004`
- Repair kind: `add_boundary_assertion`
- Verify command: `cargo test -p ripr-swarm boundary_discriminator_004`
- Receipt command: `cargo xtask receipts write --packet boundary-discriminator-004 --canonical-gap gap:boundary-discriminator-004`
- Receipt state: `receipt_movement_improved`
- Outcome: `evidence_improved`
- Readiness status: `advisory`
- Top next action: `attempt_ready_packet` via `cargo xtask ripr-swarm attempt --packet boundary-discriminator-004 --dry-run`
- Advisory consumers: `badge, lsp, pr_comment, ci`
- Reason: A single improved receipt should keep the same canonical gap, packet, repair kind, verify command, receipt command/state, and top readiness route while badge, LSP, PR, and CI remain advisory consumers.
- Errors: none

### Surface Projection Alignment `receipt_unchanged_route_quality_alignment`

- Canonical gap ID: `gap:output-observer-009`
- Packet ID: `output-observer-009`
- Repair kind: `add_output_observer`
- Verify command: `cargo test -p ripr-swarm output_observer_009`
- Receipt command: `cargo xtask receipts write --packet output-observer-009 --canonical-gap gap:output-observer-009`
- Receipt state: `receipt_movement_unchanged`
- Outcome: `evidence_unchanged`
- Readiness status: `advisory`
- Top next action: `improve_repair_route_quality` via `cargo xtask ripr-swarm readiness`
- Advisory consumers: `badge, lsp, pr_comment, ci`
- Reason: A receipt-backed unchanged attempt should keep the same canonical packet identity while routing surfaces toward repair-route quality improvement instead of another blind packet attempt.
- Errors: none

### Surface Projection Alignment `attempted_no_receipt_route_quality_alignment`

- Canonical gap ID: `gap:error-variant-012`
- Packet ID: `error-variant-012`
- Repair kind: `add_exact_error_variant_observer`
- Verify command: `cargo test -p ripr-swarm error_variant_012`
- Receipt command: `cargo xtask receipts write --packet error-variant-012 --canonical-gap gap:error-variant-012`
- Receipt state: `missing_receipt`
- Outcome: `attempted_no_receipt`
- Readiness status: `advisory`
- Top next action: `collect_missing_attempt_receipts` via `cargo xtask ripr-swarm attempt-ledger`
- Advisory consumers: `badge, lsp, pr_comment, ci`
- Reason: An attempted packet without a receipt should keep canonical packet identity while routing user surfaces to collect the missing receipt before claiming improvement.
- Errors: none

### Surface Projection Alignment `static_limitation_backlog_alignment`

- Canonical gap ID: `gap:activation-owner-call-absent-assertion-target-affinity`
- Packet ID: `limitation:activation-owner-call-absent-assertion-target-affinity:assertion-target-affinity-owner-call-tracing`
- Repair kind: `sharpen_static_limitation_route`
- Verify command: `cargo test -p ripr evidence_record_splits_assertion_target_owner_call_absence_limitation`
- Receipt command: `cargo xtask check-fixture-contracts`
- Receipt state: `not_attempted`
- Outcome: `not_attempted`
- Readiness status: `advisory`
- Top next action: `route_static_limitation_backlog` via `cargo xtask lane1-evidence-audit`
- Advisory consumers: `badge, lsp, pr_comment, ci`
- Reason: A static-limitation backlog route should preserve canonical limitation identity and analyzer repair route across readiness and downstream surfaces without creating a public repair packet.
- Errors: none

## Real Repair Attempt Receipts

These receipts record repo-local repair attempts that exercised the Lane 1 packet loop against real PR or handoff evidence. The set intentionally includes non-win outcomes so unchanged or missing-receipt attempts remain visible instead of being curated away.

- Default CI blocking: no
- Receipt input: `fixtures/real-repair-attempts/corpus.json`
- Attempted: 68; improved: 61; unchanged: 2; regressed: 0; resolved: 3; missing receipt: 2

| Case | Source | Language | Gap ID | Packet | Repair | Verify | Outcome |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `repair_route_quality_metrics_improved` | `EffortlessMetrics/ripr-swarm#415` | `unspecified` | `gap:repair-route-quality-metrics-missing` | `repair-route-quality-metrics-001` | `add_repair_route_quality_metrics` | `pass` | `evidence_improved` |
| `route_quality_backlog_nonclaims_actionability_boundary` | `EffortlessMetrics/ripr-swarm#722` | `unspecified` | `gap:route-quality-backlog-actionability-boundary` | `route-quality:backlog:actionability-boundary` | `harden_route_quality_backlog_nonclaims` | `pass` | `evidence_improved` |
| `missing_repair_kind_blocker_route_improved` | `EffortlessMetrics/ripr-swarm#725` | `unspecified` | `gap:missing-repair-kind-blocker-route` | `blocked-state:missing-repair-kind` | `route_missing_repair_kind_blockers` | `pass` | `evidence_improved` |
| `limitation_backlog_legacy_route_hardening_improved` | `EffortlessMetrics/ripr-swarm#731` | `unspecified` | `gap:limitation-backlog-legacy-route-hardening` | `limitation:activation-boundary-input-unresolved:local-computed` | `harden_limitation_backlog_routes` | `pass` | `evidence_improved` |
| `readiness_top_next_action_resolved` | `EffortlessMetrics/ripr-swarm#416` | `unspecified` | `gap:readiness-top-next-action-missing` | `readiness-top-next-action-001` | `add_readiness_top_next_action_projection` | `pass` | `resolved` |
| `projection_alignment_receipt_improved` | `EffortlessMetrics/ripr-swarm#417` | `unspecified` | `gap:surface-projection-alignment-receipt-missing` | `surface-projection-alignment-001` | `add_projection_alignment_receipt` | `pass` | `evidence_improved` |
| `targeted_outcome_left_gap_unchanged` | `docs/handoffs/2026-05-20-0.7-swarm-repair-loop-dogfood.md` | `unspecified` | `gap:unchanged` | `evidence-unchanged-from-targeted-outcome` | `add_exact_error_variant` | `pass` | `evidence_unchanged` |
| `exact_error_variant_guidance_route_improved` | `EffortlessMetrics/ripr-swarm#582` | `unspecified` | `gap:route-quality-add-exact-error-variant-unchanged` | `route-quality:add-exact-error-variant:unchanged` | `sharpen_repair_route_guidance` | `pass` | `evidence_improved` |
| `targeted_outcome_exact_error_variant_receipt_improved` | `local:bounded-add-exact-error-variant-receipt-2026-05-30` | `unspecified` | `gap:unchanged` | `evidence-unchanged-from-targeted-outcome` | `add_exact_error_variant` | `pass` | `evidence_improved` |
| `dry_run_receipt_command_missing_named` | `docs/handoffs/2026-05-20-0.7-swarm-repair-loop-dogfood.md` | `unspecified` | `gap:report-config-label` | `gap:report-config-label` | `add_output_observer` | `pass` | `attempted_no_receipt` |
| `report_config_label_output_observer_receipt_improved` | `local:bounded-add-output-observer-receipt-2026-05-30` | `unspecified` | `gap:report-config-label` | `gap:report-config-label` | `add_output_observer` | `pass` | `evidence_improved` |
| `local_computed_boundary_limitation_route_improved` | `EffortlessMetrics/ripr-swarm#499` | `unspecified` | `gap:activation-boundary-input-unresolved-local-computed` | `limitation:activation-boundary-input-unresolved:local-computed-boundary-operand-resolution` | `sharpen_static_limitation_route` | `pass` | `evidence_improved` |
| `parameter_field_boundary_operand_route_improved` | `local:parameter-field-boundary-operands-2026-06-01` | `unspecified` | `gap:activation-boundary-input-unresolved-parameter-field-projection` | `limitation:activation_boundary_input_unresolved:parameter_field_projection:analysis-local-computed-boundary-operand-resolution` | `resolve_parameter_field_boundary_operands` | `pass` | `evidence_improved` |
| `closure_boundary_operand_route_improved` | `local:closure-boundary-operands-2026-06-01` | `unspecified` | `gap:activation-boundary-input-unresolved-closure-derived` | `limitation:activation_boundary_input_unresolved:closure_derived:analysis-closure-boundary-operand-resolution` | `sharpen_static_limitation_route` | `pass` | `evidence_improved` |
| `same_file_method_chain_owner_call_route_split` | `EffortlessMetrics/ripr-swarm#853` | `unspecified` | `gap:activation-owner-call-absent-same-file-method-chain` | `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_method_chain_missing_owner_call:analysis-same-file-method-chain-owner-call-tracing` | `sharpen_static_limitation_route` | `pass` | `evidence_improved` |
| `same_file_method_chain_backlog_projection_route_improved` | `EffortlessMetrics/ripr-swarm#856` | `unspecified` | `gap:activation-owner-call-absent-same-file-method-chain-backlog-projection` | `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_method_chain_missing_owner_call:analysis-same-file-method-chain-owner-call-tracing` | `sharpen_static_limitation_route` | `pass` | `evidence_improved` |
| `same_file_method_chain_owner_call_helper_tests_improved` | `local:same-file-method-chain-owner-call-helper-tests-2026-06-01` | `unspecified` | `gap:activation-owner-call-absent-same-file-method-chain-helper-tests` | `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_method_chain_missing_owner_call:analysis-same-file-method-chain-owner-call-tracing` | `sharpen_static_limitation_route` | `pass` | `evidence_improved` |
| `activation_body_line_helper_owner_call_tests_improved` | `local:activation-body-line-helper-owner-calls-2026-06-01` | `unspecified` | `gap:activation-owner-call-absent-same-file-body-line-helpers` | `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_method_chain_missing_owner_call:analysis-same-file-method-chain-owner-call-tracing` | `sharpen_static_limitation_route` | `pass` | `evidence_improved` |
| `same_file_receiver_method_backlog_route_split` | `local:same-file-receiver-method-backlog-route-2026-06-01` | `unspecified` | `gap:activation-owner-call-absent-same-file-receiver-method` | `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_receiver_method_missing_owner_call:analysis-same-file-receiver-method-owner-call-tracing` | `sharpen_static_limitation_route` | `pass` | `evidence_improved` |
| `same_file_dotted_non_call_subroute_guard` | `local:same-file-dotted-non-call-subroute-2026-06-01` | `unspecified` | `gap:activation-owner-call-absent-same-file-dotted-non-call` | `limitation:activation_owner_call_absent_same_file_only:same_file_only_missing_owner_call:analysis-same-file-owner-call-tracing` | `sharpen_static_limitation_route` | `pass` | `evidence_improved` |
| `assertion_target_return_value_owner_call_route_split` | `EffortlessMetrics/ripr-swarm#859` | `unspecified` | `gap:activation-owner-call-absent-assertion-target-return-value` | `limitation:activation_owner_call_absent_assertion_target_affinity:assertion_target_affinity_return_value_missing_owner_call:analysis-assertion-target-return-value-owner-call-tracing` | `sharpen_static_limitation_route` | `pass` | `evidence_improved` |
| `call_presence_target_affinity_limitation_route_improved` | `EffortlessMetrics/ripr-swarm#555` | `unspecified` | `gap:activation-owner-call-absent-call-presence-target-affinity` | `limitation:activation_owner_call_absent_call_presence_target_affinity:analysis-call-presence-target-affinity-owner-call-tracing` | `sharpen_static_limitation_route` | `pass` | `evidence_improved` |
| `assertion_target_err_wrapped_helper_owner_call_route_improved` | `EffortlessMetrics/ripr-swarm#535` | `unspecified` | `gap:activation-owner-call-absent-assertion-target-affinity` | `limitation:activation-owner-call-absent-assertion-target-affinity:assertion-target-affinity-owner-call-tracing` | `trace_assertion_target_affinity_owner_call` | `pass` | `evidence_improved` |
| `same_file_owner_call_route_samples_improved` | `EffortlessMetrics/ripr-swarm#746` | `unspecified` | `gap:activation-owner-call-absent-same-file-only` | `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing` | `harden_limitation_route_samples` | `pass` | `evidence_improved` |
| `same_file_test_local_helper_owner_call_route_improved` | `EffortlessMetrics/ripr-swarm#751` | `unspecified` | `gap:activation-owner-call-absent-same-file-only` | `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing` | `trace_same_file_owner_call` | `pass` | `evidence_improved` |
| `same_file_three_hop_helper_owner_call_route_improved` | `local:same-file-three-hop-helper-owner-call-2026-06-01` | `unspecified` | `gap:activation-owner-call-absent-same-file-only` | `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing` | `trace_same_file_owner_call` | `pass` | `evidence_improved` |
| `same_file_method_owner_call_route_improved` | `EffortlessMetrics/ripr-swarm#761` | `unspecified` | `gap:activation-owner-call-absent-same-file-only` | `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_method_chain_missing_owner_call:analysis-same-file-owner-call-tracing` | `trace_same_file_owner_call` | `pass` | `evidence_improved` |
| `same_file_assertion_helper_owner_call_route_improved` | `EffortlessMetrics/ripr-swarm#765` | `unspecified` | `gap:activation-owner-call-absent-same-file-only` | `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing` | `trace_same_file_owner_call` | `pass` | `evidence_improved` |
| `same_file_helper_owner_call_route_improved` | `EffortlessMetrics/ripr-swarm#773` | `unspecified` | `gap:activation-owner-call-absent-same-file-only` | `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing` | `trace_same_file_owner_call` | `pass` | `evidence_improved` |
| `same_file_parent_qualified_owner_call_route_improved` | `EffortlessMetrics/ripr-swarm#774` | `unspecified` | `gap:activation-owner-call-absent-same-file-only` | `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing` | `trace_same_file_owner_call` | `pass` | `evidence_improved` |
| `trim_chain_helper_owner_call_route_improved` | `EffortlessMetrics/ripr-swarm#811` | `unspecified` | `gap:activation-owner-call-absent-same-file-only` | `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing` | `trace_same_file_owner_call` | `pass` | `evidence_improved` |
| `as_ref_chain_helper_owner_call_route_improved` | `EffortlessMetrics/ripr-swarm#831` | `unspecified` | `gap:activation-owner-call-absent-same-file-only` | `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing` | `trace_same_file_owner_call` | `pass` | `evidence_improved` |
| `turbofish_result_helper_owner_call_route_improved` | `EffortlessMetrics/ripr-swarm#820` | `unspecified` | `gap:activation-owner-call-absent-same-file-only` | `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing` | `trace_same_file_owner_call` | `pass` | `evidence_improved` |
| `dbg_macro_helper_owner_call_route_improved` | `EffortlessMetrics/ripr-swarm#825` | `unspecified` | `gap:activation-owner-call-absent-same-file-only` | `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing` | `trace_same_file_owner_call` | `pass` | `evidence_improved` |
| `format_args_macro_helper_owner_call_route_improved` | `EffortlessMetrics/ripr-swarm#829` | `unspecified` | `gap:activation-owner-call-absent-same-file-only` | `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing` | `trace_same_file_owner_call` | `pass` | `evidence_improved` |
| `block_helper_owner_call_route_improved` | `EffortlessMetrics/ripr-swarm#805` | `unspecified` | `gap:activation-owner-call-absent-same-file-only` | `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing` | `trace_same_file_owner_call` | `pass` | `evidence_improved` |
| `assert_macro_helper_owner_call_route_improved` | `EffortlessMetrics/ripr-swarm#808` | `unspecified` | `gap:activation-owner-call-absent-same-file-only` | `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing` | `trace_same_file_owner_call` | `pass` | `evidence_improved` |
| `matches_macro_helper_owner_call_route_improved` | `EffortlessMetrics/ripr-swarm#813` | `unspecified` | `gap:activation-owner-call-absent-same-file-only` | `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing` | `trace_same_file_owner_call` | `pass` | `evidence_improved` |
| `format_macro_helper_owner_call_route_improved` | `EffortlessMetrics/ripr-swarm#803` | `unspecified` | `gap:activation-owner-call-absent-same-file-only` | `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing` | `trace_same_file_owner_call` | `pass` | `evidence_improved` |
| `tuple_literal_helper_owner_call_route_improved` | `EffortlessMetrics/ripr-swarm#797` | `unspecified` | `gap:activation-owner-call-absent-same-file-only` | `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing` | `trace_same_file_owner_call` | `pass` | `evidence_improved` |
| `std_identity_helper_owner_call_route_improved` | `local:same-file-identity-owner-call-route-2026-06-01` | `unspecified` | `gap:activation-owner-call-absent-same-file-only` | `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing` | `trace_same_file_owner_call` | `pass` | `evidence_improved` |
| `vec_macro_helper_owner_call_route_improved` | `EffortlessMetrics/ripr-swarm#793` | `unspecified` | `gap:activation-owner-call-absent-same-file-only` | `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing` | `trace_same_file_owner_call` | `pass` | `evidence_improved` |
| `array_literal_helper_owner_call_route_improved` | `EffortlessMetrics/ripr-swarm#795` | `unspecified` | `gap:activation-owner-call-absent-same-file-only` | `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing` | `trace_same_file_owner_call` | `pass` | `evidence_improved` |
| `same_file_fanout_wrapper_owner_call_route_improved` | `local:same-file-fanout-wrapper-2026-06-01` | `unspecified` | `gap:activation-owner-call-absent-same-file-only` | `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing` | `trace_same_file_owner_call` | `pass` | `evidence_improved` |
| `equality_macro_later_arg_helper_owner_call_route_improved` | `local:equality-macro-later-arg-owner-call-2026-06-01` | `unspecified` | `gap:activation-owner-call-absent-same-file-only` | `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing` | `trace_same_file_owner_call` | `pass` | `evidence_improved` |
| `call_presence_imported_module_wrapper_route_improved` | `local:analysis-call-presence-target-affinity-owner-call-tracing-2026-05-30` | `unspecified` | `gap:activation-owner-call-absent-call-presence-target-affinity` | `limitation:activation_owner_call_absent_call_presence_target_affinity:analysis-call-presence-target-affinity-owner-call-tracing` | `trace_call_presence_target_affinity_owner_call` | `pass` | `evidence_improved` |
| `call_presence_same_file_unit_wrapper_route_improved` | `local:same-file-unit-target-affinity-wrapper-2026-05-30` | `unspecified` | `gap:activation-owner-call-absent-call-presence-target-affinity` | `limitation:activation_owner_call_absent_call_presence_target_affinity:analysis-call-presence-target-affinity-owner-call-tracing` | `trace_call_presence_target_affinity_owner_call` | `pass` | `evidence_improved` |
| `call_presence_direct_import_wrapper_route_improved` | `local:direct-import-target-affinity-wrapper-2026-05-30` | `unspecified` | `gap:activation-owner-call-absent-call-presence-target-affinity` | `limitation:activation_owner_call_absent_call_presence_target_affinity:analysis-call-presence-target-affinity-owner-call-tracing` | `trace_call_presence_target_affinity_owner_call` | `pass` | `evidence_improved` |
| `assertion_target_affinity_limitation_route_improved` | `EffortlessMetrics/ripr-swarm#504` | `unspecified` | `gap:activation-owner-call-absent-assertion-target-affinity` | `limitation:activation-owner-call-absent-assertion-target-affinity:assertion-target-affinity-owner-call-tracing` | `sharpen_static_limitation_route` | `pass` | `evidence_improved` |
| `iterator_boundary_limitation_route_improved` | `EffortlessMetrics/ripr-swarm#488` | `unspecified` | `gap:fixture_iterator_local_boundary` | `limitation:activation-boundary-input-unresolved:iterator-boundary-operand-resolution` | `sharpen_static_limitation_route` | `pass` | `evidence_improved` |
| `lsp_call_observer_attempted_no_receipt_timeout` | `local:dogfood-lsp-call-observer-packet` | `unspecified` | `gap:54970c87c8f8ca9d` | `gap:54970c87c8f8ca9d` | `add_call_observer` | `not_run` | `attempted_no_receipt` |
| `lsp_call_observer_bounded_receipt_improved` | `local:bounded-add-call-observer-receipt-2026-05-30` | `unspecified` | `gap:54970c87c8f8ca9d` | `gap:54970c87c8f8ca9d` | `add_call_observer` | `pass` | `evidence_improved` |
| `repair_route_quality_sample_actions_improved` | `EffortlessMetrics/ripr-swarm#575` | `unspecified` | `gap:repair-route-quality-sample-actions` | `repair-route-quality-sample-actions-001` | `add_repair_route_quality_action_samples` | `pass` | `evidence_improved` |
| `typescript_preview_lsp_repair_context_improved` | `EffortlessMetrics/ripr-swarm#617` | `typescript` | `gap:typescript-preview-lsp-repair-context` | `limitation:typescript-preview-lsp-repair-context` | `project_preview_actionability_context` | `pass` | `evidence_improved` |
| `javascript_preview_generated_ci_grouping_improved` | `EffortlessMetrics/ripr-swarm#622` | `javascript` | `gap:javascript-preview-generated-ci-grouping` | `limitation:javascript-preview-generated-ci-grouping` | `project_preview_ci_grouping` | `pass` | `evidence_improved` |
| `typescript_preview_mocked_module_limitation_improved` | `fixtures/typescript_mocked_module_limit` | `typescript` | `gap:typescript-preview-mocked-module-limitation` | `limitation:typescript-preview-mocked-module-limitation` | `sharpen_static_limitation_route` | `pass` | `evidence_improved` |
| `typescript_module_initializer_observer_route_improved` | `EffortlessMetrics/ripr-swarm#749` | `typescript` | `gap:typescript-preview-module-initializer-observer-route` | `limitation:typescript-preview-module-initializer-observer-route` | `route_typescript_module_initializer_observer` | `pass` | `evidence_improved` |
| `typescript_method_receiver_observer_route_improved` | `fixtures/typescript_owner_kinds` | `typescript` | `gap:typescript-preview-method-receiver-observer-route` | `limitation:typescript-preview-method-receiver-observer-route` | `route_typescript_method_receiver_observer` | `pass` | `evidence_improved` |
| `typescript_preview_weak_oracle_downgrade_unchanged` | `fixtures/typescript_jest_vitest_assertion_facts` | `typescript` | `gap:typescript-preview-weak-oracle-downgrade` | `limitation:typescript-preview-weak-oracle-downgrade` | `preserve_preview_weak_oracle_boundary` | `pass` | `evidence_unchanged` |
| `typescript_preview_weak_oracle_guidance_improved` | `local:typescript-preview-weak-oracle-route-guidance-2026-05-30` | `typescript` | `gap:typescript-preview-weak-oracle-downgrade` | `limitation:typescript-preview-weak-oracle-downgrade` | `preserve_preview_weak_oracle_boundary` | `pass` | `evidence_improved` |
| `typescript_preview_boundary_gap_test_only_closed` | `EffortlessMetrics/ripr-swarm#1490` | `typescript` | `gap:typescript:typescript_preview:2396aec1` | `typescript-repair-packet:gap:typescript:typescript_preview:2396aec1` | `AddBoundaryAssertion` | `pass` | `resolved` |
| `python_cli_output_repair_card_route_improved` | `EffortlessMetrics/ripr-swarm#750` | `python` | `gap:python:src/commands.py:ship:call_or_output_effect:side_effect:click.echo_shipment_queued` | `python-repair-card:gap:python:src/commands.py:ship:call_or_output_effect:side_effect:click.echo_shipment_queued` | `route_python_cli_output_repair_card` | `pass` | `evidence_improved` |
| `python_argparse_output_repair_card_route_improved` | `EffortlessMetrics/ripr-swarm#754` | `python` | `gap:python:src/commands.py:main:call_or_output_effect:side_effect:print_shipment_queued` | `python-repair-card:gap:python:src/commands.py:main:call_or_output_effect:side_effect:print_shipment_queued` | `route_python_cli_output_repair_card` | `pass` | `evidence_improved` |
| `python_api_json_field_repair_card_route_improved` | `EffortlessMetrics/ripr-swarm#759` | `python` | `gap:python:app/checkout.py:checkout:field_value:field_construction:return_detail_coupon_expired` | `python-repair-card:gap:python:app/checkout.py:checkout:field_value:field_construction:return_detail_coupon_expired` | `route_python_api_json_field_repair_card` | `pass` | `evidence_improved` |
| `python_model_field_repair_card_route_improved` | `EffortlessMetrics/ripr-swarm#772` | `python` | `gap:python:src/users.py:build_user:field_value:field_construction:return_user_active=true` | `python-repair-card:gap:python:src/users.py:build_user:field_value:field_construction:return_user_active=true` | `route_python_model_field_repair_card` | `pass` | `evidence_improved` |
| `python_parametrized_boundary_repair_card_route_improved` | `EffortlessMetrics/ripr-swarm#764` | `python` | `gap:python:src/discount.py:apply_discount:predicate_boundary:predicate:amount>=threshold` | `python-repair-card:gap:python:src/discount.py:apply_discount:predicate_boundary:predicate:amount>=threshold` | `route_python_parametrized_boundary_repair_card` | `pass` | `evidence_improved` |
| `python_preview_boundary_gap_test_only_closed` | `fixtures/first_successful_pr/python-preview-gap/expected/swarm-ingest/closed.json` | `python` | `gap:python:app/pricing.py:calculate_discount:predicate_boundary:amount>=threshold` | `gap:pr:gap:python:app/pricing.py:calculate_discount:predicate_boundary:amount>=threshold` | `strengthen_existing_pytest_boundary` | `pass` | `resolved` |
| `local_member_boundary_operand_route_split` | `local:local-member-boundary-operands-2026-06-02` | `unspecified` | `gap:activation-boundary-input-unresolved-local-member` | `limitation:activation_boundary_input_unresolved:local_member:analysis-local-member-boundary-operand-resolution` | `sharpen_static_limitation_route` | `pass` | `evidence_improved` |

### Real Repair Attempt `repair_route_quality_metrics_improved`

- Canonical gap ID: `gap:repair-route-quality-metrics-missing`
- Packet ID: `repair-route-quality-metrics-001`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `readiness and attempt-ledger report assertions over repair_kind attempted/improved/unchanged/regressed counts`
- Verify command: `cargo xtask check-pr`
- Verify result: `pass`
- Receipt command: `cargo xtask ripr-swarm readiness`
- Receipt path: `target/ripr/reports/swarm-readiness.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `actionable` -> `actionable`
- Attempted repair: Added repair-route quality rollups and top failing/missing evidence route fields to attempt-ledger/readiness outputs.
- Evidence movement: Readiness can now show repair_kind success-rate evidence instead of only packet counts.
- Operator note: The route-quality attempt improved analyzer-backlog signal without changing badge, LSP, PR, or CI authority.
- Must not change: `public badge semantics, CI gate defaults, autonomous repair behavior`
- Raw evidence refs: `metrics/capabilities.toml:ripr_swarm_repair_loop, docs/specs/RIPR-SPEC-0057-ripr-swarm-repair-loop.md`
- Receipt validation: pass

### Real Repair Attempt `route_quality_backlog_nonclaims_actionability_boundary`

- Canonical gap ID: `gap:route-quality-backlog-actionability-boundary`
- Packet ID: `route-quality:backlog:actionability-boundary`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `route-quality backlog assertions that non_claims preserve the actionability boundary and keep metrics from promoting or downgrading repair packets`
- Verify command: `cargo test -p xtask repair_route_quality -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `actionable` -> `actionable`
- Attempted repair: Added the route-quality backlog non-claim that route-quality evidence alone must not promote or downgrade actionability.
- Evidence movement: The route-quality backlog now carries an explicit actionability-boundary non-claim while keeping repair-route metrics separate from public repair packet eligibility.
- Operator note: This records route-learning safety progress. It improves analyzer backlog guidance without changing public packet eligibility, badge semantics, CI gates, or autonomous repair behavior.
- Must not change: `public repair packet actionability, public badge semantics, CI gate defaults, provider integration, mutation execution`
- Raw evidence refs: `xtask/src/main.rs:ripr_swarm_repair_route_quality_backlog_json, xtask/src/main.rs:repair_route_quality, docs/OUTPUT_SCHEMA.md:repair_route_quality_backlog, EffortlessMetrics/ripr-swarm#722`
- Receipt validation: pass

### Real Repair Attempt `missing_repair_kind_blocker_route_improved`

- Canonical gap ID: `gap:missing-repair-kind-blocker-route`
- Packet ID: `blocked-state:missing-repair-kind`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `readiness and swarm-plan assertions that packets excluded for missing_repair_kind route to missing-context remediation instead of public repair queues`
- Verify command: `cargo test -p xtask ripr_swarm_plan_routes_explicit_missing_repair_kind_projection_exclusion -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask ripr-swarm readiness`
- Receipt path: `target/ripr/reports/swarm-readiness.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `blocked_by_public_projection_exclusion` -> `blocked_by_missing_context`
- Attempted repair: Routed missing_repair_kind projection exclusions into the blocked-state/missing-context path so readiness names the field that must be supplied before a packet can be repair-ready.
- Evidence movement: improved: missing repair-kind blockers now carry a named blocked-state route, example packet identity, and next action instead of being ambiguous public projection exclusions.
- Operator note: This improves Lane 1 repair routing clarity without making an incomplete packet actionable and without changing public badge, CI, LSP, PR, or agent authority.
- Must not change: `do not emit repair packets without repair_kind, do not count missing-repair-kind blockers as public actionable gaps, do not change badge semantics, do not change CI gate defaults, do not add provider calls or autonomous edits`
- Raw evidence refs: `xtask/src/main.rs::tests::ripr_swarm_plan_routes_explicit_missing_repair_kind_projection_exclusion, docs/OUTPUT_SCHEMA.md:blocked_state_routes, EffortlessMetrics/ripr-swarm#725`
- Receipt validation: pass

### Real Repair Attempt `limitation_backlog_legacy_route_hardening_improved`

- Canonical gap ID: `gap:limitation-backlog-legacy-route-hardening`
- Packet ID: `limitation:activation-boundary-input-unresolved:local-computed`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `readiness top_limitation_routes assertions that legacy limitation backlog packets fill standard non-claims, why_not_actionable, unlock_condition, and explicit unknown evidence class values`
- Verify command: `cargo test -p xtask ripr_swarm_readiness_hardens_legacy_limitation_backlog_packets -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Hardened readiness projection for older or partial limitation backlog packets so top limitation routes preserve analyzer-route guidance instead of emitting empty or repair-looking rows.
- Evidence movement: improved: legacy limitation backlog routes now receive standard non-claims, fallback non-actionability text, fallback unlock conditions, and explicit unknown evidence-class values while staying non-actionable.
- Operator note: This dogfoods negative-capability routing. The improvement makes analyzer backlog safer to inspect, but it deliberately does not create public repair packets or swarm-ready work.
- Must not change: `do not mark limitation backlog packets public_projection_eligible, do not mark limitation backlog packets swarm-ready, do not emit verify or receipt commands for limitation backlog items, do not change public badge semantics, do not change CI gate defaults, do not add provider calls or mutation execution`
- Raw evidence refs: `xtask/src/main.rs::tests::ripr_swarm_readiness_hardens_legacy_limitation_backlog_packets, xtask/src/main.rs:ripr_swarm_limitation_route_non_claims, docs/OUTPUT_SCHEMA.md:top_limitation_routes, docs/specs/RIPR-SPEC-0057-ripr-swarm-repair-loop.md, EffortlessMetrics/ripr-swarm#731`
- Receipt validation: pass

### Real Repair Attempt `readiness_top_next_action_resolved`

- Canonical gap ID: `gap:readiness-top-next-action-missing`
- Packet ID: `readiness-top-next-action-001`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `readiness JSON and Markdown assertions that top_next_action equals next_actions[0]`
- Verify command: `cargo xtask check-pr`
- Verify result: `pass`
- Receipt command: `cargo xtask ripr-swarm readiness`
- Receipt path: `target/ripr/reports/swarm-readiness.json`
- Receipt state: `receipt_movement_resolved`
- Before/after: `actionable` -> `resolved`
- Attempted repair: Added a stable top_next_action projection for downstream surfaces to consume directly.
- Evidence movement: The missing readiness projection gap was resolved by preserving next_actions[0] as a named contract field.
- Operator note: This makes LSP/PR/CI/badge projection thinner because they do not need to reinterpret raw findings for the first useful action.
- Must not change: `next_actions ranking source, raw finding interpretation, default advisory posture`
- Raw evidence refs: `docs/OUTPUT_SCHEMA.md:swarm-readiness, xtask/src/main.rs:ripr_swarm_readiness`
- Receipt validation: pass

### Real Repair Attempt `projection_alignment_receipt_improved`

- Canonical gap ID: `gap:surface-projection-alignment-receipt-missing`
- Packet ID: `surface-projection-alignment-001`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `dogfood fixture proving one receipt-backed packet preserves canonical identity through attempt-ledger and readiness`
- Verify command: `cargo xtask check-pr`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/surface-projection-alignment/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `actionable` -> `actionable`
- Attempted repair: Added a surface-projection alignment dogfood corpus and dogfood report section.
- Evidence movement: The packet identity, repair kind, verify command, receipt command/state, outcome, and readiness top_next_action are now pinned together.
- Operator note: This improved proof that downstream surfaces are thin consumers of canonical state, but it deliberately did not change those surfaces.
- Must not change: `badge endpoint behavior, LSP/editor UX behavior, PR comment publishing, CI gate behavior`
- Raw evidence refs: `fixtures/surface-projection-alignment/corpus.json, docs/specs/RIPR-SPEC-0057-ripr-swarm-repair-loop.md`
- Receipt validation: pass

### Real Repair Attempt `targeted_outcome_left_gap_unchanged`

- Canonical gap ID: `gap:unchanged`
- Packet ID: `evidence-unchanged-from-targeted-outcome`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `targeted-test outcome fixture with unchanged before/after grip evidence`
- Verify command: `cargo xtask actionable-gap-outcomes --actionable-gaps target/ripr/reports/0.7-dogfood-outcomes/evidence_unchanged_from_targeted_outcome.actionable-gaps.json --agent-receipt target/ripr/reports/0.7-dogfood-outcomes/evidence_unchanged_from_targeted_outcome.agent-receipt.json --targeted-test-outcome target/ripr/reports/0.7-dogfood-outcomes/evidence_unchanged_from_targeted_outcome.targeted-test-outcome.json`
- Verify result: `pass`
- Receipt command: `cargo xtask actionable-gap-outcomes`
- Receipt path: `docs/handoffs/2026-05-20-0.7-swarm-repair-loop-dogfood.md`
- Receipt state: `receipt_missing`
- Before/after: `actionable` -> `actionable`
- Attempted repair: Exercised the outcome join where targeted movement exists but the static evidence did not improve.
- Evidence movement: Outcome reporting kept the unchanged attempt visible and did not mark the gap resolved.
- Operator note: This is intentionally not a success case; it proves failed or insufficient attempts do not disappear.
- Must not change: `resolved classification, receipt-missing visibility, orphan receipt handling`
- Raw evidence refs: `fixtures/actionable-gap-outcomes-corpus/corpus.json:evidence_unchanged_from_targeted_outcome, docs/handoffs/2026-05-20-0.7-swarm-repair-loop-dogfood.md`
- Receipt validation: pass

### Real Repair Attempt `exact_error_variant_guidance_route_improved`

- Canonical gap ID: `gap:route-quality-add-exact-error-variant-unchanged`
- Packet ID: `route-quality:add-exact-error-variant:unchanged`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `packet guidance for add_exact_error_variant that names the concrete error variant and rejects broad Err/is_err/expect_err evidence`
- Verify command: `cargo xtask check-pr`
- Verify result: `pass`
- Receipt command: `cargo xtask ripr-swarm readiness`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `actionable` -> `actionable`
- Attempted repair: Sharpened add_exact_error_variant handoff guidance so the packet tells agents to extract the error, match the exact variant or payload, and avoid broad error-only checks.
- Evidence movement: The failed unchanged add_exact_error_variant route now has a receipt-backed guidance improvement attempt without reclassifying the original unchanged repair as successful.
- Operator note: This records route-learning progress. It does not hide the unchanged repair attempt or claim the original gap was resolved.
- Must not change: `unchanged attempt visibility, public badge semantics, CI gate defaults, provider integration, mutation execution`
- Raw evidence refs: `crates/ripr/src/output/agent_seam_packets.rs:packet_v2_carries_exact_error_variant_guidance_for_error_seams, crates/ripr/src/output/agent_seam_packets.rs:error_variant_assertion_shape_fallback_names_exact_variant_without_nested_comments, target/ripr/reports/swarm-readiness.json:route-quality:add-exact-error-variant:unchanged, EffortlessMetrics/ripr-swarm#582`
- Receipt validation: pass

### Real Repair Attempt `targeted_outcome_exact_error_variant_receipt_improved`

- Canonical gap ID: `gap:unchanged`
- Packet ID: `evidence-unchanged-from-targeted-outcome`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `exact error-variant observer guidance that names the concrete variant and rejects broad Err/is_err/expect_err evidence`
- Verify command: `cargo test -p ripr exact_error_variant --lib`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `actionable` -> `actionable`
- Attempted repair: Re-ran the add_exact_error_variant route through the bounded exact-error-variant guidance tests instead of treating the original unchanged targeted outcome as the latest route result.
- Evidence movement: improved: the add_exact_error_variant route now has a focused verify command and durable dogfood receipt while the historical unchanged attempt remains visible.
- Operator note: This records route-learning progress for the failed exact-error-variant route; it does not claim the original static gap was resolved or change public badge or gate semantics.
- Must not change: `unchanged attempt visibility, resolved classification, public badge semantics, CI gate defaults, provider integration, mutation execution`
- Raw evidence refs: `fixtures/real-repair-attempts/corpus.json:targeted_outcome_left_gap_unchanged, crates/ripr/src/output/agent_seam_packets.rs:packet_v2_carries_exact_error_variant_guidance_for_error_seams, crates/ripr/src/output/agent_seam_packets.rs:error_variant_assertion_shape_fallback_names_exact_variant_without_nested_comments, target/ripr/reports/swarm-readiness.json:route-quality:add-exact-error-variant:unchanged`
- Receipt validation: pass

### Real Repair Attempt `dry_run_receipt_command_missing_named`

- Canonical gap ID: `gap:report-config-label`
- Packet ID: `gap:report-config-label`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `report golden observer for rendered configuration label`
- Verify command: `cargo xtask goldens check`
- Verify result: `pass`
- Receipt command: `cargo xtask receipts check`
- Receipt path: `missing`
- Receipt state: `receipt_missing`
- Before/after: `actionable` -> `actionable`
- Attempted repair: Dry-ran a ready packet and checked verify/receipt command separation without fabricating missing receipts.
- Evidence movement: The verify command passed, while the receipt command named required missing receipt files.
- Operator note: This records a non-closed loop explicitly; a missing receipt remains a blocker rather than hidden success.
- Missing receipt reason: receipt command reported required local receipts were absent
- Must not change: `receipt fabrication, source files, generated tests, provider calls`
- Raw evidence refs: `fixtures/swarm-plan-packet-corpus/corpus.json:high_confidence_boundary_assertion_packet, docs/handoffs/2026-05-20-0.7-swarm-repair-loop-dogfood.md`
- Receipt validation: pass

### Real Repair Attempt `report_config_label_output_observer_receipt_improved`

- Canonical gap ID: `gap:report-config-label`
- Packet ID: `gap:report-config-label`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `report golden observer for rendered configuration label with a durable dogfood receipt`
- Verify command: `cargo xtask goldens check`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `actionable` -> `actionable`
- Attempted repair: Recorded the report-config-label output observer route with the same bounded golden verification and a durable dogfood receipt path.
- Evidence movement: improved: the add_output_observer route now has a receipt-backed follow-up instead of only an attempted_no_receipt dry run.
- Operator note: The original missing-receipt dry run stays in the corpus as historical evidence; this follow-up only proves the route can be receipted, not that public badge or gate semantics changed.
- Must not change: `receipt fabrication, source files, public badge semantics, CI gate defaults, provider integration, mutation execution`
- Raw evidence refs: `fixtures/real-repair-attempts/corpus.json:dry_run_receipt_command_missing_named, fixtures/swarm-plan-packet-corpus/corpus.json:output_observer_packet, docs/handoffs/2026-05-20-0.7-swarm-repair-loop-dogfood.md`
- Receipt validation: pass

### Real Repair Attempt `local_computed_boundary_limitation_route_improved`

- Canonical gap ID: `gap:activation-boundary-input-unresolved-local-computed`
- Packet ID: `limitation:activation-boundary-input-unresolved:local-computed-boundary-operand-resolution`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `fixture assertions that local/computed boundary operands remain named static limitations and do not emit add_boundary_assertion packets`
- Verify command: `cargo test -p xtask lane1_evidence_quality_failure_corpus_is_valid -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask check-fixture-contracts`
- Receipt path: `fixtures/boundary_gap/expected/evidence-quality-failures/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Pinned local/computed boundary operand cases in evidence-quality and swarm-plan corpora so the analyzer backlog route stays precise and non-actionable.
- Evidence movement: The local/computed activation boundary route now has fixture-backed proof that RIPR names the limitation instead of inventing candidate values or public repair packets.
- Operator note: This is a successful negative-capability repair-loop attempt: it improved route trust by making unsafe actionability stay blocked.
- Must not change: `public projection eligibility for static limitations, add_boundary_assertion promotion, candidate value inference, production behavior`
- Raw evidence refs: `fixtures/boundary_gap/expected/evidence-quality-failures/corpus.json:local_computed_boundary_operands_remain_limitation, fixtures/swarm-plan-packet-corpus/corpus.json:local_computed_boundary_static_limitation_packet, .ripr/traceability.toml:RIPR-SPEC-0061, metrics/capabilities.toml:lane1_canonical_actionability_contract`
- Receipt validation: pass

### Real Repair Attempt `parameter_field_boundary_operand_route_improved`

- Canonical gap ID: `gap:activation-boundary-input-unresolved-parameter-field-projection`
- Packet ID: `limitation:activation_boundary_input_unresolved:parameter_field_projection:analysis-local-computed-boundary-operand-resolution`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `fixture-backed direct owner call where predicate operands are parameter field projections and the related test passes same-test struct values with literal fields`
- Verify command: `cargo test -p ripr parameter_field_operands --lib -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Taught the boundary activation path to resolve simple parameter field operands, such as left.value == right.value, through same-test struct field bindings when direct owner-call arguments are visible.
- Evidence movement: improved: a fixture-backed predicate boundary can now observe concrete activation values and clear the equality-boundary missing discriminator when both parameter field projections resolve to equal literal fields.
- Operator note: This is an analyzer-route dogfood slice. Opaque field projections, computed locals, and iterator-derived operands remain blocked instead of being promoted to public repair packets.
- Must not change: `do not invent values for opaque struct arguments, do not resolve arbitrary computed locals, do not resolve iterator-derived boundary operands, do not emit public repair packets from static-limitation backlog, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_boundary_owner_call_when_parameter_field_operands_resolve_then_activation_is_yes, crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_boundary_owner_call_when_parameter_field_operands_are_opaque_then_activation_stays_static_limitation, crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_boundary_owner_call_when_input_operand_is_computed_local_then_activation_stays_static_limitation, crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_boundary_owner_call_when_input_operand_is_iterator_local_then_activation_is_static_limitation, metrics/capabilities.toml:lane1_canonical_actionability_contract`
- Receipt validation: pass

### Real Repair Attempt `closure_boundary_operand_route_improved`

- Canonical gap ID: `gap:activation-boundary-input-unresolved-closure-derived`
- Packet ID: `limitation:activation_boundary_input_unresolved:closure_derived:analysis-closure-boundary-operand-resolution`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `fixture-backed predicate boundary where the unresolved operand is bound by a closure parameter, such as function.id == owner`
- Verify command: `cargo test -p ripr closure_boundary_operand --lib -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Split closure-derived boundary operands out of the generic local/computed boundary limitation route while preserving non-actionable status.
- Evidence movement: improved: closure-local boundary operands now route to analysis/closure-boundary-operand-resolution instead of blending with local/computed or iterator-derived boundary work.
- Operator note: This is analyzer-route dogfood. It improves work allocation only; it does not emit a public repair packet, infer candidate values, or move badge/gate counters.
- Must not change: `do not invent values for closure parameters, do not route closure operands as iterator-derived operands, do not emit public repair packets from static-limitation backlog, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_boundary_owner_call_when_input_operand_is_closure_local_then_activation_is_static_limitation, crates/ripr/src/analysis/test_grip_evidence.rs::tests::closure_boundary_operand_route_ignores_comment_only_closure_pattern, crates/ripr/src/output/evidence_record.rs::tests::evidence_record_routes_closure_boundary_operands_to_closure_limitation, target/ripr/reports/swarm-readiness.json:analysis/local-computed-boundary-operand-resolution`
- Receipt validation: pass

### Real Repair Attempt `same_file_method_chain_owner_call_route_split`

- Canonical gap ID: `gap:activation-owner-call-absent-same-file-method-chain`
- Packet ID: `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_method_chain_missing_owner_call:analysis-same-file-method-chain-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `same-file call-presence limitation where the unresolved owner call is a method-chain expression such as description.clone()`
- Verify command: `cargo test -p ripr evidence_record_routes_same_file_method_chain_owner_call_absence_limitation --lib -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Split same-file method-chain call-presence owner-call limitations into a dedicated repair route while preserving non-actionable status.
- Evidence movement: improved: method-chain same-file owner-call limitations now route to analysis/same-file-method-chain-owner-call-tracing instead of blending with the generic same-file owner-call route.
- Operator note: This is analyzer-route dogfood. It improves work allocation only; it does not prove owner-call reachability, emit public repair packets, or move badge/gate counters.
- Must not change: `do not treat same-file proximity alone as owner-call activation, do not promote method-chain limitations to public repair packets, do not invent observed values for call-presence arguments, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/output/evidence_record.rs::tests::evidence_record_routes_same_file_method_chain_owner_call_absence_limitation, target/ripr/reports/lane1-evidence-audit.json:analysis/same-file-owner-call-tracing, target/ripr/reports/swarm-plan.json:same_file_only_call_presence_method_chain_missing_owner_call`
- Receipt validation: pass

### Real Repair Attempt `same_file_method_chain_backlog_projection_route_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-same-file-method-chain-backlog-projection`
- Packet ID: `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_method_chain_missing_owner_call:analysis-same-file-method-chain-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `swarm plan and readiness static-limitation backlog projection for same-file call-presence method-chain owner-call limitations`
- Verify command: `cargo test -p xtask lane1_static_limitation_backlog_splits_same_file_owner_call_absence_by_expression_shape -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Project same-file method-chain static-limitation backlog packets through the specialized report route instead of the generic same-file owner-call route.
- Evidence movement: improved: fresh plan/readiness output now shows same_file_only_call_presence_method_chain_missing_owner_call under analysis/same-file-method-chain-owner-call-tracing, while same-file function-call limitations remain under analysis/same-file-owner-call-tracing.
- Operator note: This is report-route dogfood. It improves work allocation for analyzer backlog only; it does not make the limitation actionable, emit repair packets, or change badge/gate semantics.
- Must not change: `do not treat same-file proximity alone as owner-call activation, do not promote method-chain limitations to public repair packets, do not route dotted non-call text as method-chain call evidence, do not change gate or badge semantics`
- Raw evidence refs: `xtask/src/main.rs::tests::lane1_static_limitation_backlog_splits_same_file_owner_call_absence_by_expression_shape, target/ripr/reports/swarm-plan.md:same_file_only_call_presence_method_chain_missing_owner_call, target/ripr/reports/swarm-readiness.json:analysis/same-file-method-chain-owner-call-tracing`
- Receipt validation: pass

### Real Repair Attempt `same_file_method_chain_owner_call_helper_tests_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-same-file-method-chain-helper-tests`
- Packet ID: `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_method_chain_missing_owner_call:analysis-same-file-method-chain-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `same-file call-presence method-chain owner-call fixtures that exercise source-shaped helper paths through canonical gap discriminator text and activation alias helper functions`
- Verify command: `cargo test -p ripr same_file_method_chain_owner --lib -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `RIPR_LANE1_EVIDENCE_AUDIT_TIMEOUT_MS=240000 cargo xtask lane1-evidence-audit && cargo xtask ripr-swarm plan --top 10 && cargo xtask actionable-gap-outcomes && cargo xtask ripr-swarm readiness`
- Receipt path: `target/ripr/reports/lane1-evidence-audit.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Added source-shaped analyzer tests for same-file method-chain owner-call helper paths without broadening owner-call inference.
- Evidence movement: improved: added source-shaped analyzer guard coverage for same-file method-chain helper owner-call paths and if-let boundary operand aliases without claiming sampled-count movement.
- Operator note: This is analyzer dogfood. It improves regression coverage only; remaining same-file method-chain cases stay non-actionable until direct or helper owner calls are observed.
- Must not change: `do not treat same-file proximity alone as owner-call activation, do not promote method-chain limitations to public repair packets, do not infer lazy fallback closure calls as unconditional owner calls, do not invent observed values for call-presence arguments, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/analysis/classify/activation.rs::tests::same_file_method_chain_owner_call_tracing_covers_activation_alias_helpers, crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_same_file_method_chain_owner_through_helpers_then_activation_is_yes, target/ripr/reports/lane1-evidence-audit.json:finding_alignment_static_limitation_total, target/ripr/reports/swarm-plan.json:analysis/same-file-method-chain-owner-call-tracing`
- Receipt validation: pass

### Real Repair Attempt `activation_body_line_helper_owner_call_tests_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-same-file-body-line-helpers`
- Packet ID: `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_method_chain_missing_owner_call:analysis-same-file-method-chain-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `activation alias helper tests that directly call body_contains_match_parameter, body_contains_wrapper_pattern, and body_contains_direct_local_alias so body.lines() helper seams are observed without broadening short-circuit helper inference`
- Verify command: `cargo test -p ripr same_file_method_chain_owner_call_tracing_covers_activation_alias_helpers --lib -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `RIPR_LANE1_EVIDENCE_AUDIT_TIMEOUT_MS=240000 cargo xtask lane1-evidence-audit && cargo xtask ripr-swarm plan --top 10 && cargo xtask actionable-gap-outcomes && cargo xtask ripr-swarm attempt-ledger && cargo xtask ripr-swarm readiness`
- Receipt path: `target/ripr/reports/lane1-evidence-audit.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Added direct source-shaped activation helper calls for body.lines() scanner helpers instead of teaching the analyzer to treat short-circuit helper calls as unconditional.
- Evidence movement: improved: sampled 5,000-item Lane 1 audit moved finding_alignment_already_observed_total 3544 -> 3559 and finding_alignment_static_limitation_total 1456 -> 1441; analysis/same-file-method-chain-owner-call-tracing moved 196 -> 190 while remaining method-chain backlog stayed non-actionable analyzer work.
- Operator note: This is analyzer dogfood. It improves owner-call observability for private helper scanners only; it does not promote static limitations to public repair packets or change badge/gate semantics.
- Must not change: `do not treat same-file proximity alone as owner-call activation, do not promote method-chain limitations to public repair packets, do not infer lazy fallback or short-circuit helper calls as unconditional owner calls, do not invent observed values for call-presence arguments, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/analysis/classify/activation.rs::tests::same_file_method_chain_owner_call_tracing_covers_activation_alias_helpers, target/ripr/reports/lane1-evidence-audit.json:gap:52d442acc71b3b6c, target/ripr/reports/lane1-evidence-audit.json:gap:9d436b0f8e639bf9, target/ripr/reports/lane1-evidence-audit.json:gap:629800064f60d217, target/ripr/reports/swarm-readiness.json:analysis/same-file-method-chain-owner-call-tracing`
- Receipt validation: pass

### Real Repair Attempt `same_file_receiver_method_backlog_route_split`

- Canonical gap ID: `gap:activation-owner-call-absent-same-file-receiver-method`
- Packet ID: `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_receiver_method_missing_owner_call:analysis-same-file-receiver-method-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `same-file call-presence limitations where the unresolved expression is receiver-method text such as body.lines() rather than observed owner-call evidence`
- Verify command: `cargo test -p xtask lane1_static_limitation_backlog_splits_same_file_owner_call_absence_by_expression_shape -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Split remaining same-file receiver-method call-presence owner-call limitations away from stale method-chain wording while preserving non-actionable status.
- Evidence movement: improved: same-file receiver-method call-presence backlog packets now route to analysis/same-file-receiver-method-owner-call-tracing and include a non-claim that receiver method text is not owner-call evidence.
- Operator note: This is report-route dogfood. It improves analyzer work allocation only; it does not prove owner-call reachability, emit public repair packets, or move badge/gate counters.
- Must not change: `do not treat same-file proximity alone as owner-call activation, do not treat receiver method text as owner-call evidence, do not infer arbitrary receiver value flow, do not emit public repair packets from static-limitation backlog, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/output/evidence_record.rs::tests::evidence_record_routes_same_file_receiver_method_owner_call_absence_limitation, xtask/src/main.rs::tests::lane1_static_limitation_backlog_splits_same_file_owner_call_absence_by_expression_shape, target/ripr/reports/lane1-evidence-audit.json:gap:52d442acc71b3b6c, target/ripr/reports/swarm-readiness.json:analysis/same-file-receiver-method-owner-call-tracing`
- Receipt validation: pass

### Real Repair Attempt `same_file_dotted_non_call_subroute_guard`

- Canonical gap ID: `gap:activation-owner-call-absent-same-file-dotted-non-call`
- Packet ID: `limitation:activation_owner_call_absent_same_file_only:same_file_only_missing_owner_call:analysis-same-file-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `same-file call-presence limitation where dotted non-call text such as description.value must not be labeled as a method-chain call`
- Verify command: `cargo test -p xtask lane1_static_limitation_backlog_splits_same_file_owner_call_absence_by_expression_shape -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Aligned xtask planning subroutes with production route hardening so dotted field text stays on the generic same-file owner-call route.
- Evidence movement: improved: same-file dotted non-call expressions now project as same_file_only_missing_owner_call instead of method-chain owner-call work.
- Operator note: This is planning-surface dogfood. It improves route labels only; it does not infer owner-call reachability, emit public repair packets, or move badge/gate counters.
- Must not change: `do not treat dotted field access as a method-chain call without a call parenthesis, do not promote same-file owner-call limitations to public repair packets, do not infer observed call arguments from dotted field text, do not change gate or badge semantics`
- Raw evidence refs: `xtask/src/main.rs::tests::lane1_static_limitation_backlog_splits_same_file_owner_call_absence_by_expression_shape, target/ripr/reports/actionable-gaps.json:same_file_only_missing_owner_call, target/ripr/reports/lane1-evidence-audit.json:analysis/same-file-owner-call-tracing`
- Receipt validation: pass

### Real Repair Attempt `assertion_target_return_value_owner_call_route_split`

- Canonical gap ID: `gap:activation-owner-call-absent-assertion-target-return-value`
- Packet ID: `limitation:activation_owner_call_absent_assertion_target_affinity:assertion_target_affinity_return_value_missing_owner_call:analysis-assertion-target-return-value-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `assertion-target-affinity return-value limitation where the related test mentions the returned value but no direct or helper owner call is observed`
- Verify command: `cargo test -p ripr evidence_record_routes_assertion_target_return_value_owner_call_absence_limitation --lib -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Split assertion-target return-value owner-call limitations into a dedicated analyzer route while preserving non-actionable status.
- Evidence movement: improved: return-value assertion-target owner-call limitations now route to analysis/assertion-target-return-value-owner-call-tracing instead of blending with the generic assertion-target owner-call route.
- Operator note: This is analyzer-route dogfood. It improves work allocation only; it does not prove owner-call reachability, emit public repair packets, or move badge/gate counters.
- Must not change: `do not treat assertion-target affinity alone as owner-call activation, do not promote return-value assertion-target limitations to public repair packets, do not invent observed values for return expressions, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/output/evidence_record.rs::tests::evidence_record_routes_assertion_target_return_value_owner_call_absence_limitation, target/ripr/reports/swarm-readiness.json:analysis/assertion-target-affinity-owner-call-tracing, target/ripr/reports/lane1-evidence-audit.json:assertion_target_affinity_return_value_missing_owner_call`
- Receipt validation: pass

### Real Repair Attempt `call_presence_target_affinity_limitation_route_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-call-presence-target-affinity`
- Packet ID: `limitation:activation_owner_call_absent_call_presence_target_affinity:analysis-call-presence-target-affinity-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `evidence-record and benchmark fixture assertions that call-presence target-token affinity remains a named static limitation and does not satisfy activation`
- Verify command: `cargo test -p ripr evidence_record_splits_call_presence_target_affinity_owner_call_absence_limitation -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Split call-presence target-token affinity out of the broader assertion-target affinity owner-call limitation route while preserving non-actionable status.
- Evidence movement: The top sampled limitation route now points to a precise call-presence analyzer backlog item instead of blending target-token affinity with other assertion-target affinity.
- Operator note: This is a successful negative-capability repair-loop attempt: it improved analyzer backlog precision without emitting a public repair packet or treating target-token assertions as owner-call activation.
- Must not change: `public projection eligibility for static limitations, activation satisfied by call-target assertion text, direct owner-call or helper-call requirements, public badge semantics`
- Raw evidence refs: `fixtures/evidence-quality-benchmark/corpus.json:call_presence_specific_call_target_affinity_stays_named_limitation, crates/ripr/src/output/evidence_record.rs:evidence_record_splits_call_presence_target_affinity_owner_call_absence_limitation, docs/specs/RIPR-SPEC-0031-lane1-evidence-quality-audit.md, docs/OUTPUT_SCHEMA.md:activation_owner_call_absent_call_presence_target_affinity`
- Receipt validation: pass

### Real Repair Attempt `assertion_target_err_wrapped_helper_owner_call_route_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-assertion-target-affinity`
- Packet ID: `limitation:activation-owner-call-absent-assertion-target-affinity:assertion-target-affinity-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `fixture-backed test-local one-hop helper where an Err(...) wrapper directly contains the production owner call for value-insensitive call-presence activation`
- Verify command: `cargo test -p ripr given_call_presence_when_test_local_helper_wraps_owner_call_in_err_then_activation_is_yes --lib -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Taught assertion-target affinity owner-call tracing to recognize a bounded test-local helper that wraps the production owner call in `Err(...)`, while keeping arbitrary Result wrappers and affinity-only evidence limited.
- Evidence movement: improved: the analyzer route has fixture and benchmark proof that Err-wrapped helper owner calls can satisfy value-insensitive activation without inventing observed values or promoting assertion-target affinity alone to public repair work.
- Operator note: This is analyzer-route dogfood for an already-merged fixture-backed route. It records movement without changing actionability, gates, badges, or the remaining assertion-target affinity backlog.
- Must not change: `do not treat arbitrary Result wrappers as multi-hop helper tracing, do not treat assertion-target affinity alone as activation without a helper or direct owner call, do not invent observed values for call-presence arguments, do not apply this route to predicate-boundary value checks, do not emit public repair packets from static-limitation backlog, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_helper_wraps_owner_call_in_err_then_activation_is_yes, fixtures/evidence-quality-benchmark/corpus.json:call_presence_test_local_err_helper_owner_call_activation_values_not_required, docs/specs/RIPR-SPEC-0035-evidence-quality-benchmark-corpus.md, EffortlessMetrics/ripr-swarm#535`
- Receipt validation: pass

### Real Repair Attempt `same_file_owner_call_route_samples_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-same-file-only`
- Packet ID: `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `readiness top_limitation_routes assertions that every top limitation repair route keeps an inspectable sample backlog packet even when high-volume subroutes consume the bounded packet list`
- Verify command: `cargo test -p xtask lane1_static_limitation_backlog_keeps_samples_for_each_top_repair_route -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask ripr-swarm readiness`
- Receipt path: `target/ripr/reports/swarm-readiness.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Preserved representative limitation backlog packets for each top packet-backed repair route so the same-file owner-call limitation route remains inspectable instead of collapsing to unknown sample context, while keeping sampled-run diagnostics in runtime status rather than packet-backed route rows.
- Evidence movement: The sampled readiness report now shows the top analysis/same-file-owner-call-tracing route with packet_id, limitation category, evidence class, and sample canonical gaps while preserving non-actionable status.
- Operator note: This is a successful negative-capability dogfood attempt. It made the top analyzer backlog route operational without promoting same-file proximity into repair work.
- Must not change: `do not treat same-file proximity as owner-call activation, do not emit public repair packets from static-limitation backlog, do not make limitation backlog packets swarm-ready, do not invent exact candidate values, do not change public badge or CI semantics`
- Raw evidence refs: `xtask/src/main.rs::tests::lane1_static_limitation_backlog_keeps_samples_for_each_top_repair_route, target/ripr/reports/swarm-readiness.json:analysis/same-file-owner-call-tracing, docs/OUTPUT_SCHEMA.md:top_limitation_routes, EffortlessMetrics/ripr-swarm#746`
- Receipt validation: pass

### Real Repair Attempt `same_file_test_local_helper_owner_call_route_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-same-file-only`
- Packet ID: `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `fixture-backed test-local helper graph where a test-local helper calls one same-file helper before the owner for value-insensitive call-presence activation`
- Verify command: `cargo test -p ripr given_call_presence_when_test_local_two_hop_helper_calls_owner_then_activation_is_yes --lib`
- Verify result: `pass`
- Receipt command: `cargo xtask ripr-swarm readiness`
- Receipt path: `target/ripr/reports/swarm-readiness.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Taught same-file owner-call tracing to recognize a bounded test-local helper path through one same-file helper before the owner, while preserving non-actionable status for the remaining same-file backlog.
- Evidence movement: The sampled same-file owner-call limitation route moved from 644 to 640 signals, and the bounded test-local helper sub-shape now records activation without inventing observed call values.
- Operator note: This is an analyzer-routing improvement only. It reduces a top limitation route but does not promote same-file proximity, arbitrary helper depth, or synthetic values into public repair work.
- Must not change: `do not treat same-file proximity alone as owner-call activation, do not follow arbitrary multi-hop helper graphs, do not invent observed values for call-presence arguments, do not emit public repair packets from static-limitation backlog, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_two_hop_helper_calls_owner_then_activation_is_yes, docs/specs/RIPR-SPEC-0035-evidence-quality-benchmark-corpus.md, target/ripr/reports/swarm-readiness.json:analysis/same-file-owner-call-tracing, EffortlessMetrics/ripr-swarm#751`
- Receipt validation: pass

### Real Repair Attempt `same_file_three_hop_helper_owner_call_route_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-same-file-only`
- Packet ID: `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `fixture-backed same-file unit-test helper graph where a test calls a public helper, that helper delegates through two same-file helpers, and the final helper calls the owner for value-insensitive call-presence activation`
- Verify command: `cargo test -p ripr given_call_presence_when_same_file_three_hop_helper_reaches_owner_then_activation_is_yes --lib -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask ripr-swarm readiness`
- Receipt path: `target/ripr/reports/swarm-readiness.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Raised same-file helper-owner call tracing from two bounded helper edges to three and pinned a same-file unit-test fixture for activation-style helper chains, while preserving the conditional, short-circuit, dynamic-receiver, and unknown-chain limitations.
- Evidence movement: improved: the analyzer route now has positive fixture and benchmark proof for bounded three-hop same-file helper owner-call tracing. The sampled readiness route count stayed at 438 signals, so this receipt does not claim sampled backlog burn-down.
- Operator note: This is analyzer-route dogfood. It extends a bounded same-file helper graph only far enough to cover the activation helper-chain shape, while keeping same-file proximity, conditional helper calls, short-circuiting calls, dynamic receivers, and arbitrary helper graphs non-actionable.
- Must not change: `do not treat same-file proximity alone as owner-call activation, do not follow arbitrary helper graphs beyond the bounded three-hop same-file route, do not accept conditional helper calls or short-circuiting expressions as guaranteed owner-call proof, do not invent observed values for call-presence arguments, do not apply this route to predicate-boundary value checks, do not emit public repair packets from static-limitation backlog, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_same_file_three_hop_helper_reaches_owner_then_activation_is_yes, crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_helper_conditionally_calls_owner_then_activation_stays_unknown, crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_format_args_macro_short_circuits_owner_then_activation_stays_unknown, fixtures/evidence-quality-benchmark/corpus.json:call_presence_same_file_three_hop_helper_owner_call_activation_values_not_required, target/ripr/reports/swarm-readiness.json:analysis/same-file-owner-call-tracing`
- Receipt validation: pass

### Real Repair Attempt `same_file_method_owner_call_route_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-same-file-only`
- Packet ID: `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_method_chain_missing_owner_call:analysis-same-file-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `fixture-backed same-file wrapper where a simple identifier receiver method call reaches the owner for value-insensitive call-presence activation, while a dynamic receiver method chain remains limited`
- Verify command: `cargo test -p ripr same_file_wrapper --lib -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Taught same-file owner-call tracing to recognize simple receiver method calls such as `pipeline.render_pipeline(...)` while keeping dynamic receivers such as `factory().render_pipeline(...)` limited.
- Evidence movement: improved: the analyzer route now has positive and negative fixture proof for same-file receiver-method activation without inventing observed values or promoting the remaining same-file backlog to public repair work.
- Operator note: This is analyzer-route dogfood. It sharpens a top static-limitation route but does not claim sampled route-count movement, public packet eligibility, badge movement, gate movement, or parser support promotion.
- Must not change: `do not treat same-file proximity alone as owner-call activation, do not accept dynamic method receivers as direct owner-call proof, do not invent observed values for call-presence arguments, do not emit public repair packets from static-limitation backlog, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_same_file_wrapper_calls_owner_method_then_activation_is_yes, crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_same_file_wrapper_uses_dynamic_method_receiver_then_activation_stays_unknown, target/ripr/reports/swarm-readiness.json:analysis/same-file-owner-call-tracing, EffortlessMetrics/ripr-swarm#761`
- Receipt validation: pass

### Real Repair Attempt `same_file_assertion_helper_owner_call_route_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-same-file-only`
- Packet ID: `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `fixture-backed test-local assertion helper where a known oracle macro directly wraps the owner call for value-insensitive call-presence activation, while an unknown macro wrapper remains limited`
- Verify command: `cargo test -p ripr test_local_assertion_helper --lib -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Taught same-file owner-call tracing to recognize bounded test-local assertion helpers such as `assert_eq!(render_pipeline(...), expected)` while keeping unknown macro wrappers such as `trace_value!(render_pipeline(...))` limited.
- Evidence movement: improved: the analyzer route now has positive and negative fixture proof for assertion-helper owner-call activation without inventing observed values or promoting the remaining same-file backlog to public repair work.
- Operator note: This is analyzer-route dogfood. It records movement for an already-merged fixture-backed route and preserves the static-limitation boundary for unknown macros, arbitrary helper wrappers, and the remaining same-file backlog.
- Must not change: `do not treat same-file proximity alone as owner-call activation, do not accept unknown macro wrappers as direct owner-call proof, do not invent observed values for call-presence arguments, do not emit public repair packets from static-limitation backlog, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_assertion_helper_calls_owner_then_activation_is_yes, crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_helper_wraps_owner_call_in_unknown_macro_then_activation_stays_unknown, fixtures/evidence-quality-benchmark/corpus.json:call_presence_assertion_helper_owner_call_activation_values_not_required, EffortlessMetrics/ripr-swarm#765`
- Receipt validation: pass

### Real Repair Attempt `same_file_helper_owner_call_route_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-same-file-only`
- Packet ID: `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `fixture-backed same-file one-hop helper where a unit test calls a helper in the same file and that helper directly calls the production owner for value-insensitive call-presence activation`
- Verify command: `cargo test -p ripr same_file_test_helper_call_counts_as_owner_call_evidence --lib -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Taught compact same-file owner-call context to carry helper-owner evidence when a test calls a same-file helper such as `case_at_threshold()` and that helper directly calls `discounted_total(...)`.
- Evidence movement: improved: the analyzer route now has compact-context proof and benchmark-backed route evidence for one-hop same-file helper activation without inventing observed values or promoting the remaining same-file backlog to public repair work.
- Operator note: This is analyzer-route dogfood for an already-merged fixture-backed route. It records movement for the top sampled same-file owner-call backlog while preserving static-limitation boundaries for proximity-only, multi-hop, and value-invention cases.
- Must not change: `do not treat same-file proximity alone as owner-call activation, do not follow arbitrary multi-hop helper graphs, do not use a helper unless it directly calls the owner, do not invent observed values for call-presence arguments, do not apply this route to predicate-boundary value checks, do not emit public repair packets from static-limitation backlog, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/analysis/test_grip_evidence.rs::tests::same_file_test_helper_call_counts_as_owner_call_evidence, fixtures/evidence-quality-benchmark/corpus.json:call_presence_same_file_helper_owner_call_activation_values_not_required, docs/specs/RIPR-SPEC-0035-evidence-quality-benchmark-corpus.md, EffortlessMetrics/ripr-swarm#773`
- Receipt validation: pass

### Real Repair Attempt `same_file_parent_qualified_owner_call_route_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-same-file-only`
- Packet ID: `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `fixture-backed same-file unit test where an explicit parent-qualified production wrapper call reaches the owner for value-insensitive call-presence activation, while a bare shadowed helper name remains limited`
- Verify command: `cargo test -p ripr same_file_unit --lib -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Taught same-file owner-call tracing to recognize explicit parent-qualified unit-test calls such as `super::exercise_pipeline()` when that production wrapper directly calls the owner, while keeping bare test-local shadow calls limited.
- Evidence movement: improved: the analyzer route now has positive and negative fixture proof for parent-qualified same-file unit-test wrapper activation without inventing observed values or promoting the remaining same-file backlog to public repair work.
- Operator note: This is analyzer-route dogfood. It records movement for an already-merged fixture-backed route and preserves the static-limitation boundary for same-file proximity, bare shadowed helpers, predicate-boundary value checks, and the remaining same-file backlog.
- Must not change: `do not treat same-file proximity alone as owner-call activation, do not inherit production wrapper relations for bare test-local helper shadows, do not invent observed values for call-presence arguments, do not apply this route to predicate-boundary value checks, do not emit public repair packets from static-limitation backlog, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_same_file_unit_parent_qualified_wrapper_calls_owner_then_activation_is_yes, crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_same_file_unit_shadow_calls_wrapper_name_then_activation_stays_unknown, fixtures/evidence-quality-benchmark/corpus.json:call_presence_same_file_unit_parent_qualified_helper_activation_values_not_required, fixtures/evidence-quality-benchmark/corpus.json:call_presence_same_file_unit_shadow_helper_stays_named_limitation, EffortlessMetrics/ripr-swarm#774`
- Receipt validation: pass

### Real Repair Attempt `trim_chain_helper_owner_call_route_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-same-file-only`
- Packet ID: `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `fixture-backed test-local helper where a direct production owner call is followed only by simple trim/to_string result normalization for value-insensitive call-presence activation`
- Verify command: `cargo test -p ripr given_call_presence_when_test_local_helper_trims_owner_call_result_then_activation_is_yes --lib -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Taught same-file owner-call tracing to recognize a bounded test-local helper that directly calls the production owner and applies only simple result normalization such as `.trim().to_string()` while preserving value-insensitive activation boundaries.
- Evidence movement: improved: the analyzer route now has positive and negative fixture proof that simple trim/to_string result normalization after a direct owner call can satisfy value-insensitive activation without accepting arbitrary post-owner method chains.
- Operator note: This is analyzer-route dogfood for an already-merged fixture-backed route. It records movement for the same-file owner-call backlog while preserving static-limitation boundaries for arbitrary method chains, predicate-boundary value checks, invented observed values, and the remaining backlog.
- Must not change: `do not treat same-file proximity alone as owner-call activation, do not accept arbitrary post-owner method chains as direct owner-call proof, do not invent observed values for call-presence arguments, do not apply this route to predicate-boundary value checks, do not emit public repair packets from static-limitation backlog, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_helper_trims_owner_call_result_then_activation_is_yes, crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_helper_uses_unknown_owner_call_chain_then_activation_stays_unknown, fixtures/evidence-quality-benchmark/corpus.json:call_presence_trim_chain_helper_owner_call_activation_values_not_required, docs/specs/RIPR-SPEC-0035-evidence-quality-benchmark-corpus.md, EffortlessMetrics/ripr-swarm#811`
- Receipt validation: pass

### Real Repair Attempt `as_ref_chain_helper_owner_call_route_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-same-file-only`
- Packet ID: `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `fixture-backed test-local helper where a direct production owner call is followed only by a bounded as_ref/unwrap/clone borrow chain for value-insensitive call-presence activation`
- Verify command: `cargo test -p ripr given_call_presence_when_test_local_helper_borrows_owner_call_result_then_activation_is_yes --lib -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Taught same-file owner-call tracing to recognize a bounded test-local helper that directly calls the production owner and applies only `.as_ref().unwrap().clone()` borrow/result handling while preserving value-insensitive activation boundaries.
- Evidence movement: improved: the analyzer route now has positive fixture proof that a bounded as_ref borrow chain after a direct owner call can satisfy value-insensitive activation, while the existing unknown-method chain guard preserves arbitrary post-owner methods as limited.
- Operator note: This is analyzer-route dogfood for a fixture-backed route. It records movement for the same-file owner-call backlog while preserving static-limitation boundaries for arbitrary method chains, predicate-boundary value checks, invented observed values, and the remaining backlog.
- Must not change: `do not treat same-file proximity alone as owner-call activation, do not accept arbitrary post-owner method chains as direct owner-call proof, do not invent observed values for call-presence arguments, do not apply this route to predicate-boundary value checks, do not emit public repair packets from static-limitation backlog, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_helper_borrows_owner_call_result_then_activation_is_yes, crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_helper_uses_unknown_owner_call_chain_then_activation_stays_unknown, fixtures/evidence-quality-benchmark/corpus.json:call_presence_as_ref_chain_helper_owner_call_activation_values_not_required, docs/specs/RIPR-SPEC-0035-evidence-quality-benchmark-corpus.md`
- Receipt validation: pass

### Real Repair Attempt `turbofish_result_helper_owner_call_route_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-same-file-only`
- Packet ID: `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `fixture-backed test-local helper where a direct production owner call is wrapped by an already-supported inert Result constructor using turbofish syntax for value-insensitive call-presence activation`
- Verify command: `cargo test -p ripr given_call_presence_when_test_local_helper_wraps_owner_call_in_result_turbofish_then_activation_is_yes --lib -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Taught same-file owner-call tracing to preserve inert wrapper constructor names when the wrapper uses turbofish syntax, so `Ok::<String, ()>(render_pipeline(...))` keeps the same bounded activation semantics as `Ok(render_pipeline(...))`.
- Evidence movement: improved: the analyzer route now has parser-level and fixture proof that turbofish forms of already-supported inert wrappers can satisfy value-insensitive activation without broadening arbitrary generic wrappers.
- Operator note: This is analyzer-route dogfood for a fixture-backed route. It records movement for the same-file owner-call backlog while preserving static-limitation boundaries for arbitrary generic wrappers, predicate-boundary value checks, invented observed values, and the remaining backlog.
- Must not change: `do not treat same-file proximity alone as owner-call activation, do not accept arbitrary generic wrappers as direct owner-call proof, do not invent observed values for call-presence arguments, do not apply this route to predicate-boundary value checks, do not emit public repair packets from static-limitation backlog, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_helper_wraps_owner_call_in_result_turbofish_then_activation_is_yes, crates/ripr/src/analysis/test_grip_evidence.rs::tests::direct_delegate_wrapper_name_preserves_turbofish_constructor, docs/specs/RIPR-SPEC-0035-evidence-quality-benchmark-corpus.md`
- Receipt validation: pass

### Real Repair Attempt `dbg_macro_helper_owner_call_route_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-same-file-only`
- Packet ID: `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `fixture-backed test-local helper where a direct production owner call is wrapped by the standard non-short-circuiting dbg! macro for value-insensitive call-presence activation`
- Verify command: `cargo test -p ripr dbg_macro --lib -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Taught same-file owner-call tracing to recognize the standard `dbg!` macro when it directly wraps the production owner call, while keeping short-circuiting `dbg!(enabled && owner(...))` limited.
- Evidence movement: improved: the analyzer route now has positive and negative fixture proof that direct dbg!-macro owner-call wrapping can satisfy value-insensitive activation without accepting short-circuiting macro arguments or arbitrary macros.
- Operator note: This is analyzer-route dogfood for a fixture-backed route. It records movement for the same-file owner-call backlog while preserving static-limitation boundaries for short-circuiting dbg! guards, arbitrary macro wrappers, predicate-boundary value checks, invented observed values, and the remaining backlog.
- Must not change: `do not treat same-file proximity alone as owner-call activation, do not accept short-circuiting dbg! arguments as guaranteed owner-call proof, do not accept arbitrary macro wrappers as direct owner-call proof, do not invent observed values for call-presence arguments, do not apply this route to predicate-boundary value checks, do not emit public repair packets from static-limitation backlog, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_dbg_macro_helper_calls_owner_then_activation_is_yes, crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_dbg_macro_short_circuits_owner_then_activation_stays_unknown, docs/specs/RIPR-SPEC-0035-evidence-quality-benchmark-corpus.md`
- Receipt validation: pass

### Real Repair Attempt `format_args_macro_helper_owner_call_route_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-same-file-only`
- Packet ID: `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `fixture-backed test-local helper where a direct production owner call is wrapped by the standard non-short-circuiting format_args! macro for value-insensitive call-presence activation`
- Verify command: `cargo test -p ripr format_args_macro --lib -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Taught same-file owner-call tracing to recognize the standard `format_args!` macro when it directly wraps the production owner call, while keeping short-circuiting `format_args!(..., enabled && owner(...))` limited.
- Evidence movement: improved: the analyzer route now has positive and negative fixture proof that direct format_args!-macro owner-call wrapping can satisfy value-insensitive activation without accepting guarded macro arguments or arbitrary macros.
- Operator note: This is analyzer-route dogfood for a fixture-backed route. It records movement for the same-file owner-call backlog while preserving static-limitation boundaries for short-circuiting format_args! guards, arbitrary macro wrappers, predicate-boundary value checks, invented observed values, and the remaining backlog.
- Must not change: `do not treat same-file proximity alone as owner-call activation, do not accept short-circuiting format_args! arguments as guaranteed owner-call proof, do not accept arbitrary macro wrappers as direct owner-call proof, do not invent observed values for call-presence arguments, do not apply this route to predicate-boundary value checks, do not emit public repair packets from static-limitation backlog, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_helper_wraps_owner_call_in_format_args_macro_then_activation_is_yes, crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_format_args_macro_short_circuits_owner_then_activation_stays_unknown, docs/specs/RIPR-SPEC-0035-evidence-quality-benchmark-corpus.md`
- Receipt validation: pass

### Real Repair Attempt `block_helper_owner_call_route_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-same-file-only`
- Packet ID: `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `fixture-backed test-local helper where a direct production owner call is wrapped only by a simple block expression for value-insensitive call-presence activation`
- Verify command: `cargo test -p ripr given_call_presence_when_test_local_helper_wraps_owner_call_in_block_then_activation_is_yes --lib -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Taught same-file owner-call tracing to recognize a bounded test-local helper that directly calls the production owner inside a simple block expression while preserving value-insensitive activation boundaries.
- Evidence movement: improved: the analyzer route now has positive and negative fixture proof that a simple direct block wrapper can satisfy value-insensitive activation without accepting conditional block execution.
- Operator note: This is analyzer-route dogfood for an already-merged fixture-backed route. It records movement for the same-file owner-call backlog while preserving static-limitation boundaries for conditional blocks, predicate-boundary value checks, invented observed values, and the remaining backlog.
- Must not change: `do not treat same-file proximity alone as owner-call activation, do not accept conditional block execution as guaranteed owner-call proof, do not invent observed values for call-presence arguments, do not apply this route to predicate-boundary value checks, do not emit public repair packets from static-limitation backlog, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_helper_wraps_owner_call_in_block_then_activation_is_yes, crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_helper_conditionally_calls_owner_then_activation_stays_unknown, fixtures/evidence-quality-benchmark/corpus.json:call_presence_block_helper_owner_call_activation_values_not_required, docs/specs/RIPR-SPEC-0035-evidence-quality-benchmark-corpus.md, EffortlessMetrics/ripr-swarm#805`
- Receipt validation: pass

### Real Repair Attempt `assert_macro_helper_owner_call_route_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-same-file-only`
- Packet ID: `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `fixture-backed test-local helper where a recognized assert! macro directly wraps the production owner call for value-insensitive call-presence activation`
- Verify command: `cargo test -p ripr given_call_presence_when_test_local_assert_macro_helper_calls_owner_then_activation_is_yes --lib -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Taught same-file owner-call tracing to recognize a bounded test-local helper that directly calls the production owner inside a recognized `assert!` macro while preserving value-insensitive activation boundaries.
- Evidence movement: improved: the analyzer route now has positive and negative fixture proof that direct assert!-macro owner calls can satisfy value-insensitive activation without accepting short-circuiting predicates.
- Operator note: This is analyzer-route dogfood for an already-merged fixture-backed route. It records movement for the same-file owner-call backlog while preserving static-limitation boundaries for short-circuiting predicates, arbitrary macros, predicate-boundary value checks, invented observed values, and the remaining backlog.
- Must not change: `do not treat same-file proximity alone as owner-call activation, do not accept short-circuiting assert! predicates as guaranteed owner-call proof, do not accept arbitrary macro wrappers as direct owner-call proof, do not invent observed values for call-presence arguments, do not apply this route to predicate-boundary value checks, do not emit public repair packets from static-limitation backlog, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_assert_macro_helper_calls_owner_then_activation_is_yes, crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_assert_macro_short_circuits_owner_then_activation_stays_unknown, fixtures/evidence-quality-benchmark/corpus.json:call_presence_assert_macro_helper_owner_call_activation_values_not_required, docs/specs/RIPR-SPEC-0035-evidence-quality-benchmark-corpus.md, EffortlessMetrics/ripr-swarm#808`
- Receipt validation: pass

### Real Repair Attempt `matches_macro_helper_owner_call_route_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-same-file-only`
- Packet ID: `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `fixture-backed test-local helper where a direct production owner call is wrapped directly by matches!(...) for value-insensitive call-presence activation`
- Verify command: `cargo test -p ripr given_call_presence_when_test_local_matches_macro_helper_calls_owner_then_activation_is_yes --lib -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Taught same-file owner-call tracing to recognize a bounded test-local helper that wraps the production owner call directly in matches!(...) while preserving value-insensitive activation boundaries.
- Evidence movement: improved: the analyzer route now has positive and negative fixture proof that direct matches!(...) owner-call wrapping can satisfy value-insensitive activation without accepting short-circuiting or conditional owner-call execution.
- Operator note: This is analyzer-route dogfood for an already-merged fixture-backed route. It records movement for the same-file owner-call backlog while preserving static-limitation boundaries for short-circuiting matches! guards, predicate-boundary value checks, invented observed values, and the remaining backlog.
- Must not change: `do not treat same-file proximity alone as owner-call activation, do not accept short-circuiting matches! guards as direct owner-call proof, do not invent observed values for call-presence arguments, do not apply this route to predicate-boundary value checks, do not emit public repair packets from static-limitation backlog, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_matches_macro_helper_calls_owner_then_activation_is_yes, crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_matches_macro_short_circuits_owner_then_activation_stays_unknown, fixtures/evidence-quality-benchmark/corpus.json:call_presence_matches_macro_helper_owner_call_activation_values_not_required, docs/specs/RIPR-SPEC-0035-evidence-quality-benchmark-corpus.md, EffortlessMetrics/ripr-swarm#813`
- Receipt validation: pass

### Real Repair Attempt `format_macro_helper_owner_call_route_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-same-file-only`
- Packet ID: `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `fixture-backed test-local helper where a standard format! macro directly contains the production owner call for value-insensitive call-presence activation`
- Verify command: `cargo test -p ripr given_call_presence_when_test_local_helper_wraps_owner_call_in_format_macro_then_activation_is_yes --lib -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Taught same-file owner-call tracing to recognize a bounded test-local helper that wraps the production owner call in a standard `format!` macro argument while preserving value-insensitive activation boundaries.
- Evidence movement: improved: the analyzer route now has fixture and benchmark proof that standard format!-macro helper owner calls can satisfy value-insensitive activation without accepting arbitrary macro wrappers.
- Operator note: This is analyzer-route dogfood for an already-merged fixture-backed route. It records movement for the same-file owner-call backlog while preserving static-limitation boundaries for arbitrary macros, predicate-boundary value checks, invented observed values, and the remaining backlog.
- Must not change: `do not treat same-file proximity alone as owner-call activation, do not accept arbitrary macro wrappers as direct owner-call proof, do not invent observed values for call-presence arguments, do not apply this route to predicate-boundary value checks, do not emit public repair packets from static-limitation backlog, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_helper_wraps_owner_call_in_format_macro_then_activation_is_yes, crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_helper_wraps_owner_call_in_unknown_macro_then_activation_stays_unknown, fixtures/evidence-quality-benchmark/corpus.json:call_presence_format_macro_helper_owner_call_activation_values_not_required, docs/specs/RIPR-SPEC-0035-evidence-quality-benchmark-corpus.md, EffortlessMetrics/ripr-swarm#803`
- Receipt validation: pass

### Real Repair Attempt `tuple_literal_helper_owner_call_route_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-same-file-only`
- Packet ID: `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `fixture-backed test-local helper where a tuple literal directly contains the production owner call for value-insensitive call-presence activation`
- Verify command: `cargo test -p ripr given_call_presence_when_test_local_helper_wraps_owner_call_in_tuple_literal_then_activation_is_yes --lib -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Taught same-file owner-call tracing to recognize a bounded test-local helper that wraps the production owner call in a tuple literal while preserving value-insensitive activation boundaries.
- Evidence movement: improved: the analyzer route now has fixture and benchmark proof that tuple-literal helper owner calls can satisfy value-insensitive activation without accepting arbitrary non-container wrapper calls.
- Operator note: This is analyzer-route dogfood for an already-merged fixture-backed route. It records movement for the same-file owner-call backlog while preserving static-limitation boundaries for arbitrary function wrappers, predicate-boundary value checks, invented observed values, and the remaining backlog.
- Must not change: `do not treat same-file proximity alone as owner-call activation, do not accept arbitrary function wrappers as direct owner-call proof, do not invent observed values for call-presence arguments, do not apply this route to predicate-boundary value checks, do not emit public repair packets from static-limitation backlog, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_helper_wraps_owner_call_in_tuple_literal_then_activation_is_yes, crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_helper_wraps_owner_call_with_non_container_call_then_activation_stays_unknown, fixtures/evidence-quality-benchmark/corpus.json:call_presence_tuple_literal_helper_owner_call_activation_values_not_required, docs/specs/RIPR-SPEC-0035-evidence-quality-benchmark-corpus.md, EffortlessMetrics/ripr-swarm#797`
- Receipt validation: pass

### Real Repair Attempt `std_identity_helper_owner_call_route_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-same-file-only`
- Packet ID: `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `fixture-backed test-local helper where fully qualified std::convert::identity directly contains the production owner call for value-insensitive call-presence activation`
- Verify command: `cargo test -p ripr std_identity --lib -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Taught same-file owner-call tracing to recognize a bounded test-local helper that wraps the production owner call in fully qualified std::convert::identity while preserving the value-insensitive activation boundary.
- Evidence movement: improved: the analyzer route now has fixture proof that std/core convert identity helper owner calls can satisfy value-insensitive activation without accepting local identity wrappers or arbitrary non-container function wrappers.
- Operator note: This is analyzer-route dogfood for the same-file owner-call backlog. It records movement for a syntax-bounded standard-library wrapper while preserving non-actionable backlog boundaries for local identity wrappers, arbitrary function wrappers, predicate-boundary value checks, invented observed values, and the remaining backlog.
- Must not change: `do not treat same-file proximity alone as owner-call activation, do not accept local identity wrappers as direct owner-call proof, do not accept arbitrary function wrappers as direct owner-call proof, do not invent observed values for call-presence arguments, do not apply this route to predicate-boundary value checks, do not emit public repair packets from static-limitation backlog, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_helper_wraps_owner_call_in_std_identity_then_activation_is_yes, crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_helper_wraps_owner_call_in_local_identity_then_activation_stays_unknown, crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_helper_wraps_owner_call_with_non_container_call_then_activation_stays_unknown, target/ripr/reports/swarm-readiness.json:analysis/same-file-owner-call-tracing`
- Receipt validation: pass

### Real Repair Attempt `vec_macro_helper_owner_call_route_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-same-file-only`
- Packet ID: `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `fixture-backed test-local helper where a standard vec! container macro directly contains the production owner call for value-insensitive call-presence activation`
- Verify command: `cargo test -p ripr given_call_presence_when_test_local_helper_wraps_owner_call_in_vec_macro_then_activation_is_yes --lib -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Taught same-file owner-call tracing to recognize a bounded test-local helper that wraps the production owner call in a standard `vec!` container macro such as `vec![render_pipeline("alpha")]` while preserving value-insensitive activation boundaries.
- Evidence movement: improved: the analyzer route now has fixture and benchmark proof that vec-macro helper owner calls can satisfy value-insensitive activation without inventing observed values or promoting the remaining same-file backlog to public repair work.
- Operator note: This is analyzer-route dogfood for an already-merged fixture-backed route. It records movement for the top sampled same-file owner-call backlog while preserving static-limitation boundaries for arbitrary macros, predicate-boundary value checks, and the remaining backlog.
- Must not change: `do not treat same-file proximity alone as owner-call activation, do not accept arbitrary macro wrappers as direct owner-call proof, do not invent observed values for call-presence arguments, do not apply this route to predicate-boundary value checks, do not emit public repair packets from static-limitation backlog, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_helper_wraps_owner_call_in_vec_macro_then_activation_is_yes, fixtures/evidence-quality-benchmark/corpus.json:call_presence_vec_macro_helper_owner_call_activation_values_not_required, docs/specs/RIPR-SPEC-0035-evidence-quality-benchmark-corpus.md, EffortlessMetrics/ripr-swarm#793`
- Receipt validation: pass

### Real Repair Attempt `array_literal_helper_owner_call_route_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-same-file-only`
- Packet ID: `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `fixture-backed test-local helper where a fixed-size array literal directly contains the production owner call for value-insensitive call-presence activation`
- Verify command: `cargo test -p ripr given_call_presence_when_test_local_helper_wraps_owner_call_in_array_literal_then_activation_is_yes --lib -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Taught same-file owner-call tracing to recognize a bounded test-local helper that wraps the production owner call in a fixed-size array literal such as `[render_pipeline("alpha")]` while preserving value-insensitive activation boundaries.
- Evidence movement: improved: the analyzer route now has fixture and benchmark proof that array-literal helper owner calls can satisfy value-insensitive activation without inventing observed values or promoting the remaining same-file backlog to public repair work.
- Operator note: This is analyzer-route dogfood for an already-merged fixture-backed route. It records movement for the top sampled same-file owner-call backlog while preserving static-limitation boundaries for arbitrary wrappers, predicate-boundary value checks, and the remaining backlog.
- Must not change: `do not treat same-file proximity alone as owner-call activation, do not accept arbitrary container or macro wrappers as direct owner-call proof, do not invent observed values for call-presence arguments, do not apply this route to predicate-boundary value checks, do not emit public repair packets from static-limitation backlog, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_helper_wraps_owner_call_in_array_literal_then_activation_is_yes, fixtures/evidence-quality-benchmark/corpus.json:call_presence_array_literal_helper_owner_call_activation_values_not_required, docs/specs/RIPR-SPEC-0035-evidence-quality-benchmark-corpus.md, EffortlessMetrics/ripr-swarm#795`
- Receipt validation: pass

### Real Repair Attempt `same_file_fanout_wrapper_owner_call_route_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-same-file-only`
- Packet ID: `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `fixture-backed embedded same-file unit test where a production fan-out wrapper directly and unconditionally calls the owner for value-insensitive call-presence activation`
- Verify command: `cargo test -p ripr given_call_presence_when_test_calls_same_file_fanout_wrapper_then_activation_is_yes --lib -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Taught same-file owner-call tracing to recognize embedded unit tests that call a production fan-out wrapper when that wrapper directly and unconditionally calls the owner along with other local helpers.
- Evidence movement: improved: the analyzer route now has fixture and benchmark proof that embedded same-file fan-out wrappers can satisfy value-insensitive activation without exporting mixed-owner wrapper activation to tests/*.rs helpers or cross-file tests.
- Operator note: This is analyzer-route dogfood. It records fixture-backed movement for the same-file owner-call backlog while preserving target-affinity guardrails for tests/*.rs fan-out helpers and cross-file mixed-owner production wrappers.
- Must not change: `do not treat same-file proximity alone as owner-call activation, do not export relaxed fan-out matching to tests/*.rs helpers without target affinity, do not export mixed-owner production wrappers to cross-file integration tests without target affinity, do not accept conditional owner-call paths as guaranteed activation, do not invent observed values for call-presence arguments, do not apply this route to predicate-boundary value checks, do not emit public repair packets from static-limitation backlog, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_calls_same_file_fanout_wrapper_then_activation_is_yes, crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_fanout_helper_asserts_other_target_then_activation_stays_unknown, crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_multi_owner_wrapper_asserts_other_target_then_activation_stays_unknown, crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_value_insensitive_seam_when_multi_owner_wrapper_asserts_other_target_then_activation_stays_unknown, fixtures/evidence-quality-benchmark/corpus.json:call_presence_same_file_fanout_wrapper_activation_values_not_required, docs/specs/RIPR-SPEC-0035-evidence-quality-benchmark-corpus.md`
- Receipt validation: pass

### Real Repair Attempt `equality_macro_later_arg_helper_owner_call_route_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-same-file-only`
- Packet ID: `limitation:activation_owner_call_absent_same_file_only:same_file_only_call_presence_function_call_missing_owner_call:analysis-same-file-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `fixture-backed test-local helper where a recognized equality macro directly and unconditionally calls the owner as a later top-level argument for value-insensitive call-presence activation`
- Verify command: `cargo test -p ripr given_call_presence_when_test_local_equality_helper_calls_owner_as_later_arg_then_activation_is_yes --lib -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Taught same-file owner-call tracing to recognize owner calls passed as later top-level arguments to eager equality macros such as `assert_eq!(expected, render_pipeline("alpha"))`.
- Evidence movement: improved: the analyzer route now has fixture and benchmark proof that equality-macro helper owner calls can satisfy value-insensitive activation without treating assert! message arguments or arbitrary macro arguments as owner-call proof.
- Operator note: This is analyzer-route dogfood. It records fixture-backed movement for the same-file owner-call backlog while preserving static-limitation boundaries for assert! message arguments, arbitrary macros, predicate-boundary value checks, and public repair packets.
- Must not change: `do not treat same-file proximity alone as owner-call activation, do not treat assert! message arguments as guaranteed owner-call activation, do not accept arbitrary later macro arguments as direct owner-call proof, do not invent observed values for call-presence arguments, do not apply this route to predicate-boundary value checks, do not emit public repair packets from static-limitation backlog, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_equality_helper_calls_owner_as_later_arg_then_activation_is_yes, crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_assert_message_arg_calls_owner_then_activation_stays_unknown, fixtures/evidence-quality-benchmark/corpus.json:call_presence_equality_macro_later_arg_helper_owner_call_activation_values_not_required, fixtures/evidence-quality-benchmark/corpus.json:call_presence_assert_message_arg_helper_stays_named_limitation, docs/specs/RIPR-SPEC-0035-evidence-quality-benchmark-corpus.md`
- Receipt validation: pass

### Real Repair Attempt `call_presence_imported_module_wrapper_route_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-call-presence-target-affinity`
- Packet ID: `limitation:activation_owner_call_absent_call_presence_target_affinity:analysis-call-presence-target-affinity-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `fixture-backed imported module-qualified production wrapper that directly calls the owner while assertions mention the call-presence target token`
- Verify command: `cargo test -p ripr given_call_presence_when_imported_module_wrapper_has_target_affinity_then_activation_is_yes --lib`
- Verify result: `pass`
- Receipt command: `cargo xtask lane1-evidence-audit`
- Receipt path: `target/ripr/reports/lane1-evidence-audit.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Taught target-affinity owner-call tracing to recognize imported module-qualified production wrapper calls such as loop_commands::shell_path when the import resolves to the owner module and the assertion target token matches.
- Evidence movement: The sampled call-presence target-affinity limitation route moved from 801 to 799 signals, and the prior shell_path/display_path sample from crates/ripr/src/agent/loop_commands.rs no longer appears as a top backlog sample.
- Operator note: This is an analyzer-routing improvement only. The route remains a static-limitation backlog because most target-affinity cases still lack safe owner-call tracing.
- Must not change: `do not treat assertion target text alone as activation, do not resolve arbitrary cross-module calls without an imported module alias, do not invent observed values for call-presence arguments, do not emit public repair packets from static-limitation backlog, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_imported_module_wrapper_has_target_affinity_then_activation_is_yes, fixtures/evidence-quality-benchmark/corpus.json:call_presence_imported_module_target_affinity_wrapper_activation_values_not_required, target/ripr/reports/lane1-evidence-audit.json:activation_owner_call_absent_call_presence_target_affinity`
- Receipt validation: pass

### Real Repair Attempt `call_presence_same_file_unit_wrapper_route_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-call-presence-target-affinity`
- Packet ID: `limitation:activation_owner_call_absent_call_presence_target_affinity:analysis-call-presence-target-affinity-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `fixture-backed same-file unit test that calls a production wrapper imported from the parent module while assertions mention the call-presence target token`
- Verify command: `cargo test -p ripr given_call_presence_when_unit_test_calls_same_file_target_affinity_wrapper_then_activation_is_yes --lib`
- Verify result: `pass`
- Receipt command: `cargo xtask lane1-evidence-audit`
- Receipt path: `target/ripr/reports/lane1-evidence-audit.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Taught target-affinity owner-call tracing to recognize same-file unit tests that call production wrappers imported from the parent module, while preserving test-local helper shadow limits.
- Evidence movement: The sampled call-presence target-affinity limitation route moved from 799 to 762 signals, and the assertion-target affinity owner-call route moved from 737 to 684 signals.
- Operator note: This is an analyzer-routing improvement only. The route remains a static-limitation backlog because remaining target-affinity cases still lack safe owner-call tracing.
- Must not change: `do not treat assertion target text alone as activation, do not inherit production wrapper relations for test-local helper shadows, do not follow multi-hop wrappers, do not invent observed values for call-presence arguments, do not emit public repair packets from static-limitation backlog, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_unit_test_calls_same_file_target_affinity_wrapper_then_activation_is_yes, crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_test_local_helper_shadows_target_affinity_wrapper_then_activation_stays_unknown, fixtures/evidence-quality-benchmark/corpus.json:call_presence_same_file_unit_target_affinity_wrapper_activation_values_not_required, target/ripr/reports/lane1-evidence-audit.json:activation_owner_call_absent_call_presence_target_affinity`
- Receipt validation: pass

### Real Repair Attempt `call_presence_direct_import_wrapper_route_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-call-presence-target-affinity`
- Packet ID: `limitation:activation_owner_call_absent_call_presence_target_affinity:analysis-call-presence-target-affinity-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `fixture-backed production wrapper that calls an unambiguous crate-local directly imported owner while assertions mention the call-presence target token`
- Verify command: `cargo test -p ripr given_call_presence_when_direct_imported_wrapper_has_target_affinity_then_activation_is_yes --lib`
- Verify result: `pass`
- Receipt command: `cargo xtask lane1-evidence-audit`
- Receipt path: `target/ripr/reports/lane1-evidence-audit.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Taught target-affinity owner-call tracing to recognize production wrappers that call crate-local directly imported owner functions, including import aliases, while preserving target-token, external-import, and ambiguity guards.
- Evidence movement: The sampled call-presence target-affinity limitation route moved from 762 to 750 signals, and the assertion-target affinity owner-call route moved from 684 to 663 signals.
- Operator note: This is an analyzer-routing improvement only. The route remains a static-limitation backlog because remaining target-affinity cases still lack safe owner-call tracing.
- Must not change: `do not treat assertion target text alone as activation, do not treat external or bare imports as local owner-call evidence, do not resolve ambiguous imported owner names, do not follow multi-hop wrappers, do not invent observed values for call-presence arguments, do not emit public repair packets from static-limitation backlog, do not change gate or badge semantics`
- Raw evidence refs: `crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_direct_imported_wrapper_has_target_affinity_then_activation_is_yes, crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_external_direct_import_matches_local_owner_name_then_activation_stays_unknown, crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_ambiguous_direct_import_owner_name_then_activation_stays_unknown, crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_direct_imported_wrapper_asserts_other_target_then_activation_stays_unknown, crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_aliased_module_wrapper_has_target_affinity_then_activation_is_yes, crates/ripr/src/analysis/test_grip_evidence.rs::tests::given_call_presence_when_bare_aliased_module_wrapper_has_target_affinity_then_activation_stays_unknown, fixtures/evidence-quality-benchmark/corpus.json:call_presence_direct_import_target_affinity_wrapper_activation_values_not_required, target/ripr/reports/lane1-evidence-audit.json:activation_owner_call_absent_call_presence_target_affinity`
- Receipt validation: pass

### Real Repair Attempt `assertion_target_affinity_limitation_route_improved`

- Canonical gap ID: `gap:activation-owner-call-absent-assertion-target-affinity`
- Packet ID: `limitation:activation-owner-call-absent-assertion-target-affinity:assertion-target-affinity-owner-call-tracing`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `evidence-record and benchmark fixture assertions that assertion-target affinity remains a named static limitation and does not satisfy activation`
- Verify command: `cargo test -p ripr evidence_record_splits_assertion_target_owner_call_absence_limitation`
- Verify result: `pass`
- Receipt command: `cargo xtask check-fixture-contracts`
- Receipt path: `fixtures/evidence-quality-benchmark/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Split assertion-target affinity owner-call absence into a route-specific static limitation while preserving non-actionable status.
- Evidence movement: The assertion-target affinity route now has fixture-backed proof that RIPR names the analyzer limitation instead of treating token affinity as activation evidence.
- Operator note: This is a successful negative-capability repair-loop attempt: it improved analyzer backlog precision without emitting a public repair packet.
- Must not change: `public projection eligibility for static limitations, activation satisfied by token affinity, direct owner-call or helper-call requirements, public badge semantics`
- Raw evidence refs: `fixtures/evidence-quality-benchmark/corpus.json:call_presence_specific_call_target_affinity_stays_named_limitation, crates/ripr/src/output/evidence_record.rs:evidence_record_splits_assertion_target_owner_call_absence_limitation, docs/specs/RIPR-SPEC-0031-lane1-evidence-quality-audit.md, metrics/capabilities.toml:lane1_canonical_actionability_contract`
- Receipt validation: pass

### Real Repair Attempt `iterator_boundary_limitation_route_improved`

- Canonical gap ID: `gap:fixture_iterator_local_boundary`
- Packet ID: `limitation:activation-boundary-input-unresolved:iterator-boundary-operand-resolution`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `evidence-record and swarm-plan fixture assertions that iterator-derived boundary operands remain named static limitations instead of public repair packets`
- Verify command: `cargo test -p ripr evidence_record_keeps_unresolved_boundary_operands_as_named_limitation`
- Verify result: `pass`
- Receipt command: `cargo xtask check-fixture-contracts`
- Receipt path: `fixtures/boundary_gap/expected/evidence-quality-failures/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Routed iterator-derived activation-boundary operands to a precise static-limitation analyzer backlog instead of speculative boundary assertion guidance.
- Evidence movement: Iterator-derived operands now have fixture-backed proof that RIPR refuses to invent activation values and names the analyzer route required to unlock actionability.
- Operator note: This is a negative-capability repair-loop attempt: the useful outcome is sharper non-actionability, not a public packet.
- Must not change: `public projection eligibility for static limitations, synthetic activation values for iterator-derived operands, repair_kind add_boundary_assertion for unresolved iterator operands, public badge semantics`
- Raw evidence refs: `fixtures/boundary_gap/expected/evidence-quality-failures/corpus.json:fixture_iterator_local_boundary, fixtures/swarm-plan-packet-corpus/corpus.json:iterator_boundary_static_limitation, crates/ripr/src/output/evidence_record.rs:evidence_record_keeps_unresolved_boundary_operands_as_named_limitation, docs/specs/RIPR-SPEC-0061-lane1-canonical-actionability-contract.md`
- Receipt validation: pass

### Real Repair Attempt `lsp_call_observer_attempted_no_receipt_timeout`

- Canonical gap ID: `gap:54970c87c8f8ca9d`
- Packet ID: `gap:54970c87c8f8ca9d`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `serve_stdio call-presence observer asserting the LSP stdio entrypoint still constructs and serves the tower LSP server`
- Verify command: `ripr agent verify --root . --before target/ripr/pilot/repo-exposure.json --after target/ripr/pilot/after.repo-exposure.json --json`
- Verify result: `not_run`
- Receipt command: `ripr agent receipt --root . --verify-json target/ripr/workflow/agent-verify.json --seam-id c4abcb8dc04cf9c4 --json --out target/ripr/reports/agent-receipt.json`
- Receipt path: `missing`
- Receipt state: `receipt_missing`
- Before/after: `actionable` -> `actionable`
- Attempted repair: Added serve_stdio_call_presence_observer in the packet's allowed edit surface, crates/ripr/src/lsp/tests.rs.
- Evidence movement: The narrow observer test passed, but the formal after repo-exposure snapshot generation timed out before agent verify or receipt could complete.
- Operator note: This is an honest failed handoff proof: the packet was bounded enough to attempt, but its generated verify route was too broad to receipt in this run.
- Missing receipt reason: formal agent verify receipt was not emitted because generating target/ripr/pilot/after.repo-exposure.json timed out after 20 minutes
- Must not change: `crates/ripr/src/lsp.rs, public badge semantics, CI gate defaults, provider integration, mutation execution`
- Raw evidence refs: `target/ripr/reports/swarm-plan.json:gap:54970c87c8f8ca9d, target/ripr/reports/swarm-attempt-ledger.json:gap:54970c87c8f8ca9d, crates/ripr/src/lsp/tests.rs:serve_stdio_call_presence_observer`
- Receipt validation: pass

### Real Repair Attempt `lsp_call_observer_bounded_receipt_improved`

- Canonical gap ID: `gap:54970c87c8f8ca9d`
- Packet ID: `gap:54970c87c8f8ca9d`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `bounded serve_stdio call-presence observer verified by the focused LSP test filter`
- Verify command: `cargo test -p ripr serve_stdio_call_presence_observer`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `actionable` -> `actionable`
- Attempted repair: Re-ran the call-observer route through the bounded related-test verify command instead of the broad repo-exposure snapshot comparison.
- Evidence movement: improved: the add_call_observer route now has a focused verify command and a durable dogfood receipt instead of only an attempted_no_receipt timeout.
- Operator note: The original timed-out attempt stays in the corpus as historical evidence; this follow-up only proves the bounded receipt route, not closure of the original static gap.
- Must not change: `crates/ripr/src/lsp.rs, public badge semantics, CI gate defaults, provider integration, mutation execution`
- Raw evidence refs: `fixtures/real-repair-attempts/corpus.json:lsp_call_observer_attempted_no_receipt_timeout, crates/ripr/src/lsp/tests.rs:serve_stdio_call_presence_observer, xtask/src/main.rs:lane1_actionable_gap_verify_command_uses_bounded_related_target_over_unbounded_snapshot`
- Receipt validation: pass

### Real Repair Attempt `repair_route_quality_sample_actions_improved`

- Canonical gap ID: `gap:repair-route-quality-sample-actions`
- Packet ID: `repair-route-quality-sample-actions-001`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `attempt-ledger and readiness assertions that failing repair-route quality rows carry sample packet and canonical gap identities`
- Verify command: `cargo test -p xtask ripr_swarm_repair_route_quality_samples_failed_attempts_only -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask ripr-swarm readiness`
- Receipt path: `target/ripr/reports/swarm-readiness.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `actionable` -> `actionable`
- Attempted repair: Added sample packet IDs and canonical gap IDs to repair-route quality rows and copied the first failed sample into improve_repair_route_quality readiness actions.
- Evidence movement: Readiness can now route repair-route-quality work to a concrete failed attempt instead of only naming an aggregate repair_kind.
- Operator note: This improves route-learning guidance without changing public badge semantics, CI gates, provider integration, mutation execution, or autonomous edits.
- Must not change: `public badge semantics, CI gate defaults, provider integration, mutation execution, autonomous repair behavior`
- Raw evidence refs: `xtask/src/main.rs:ripr_swarm_repair_route_quality_samples_failed_attempts_only, xtask/src/main.rs:ripr_swarm_readiness_consumes_attempt_ledger_counts, docs/OUTPUT_SCHEMA.md:repair-route quality rows include sample packet IDs, docs/specs/RIPR-SPEC-0057-ripr-swarm-repair-loop.md:Repair-route quality rows should carry sample packet IDs`
- Receipt validation: pass

### Real Repair Attempt `typescript_preview_lsp_repair_context_improved`

- Canonical gap ID: `gap:typescript-preview-lsp-repair-context`
- Packet ID: `limitation:typescript-preview-lsp-repair-context`
- Language: `typescript`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `LSP diagnostic, hover, and inspect context preserve TypeScript preview actionability without offering repair-packet commands.`
- Verify command: `cargo test -p ripr lsp --lib`
- Verify result: `pass`
- Receipt command: `cargo xtask check-capabilities`
- Receipt path: `docs/lanes/LANE_1_TYPESCRIPT_PREVIEW_COMPLETION.md`
- Receipt state: `receipt_movement_improved`
- Before/after: `advisory` -> `advisory`
- Attempted repair: Projected TypeScript preview LSP context as advisory evidence so editors can explain the limitation without creating a delegated repair packet.
- Evidence movement: improved: TypeScript preview context became inspectable while remaining non-actionable and outside public repair projection.
- Operator note: Useful as a preview repair-loop receipt because it proves the LSP surface can carry bounded context without inventing verify or receipt commands for TypeScript analysis.
- Must not change: `do not emit public repair packets for TypeScript preview advisory context, do not add generated test edits, do not change badge or gate semantics, do not claim full TypeScript support`
- Raw evidence refs: `crates/ripr/src/lsp/diagnostics.rs, crates/ripr/src/lsp/hover.rs, docs/lanes/LANE_1_TYPESCRIPT_PREVIEW_COMPLETION.md, EffortlessMetrics/ripr-swarm#617`
- Receipt validation: pass

### Real Repair Attempt `javascript_preview_generated_ci_grouping_improved`

- Canonical gap ID: `gap:javascript-preview-generated-ci-grouping`
- Packet ID: `limitation:javascript-preview-generated-ci-grouping`
- Language: `javascript`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `Generated CI groups JavaScript preview evidence under the TypeScript-family preview configuration and keeps actionability/gate impact set to none.`
- Verify command: `cargo test -p ripr init_generated_github_workflow_groups_preview_languages_only_when_configured --lib`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `docs/lanes/LANE_1_TYPESCRIPT_PREVIEW_COMPLETION.md`
- Receipt state: `receipt_movement_improved`
- Before/after: `advisory` -> `advisory`
- Attempted repair: Recorded generated-CI grouping proof for JavaScript/TypeScript preview artifacts without making preview language evidence block CI.
- Evidence movement: improved: preview language CI grouping is now represented as auditable advisory evidence with no public repair or gate claim.
- Operator note: This receipt documents the generated-CI proof path needed before route-quality metrics can distinguish TypeScript-family preview evidence from Rust repair packets.
- Must not change: `do not make preview language evidence blocking, do not mark JavaScript preview gaps public_projection_eligible, do not emit repair verify commands for preview language artifacts, do not change published badge semantics`
- Raw evidence refs: `crates/ripr/src/cli/commands.rs:init_generated_github_workflow_groups_preview_languages_only_when_configured, xtask/src/main.rs:dogfood_language_preview_scenarios_cover_projection_boundaries, docs/specs/RIPR-SPEC-0038-generated-pr-ci-review-workflow.md, EffortlessMetrics/ripr-swarm#622`
- Receipt validation: pass

### Real Repair Attempt `typescript_preview_mocked_module_limitation_improved`

- Canonical gap ID: `gap:typescript-preview-mocked-module-limitation`
- Packet ID: `limitation:typescript-preview-mocked-module-limitation`
- Language: `typescript`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `Mocked-module TypeScript preview evidence remains a static limitation with samples and route context, not a repair packet.`
- Verify command: `cargo xtask fixtures typescript_mocked_module_limit`
- Verify result: `pass`
- Receipt command: `cargo xtask check-fixture-contracts`
- Receipt path: `fixtures/typescript_mocked_module_limit/expected/check.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Dogfooded the mocked-module preview limitation as analyzer backlog evidence instead of promoting it to a repair queue item.
- Evidence movement: improved: mocked-module preview evidence has a stable limitation receipt and does not leak into public repair projection.
- Operator note: The useful movement is sharper non-actionability: the fixture tells future analyzer work what context is missing without telling an agent to edit tests.
- Must not change: `do not emit a public repair packet from mocked-module preview evidence, do not invent module-resolution certainty, do not edit production TypeScript from this backlog item alone, do not add verify or receipt commands to the preview limitation itself`
- Raw evidence refs: `fixtures/typescript_mocked_module_limit/expected/check.json, docs/lanes/LANE_1_TYPESCRIPT_PREVIEW_COMPLETION.md, metrics/capabilities.toml:language_adapter_typescript_preview`
- Receipt validation: pass

### Real Repair Attempt `typescript_module_initializer_observer_route_improved`

- Canonical gap ID: `gap:typescript-preview-module-initializer-observer-route`
- Packet ID: `limitation:typescript-preview-module-initializer-observer-route`
- Language: `typescript`
- Evidence class: `module_initializer`
- Source file: `src/owners.ts`
- Target shape: `TypeScript preview module-initializer evidence recognizes direct named-import and namespace `expect(...)` observers as related advisory context while helper-derived values, shadowed aliases, dynamic initialization, and non-expect references remain non-actionable.`
- Verify command: `cargo test -p ripr typescript --lib -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask fixtures typescript_owner_kinds`
- Receipt path: `fixtures/typescript_owner_kinds/expected/check.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `advisory` -> `advisory`
- Attempted repair: Added bounded TypeScript module-initializer observer matching for direct named-import and namespace expect actuals, while keeping shadowed aliases, derived locals, and non-expect references out of relation matching.
- Evidence movement: improved: the DEFAULT_RATE fixture now records one related weak relational observer and routes the item to missing_target_shape with explicit promotion requirements instead of generic missing_context.
- Operator note: This is analyzer-route dogfood. The route makes advisory evidence sharper, but it deliberately does not emit a repair packet because repair_kind, target_test_shape, verify_command, receipt_command, must_not_change, allowed_edit_surface, and raw_evidence_refs are still required for public delegation.
- Must not change: `do not promote TypeScript preview module-initializer evidence to public repair packets, do not treat relational checks as exact target-shape proof, do not accept helper-derived values, shadowed aliases, or non-expect references as bounded observers, do not change badge or CI gate semantics, do not add provider calls or mutation execution`
- Raw evidence refs: `fixtures/typescript_owner_kinds/expected/check.json, fixtures/typescript_owner_kinds/expected/human.txt, fixtures/typescript-preview-false-actionable-audit/corpus.json, crates/ripr/src/analysis/language/typescript.rs, docs/lanes/TYPESCRIPT_ENABLEMENT.md, EffortlessMetrics/ripr-swarm#749`
- Receipt validation: pass

### Real Repair Attempt `typescript_method_receiver_observer_route_improved`

- Canonical gap ID: `gap:typescript-preview-method-receiver-observer-route`
- Packet ID: `limitation:typescript-preview-method-receiver-observer-route`
- Language: `typescript`
- Evidence class: `method_receiver`
- Source file: `src/owners.ts`
- Target shape: `TypeScript preview method evidence recognizes direct `new ClassName(...)` receiver observers as related advisory context while factories, dependency injection, mocked classes, prototype aliases, and dynamic property access remain non-actionable.`
- Verify command: `cargo test -p ripr find_related_tests_matches_bounded_method_receiver_calls --lib`
- Verify result: `pass`
- Receipt command: `cargo xtask fixtures typescript_owner_kinds`
- Receipt path: `fixtures/typescript_owner_kinds/expected/check.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `advisory` -> `advisory`
- Attempted repair: Recorded bounded TypeScript method-receiver related-test matching for direct new-instance observers, while preserving the incomplete repair-packet boundary for the preview finding.
- Evidence movement: improved: the Cart.total fixture records a related method receiver smoke observer and exact missing return-value discriminator without promoting the item beyond advisory preview.
- Operator note: This is TypeScript enablement dogfood for useful advisory guidance. The route is more precise, but it deliberately remains non-actionable because canonical packet fields, verify and receipt authority, edit boundaries, and stronger oracle proof are still required for public delegation.
- Must not change: `do not promote TypeScript preview method-receiver evidence to public repair packets, do not infer through factories, dependency injection, mocked classes, prototype aliases, or dynamic property access, do not treat smoke-only receiver evidence as exact proof, do not change badge or CI gate semantics, do not add provider calls, generated tests, source edits, runtime Jest/Vitest execution, or mutation execution`
- Raw evidence refs: `fixtures/typescript_owner_kinds/expected/check.json#probe:src_owners.ts:typescript_preview:bd728bcd, fixtures/typescript_owner_kinds/expected/human.txt, fixtures/typescript-preview-false-actionable-audit/corpus.json, crates/ripr/src/analysis/language/typescript.rs, docs/lanes/TYPESCRIPT_ENABLEMENT.md`
- Receipt validation: pass

### Real Repair Attempt `typescript_preview_weak_oracle_downgrade_unchanged`

- Canonical gap ID: `gap:typescript-preview-weak-oracle-downgrade`
- Packet ID: `limitation:typescript-preview-weak-oracle-downgrade`
- Language: `typescript`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `Weak TypeScript preview oracles stay advisory/static-limitation evidence until the analyzer can prove a bounded repair shape.`
- Verify command: `cargo xtask fixtures typescript_jest_vitest_assertion_facts`
- Verify result: `pass`
- Receipt command: `cargo xtask check-fixture-contracts`
- Receipt path: `fixtures/typescript_jest_vitest_assertion_facts/expected/check.json`
- Receipt state: `receipt_missing`
- Before/after: `advisory` -> `advisory`
- Attempted repair: Checked the weak-oracle preview boundary and intentionally left it non-actionable because it lacks a safe delegated repair contract.
- Evidence movement: unchanged: weak-oracle preview evidence remains advisory, which is the correct outcome until TypeScript oracle semantics are stronger.
- Operator note: This is a useful unchanged receipt: the route should not be promoted just to increase actionable count.
- Must not change: `do not promote weak TypeScript preview oracles to actionable repair packets, do not invent exact observer expectations, do not emit generated tests, do not change gate or badge semantics`
- Raw evidence refs: `fixtures/typescript_jest_vitest_assertion_facts/expected/check.json, docs/lanes/LANE_1_TYPESCRIPT_PREVIEW_COMPLETION.md, metrics/capabilities.toml:language_adapter_typescript_preview`
- Receipt validation: pass

### Real Repair Attempt `typescript_preview_weak_oracle_guidance_improved`

- Canonical gap ID: `gap:typescript-preview-weak-oracle-downgrade`
- Packet ID: `limitation:typescript-preview-weak-oracle-downgrade`
- Language: `typescript`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `Weak TypeScript preview oracle guidance is oracle-specific: snapshot and smoke evidence name exact-value assertion shape, while mock interaction evidence stays advisory until mock payload support can name callee, expected arguments, verify command, receipt command, and edit boundaries.`
- Verify command: `cargo test -p ripr typescript_preview_weak_oracle --lib`
- Verify result: `pass`
- Receipt command: `cargo xtask fixtures typescript_jest_vitest_assertion_facts`
- Receipt path: `fixtures/typescript_jest_vitest_assertion_facts/expected/check.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `advisory` -> `advisory`
- Attempted repair: Refined TypeScript weak-oracle guidance so the route preserves the preview boundary without telling agents to add the wrong assertion shape.
- Evidence movement: improved: snapshot, smoke-only, and mock-interaction weak oracles now produce distinct advisory guidance while still withholding actionable repair packets.
- Operator note: The useful movement is sharper routing language, not support-tier promotion. The mock interaction case remains non-actionable because payload-level evidence and strict packet fields are still absent.
- Must not change: `do not promote weak TypeScript preview oracles to actionable repair packets, do not treat mock expectations as payload-safe discriminators, do not emit generated tests, do not change gate or badge semantics`
- Raw evidence refs: `fixtures/typescript_jest_vitest_assertion_facts/expected/check.json, fixtures/typescript_jest_vitest_assertion_facts/expected/human.txt, crates/ripr/src/analysis/language/typescript.rs`
- Receipt validation: pass

### Real Repair Attempt `typescript_preview_boundary_gap_test_only_closed`

- Canonical gap ID: `gap:typescript:typescript_preview:2396aec1`
- Packet ID: `typescript-repair-packet:gap:typescript:typescript_preview:2396aec1`
- Language: `typescript`
- Evidence class: `predicate_boundary`
- Source file: `src/discount.ts`
- Target shape: `strengthen the existing Jest test with an exact amount == threshold boundary assertion`
- Verify command: `jest tests/discount.test.ts`
- Verify result: `pass`
- Receipt command: `ripr outcome --before fixtures/first_successful_pr/typescript-preview-gap/inputs/reports/before-check.json --after fixtures/first_successful_pr/typescript-preview-gap/inputs/reports/after-check.json --format json`
- Receipt path: `fixtures/first_successful_pr/typescript-preview-gap/expected/outcome/closed.json`
- Receipt state: `receipt_movement_resolved`
- Before/after: `actionable` -> `resolved`
- Attempted repair: A bounded TypeScript preview packet edited only tests/discount.test.ts, replaced weak relational evidence with an exact threshold assertion, and left src/discount.ts forbidden.
- Evidence movement: resolved: ripr outcome compared before/after TypeScript check-output snapshots and reported the canonical predicate-boundary gap closed.
- Operator note: This is static preview repair-routing dogfood: verify evidence and receipt movement are present, but the TypeScript packet remains advisory and does not claim runtime adequacy or support-tier promotion.
- Must not change: `src/discount.ts, production TypeScript files, preview support tier, CI gate defaults, badge, baseline, or RIPR Zero authority, provider integration, mutation execution`
- Raw evidence refs: `fixtures/ts_repair_packet_complete/expected/check.json, fixtures/first_successful_pr/typescript-preview-gap/inputs/reports/before-check.json, fixtures/first_successful_pr/typescript-preview-gap/inputs/reports/after-check.json, fixtures/first_successful_pr/typescript-preview-gap/expected/outcome/closed.json, fixtures/first_successful_pr/typescript-preview-gap/expected/outcome/closed.md`
- Receipt validation: pass

### Real Repair Attempt `python_cli_output_repair_card_route_improved`

- Canonical gap ID: `gap:python:src/commands.py:ship:call_or_output_effect:side_effect:click.echo_shipment_queued`
- Packet ID: `python-repair-card:gap:python:src/commands.py:ship:call_or_output_effect:side_effect:click.echo_shipment_queued`
- Language: `python`
- Evidence class: `call_or_output_effect`
- Source file: `src/commands.py`
- Target shape: `Python preview repair-card guidance for a Click CLI output change: strengthen the existing pytest test `tests/test_commands.py::test_ship_smoke` with an exact output/log/call-effect assertion for `output contains "shipment queued"`.`
- Verify command: `cargo xtask fixtures python_cli_output_repair_gap`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `advisory`
- Attempted repair: Routed simple Python Click CLI output changes through the Python preview repair-card flow instead of decorator/import limitations, with a concrete missing output discriminator and focused pytest verify command.
- Evidence movement: improved: the fixture now emits a canonical Python output-effect gap, repair-card target shape, suggested pytest file/test/node id, and verify command while keeping receipt authority unavailable until the Python gap ledger can issue concrete receipt commands.
- Operator note: This is analyzer-route dogfood, not a public repair delegation. The preview card is useful guidance, but it remains advisory because the card itself reports receipt status as unavailable_until_python_gap_ledger.
- Must not change: `do not promote Python preview cards to public repair packets, do not claim runtime adequacy from static Click/Typer recognition, do not require pytest, Click, imports, or virtualenv execution during analysis, do not authorize production-code edits, do not change badge or CI gate semantics`
- Raw evidence refs: `fixtures/python_cli_output_repair_gap/SPEC.md, fixtures/python_cli_output_repair_gap/expected/check.json, fixtures/python_cli_output_repair_gap/expected/human.txt, crates/ripr/src/analysis/language/python.rs, crates/ripr/src/output/python_repair_card.rs, EffortlessMetrics/ripr-swarm#750`
- Receipt validation: pass

### Real Repair Attempt `python_argparse_output_repair_card_route_improved`

- Canonical gap ID: `gap:python:src/commands.py:main:call_or_output_effect:side_effect:print_shipment_queued`
- Packet ID: `python-repair-card:gap:python:src/commands.py:main:call_or_output_effect:side_effect:print_shipment_queued`
- Language: `python`
- Evidence class: `call_or_output_effect`
- Source file: `src/commands.py`
- Target shape: `Python preview repair-card guidance for an argparse CLI output change: strengthen the existing pytest test `tests/test_commands.py::test_main_smoke` with an exact output assertion for `output contains "shipment queued"`.`
- Verify command: `cargo xtask fixtures python_argparse_output_repair_gap`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `advisory`
- Attempted repair: Routed argparse-shaped Python CLI output changes through the Python preview repair-card flow instead of treating parser construction as the changed behavior, with a concrete missing output discriminator and focused pytest verify command.
- Evidence movement: improved: the fixture now emits a canonical Python output-effect gap, repair-card target shape, suggested pytest file/test/node id, and verify command for argparse-style command output while keeping receipt authority unavailable until the Python gap ledger can issue concrete receipt commands.
- Operator note: This records a second real Python CLI-output route-quality sample. The preview card is useful guidance, but it remains advisory because the card itself reports receipt status as unavailable_until_python_gap_ledger.
- Must not change: `do not promote Python preview cards to public repair packets, do not treat argparse parser construction as the changed behavior, do not require pytest, argparse imports, or virtualenv execution during analysis, do not authorize production-code edits, do not change badge or CI gate semantics`
- Raw evidence refs: `fixtures/python_argparse_output_repair_gap/SPEC.md, fixtures/python_argparse_output_repair_gap/expected/check.json, fixtures/python_argparse_output_repair_gap/expected/human.txt, crates/ripr/src/analysis/language/python.rs, crates/ripr/src/output/python_repair_card.rs, EffortlessMetrics/ripr-swarm#754`
- Receipt validation: pass

### Real Repair Attempt `python_api_json_field_repair_card_route_improved`

- Canonical gap ID: `gap:python:app/checkout.py:checkout:field_value:field_construction:return_detail_coupon_expired`
- Packet ID: `python-repair-card:gap:python:app/checkout.py:checkout:field_value:field_construction:return_detail_coupon_expired`
- Language: `python`
- Evidence class: `field_construction`
- Source file: `app/checkout.py`
- Target shape: `Python preview repair-card guidance for a literal FastAPI/Flask-shaped route returning a changed JSON field: strengthen the existing pytest test `tests/test_checkout.py::test_expired_coupon_response_smoke` with `response.json()["detail"] == "coupon expired"`.`
- Verify command: `cargo xtask fixtures python_api_json_field_repair_gap`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `advisory`
- Attempted repair: Routed literal Python API endpoint JSON-field changes through the Python preview repair-card flow, linking `client.post("/checkout")` to the route owner and naming the exact `response.json()["detail"] == "coupon expired"` discriminator.
- Evidence movement: improved: the fixture now emits a canonical Python field-value gap, repair-card target shape, suggested pytest file/test/node id, and verify command for API JSON response fields while dynamic route registration remains fail-closed.
- Operator note: This is analyzer-route dogfood, not public repair delegation. The preview card is useful guidance, but it remains advisory because the card itself reports receipt status as unavailable_until_python_gap_ledger.
- Must not change: `do not promote Python preview cards to public repair packets, do not emit repair cards for dynamically registered route paths, do not execute imports, route registration, pytest, or route handlers during analysis, do not authorize production-code edits, do not change badge or CI gate semantics`
- Raw evidence refs: `fixtures/python_api_json_field_repair_gap/SPEC.md, fixtures/python_api_json_field_repair_gap/expected/check.json, fixtures/python_api_json_field_repair_gap/expected/human.txt, fixtures/python_dynamic_route_registration_limit/expected/check.json, crates/ripr/src/analysis/language/python.rs, crates/ripr/src/output/python_repair_card.rs, EffortlessMetrics/ripr-swarm#759`
- Receipt validation: pass

### Real Repair Attempt `python_model_field_repair_card_route_improved`

- Canonical gap ID: `gap:python:src/users.py:build_user:field_value:field_construction:return_user_active=true`
- Packet ID: `python-repair-card:gap:python:src/users.py:build_user:field_value:field_construction:return_user_active=true`
- Language: `python`
- Evidence class: `field_construction`
- Source file: `src/users.py`
- Target shape: `Python preview repair-card guidance for a simple returned model-like constructor keyword field: strengthen the existing pytest test `tests/test_users.py::test_build_user_smoke` with `assert result.active == True`.`
- Verify command: `cargo xtask fixtures python_model_field_repair_gap`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `advisory`
- Attempted repair: Routed simple returned Python model-like constructor keyword fields through the Python preview repair-card flow, naming the exact `result.active == True` discriminator and suggested pytest node.
- Evidence movement: improved: the fixture now emits a canonical Python field-value gap, repair-card target shape, suggested pytest file/test/node id, and verify command for returned object fields while non-simple constructor values remain fail-closed.
- Operator note: This is analyzer-route dogfood, not public repair delegation. The preview card is useful guidance, but it remains advisory because the card itself reports receipt status as unavailable_until_python_gap_ledger.
- Must not change: `do not promote Python preview cards to public repair packets, do not assume runtime constructor semantics or import Python modules, do not emit repair cards for non-simple constructor values, do not authorize production-code edits, do not change badge or CI gate semantics`
- Raw evidence refs: `fixtures/python_model_field_repair_gap/SPEC.md, fixtures/python_model_field_repair_gap/expected/check.json, fixtures/python_model_field_repair_gap/expected/human.txt, crates/ripr/src/analysis/language/python.rs, crates/ripr/src/output/python_repair_card.rs, EffortlessMetrics/ripr-swarm#772`
- Receipt validation: pass

### Real Repair Attempt `python_parametrized_boundary_repair_card_route_improved`

- Canonical gap ID: `gap:python:src/discount.py:apply_discount:predicate_boundary:predicate:amount>=threshold`
- Packet ID: `python-repair-card:gap:python:src/discount.py:apply_discount:predicate_boundary:predicate:amount>=threshold`
- Language: `python`
- Evidence class: `predicate_boundary`
- Source file: `src/discount.py`
- Target shape: `Python preview repair-card guidance for an equality-boundary change: strengthen the existing pytest test `tests/test_discount.py::test_apply_discount_smoke` at `amount == threshold`, with optional below/equal/above parametrized rows only when expected values are clear.`
- Verify command: `cargo xtask fixtures python_parametrized_boundary_repair_gap`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `advisory` -> `advisory`
- Attempted repair: Sharpened Python boundary repair-card guidance so the equality discriminator remains the minimum repair and optional pytest parameterization is presented only when domain expected values are known.
- Evidence movement: improved: the fixture now emits a canonical Python predicate-boundary gap, repair-card target shape, focused pytest verify command, optional parametrized-row guidance, and an explicit stop condition for unclear below/above expected values.
- Operator note: This is analyzer-route dogfood, not public repair delegation. The preview card is useful guidance, but it remains advisory because the card itself reports receipt status as unavailable_until_python_gap_ledger and does not authorize invented expected values.
- Must not change: `do not promote Python preview cards to public repair packets, do not claim parameterized rows are required for gap closure, do not invent expected output values for below/equal/above rows, do not execute pytest or import the Python project during analysis, do not change badge or CI gate semantics`
- Raw evidence refs: `fixtures/python_parametrized_boundary_repair_gap/SPEC.md, fixtures/python_parametrized_boundary_repair_gap/expected/check.json, fixtures/python_parametrized_boundary_repair_gap/expected/human.txt, crates/ripr/src/output/python_repair_card.rs, EffortlessMetrics/ripr-swarm#764`
- Receipt validation: pass

### Real Repair Attempt `python_preview_boundary_gap_test_only_closed`

- Canonical gap ID: `gap:python:app/pricing.py:calculate_discount:predicate_boundary:amount>=threshold`
- Packet ID: `gap:pr:gap:python:app/pricing.py:calculate_discount:predicate_boundary:amount>=threshold`
- Language: `python`
- Evidence class: `predicate_boundary`
- Source file: `app/pricing.py`
- Target shape: `strengthen the existing pytest test with an exact amount == threshold boundary assertion`
- Verify command: `pytest tests/test_pricing.py::test_calculate_discount_smoke`
- Verify result: `pass`
- Receipt command: `ripr outcome --before fixtures/first_successful_pr/python-preview-gap/inputs/reports/before-check.json --after fixtures/first_successful_pr/python-preview-gap/inputs/reports/after-check.json --format json`
- Receipt path: `fixtures/first_successful_pr/python-preview-gap/expected/outcome/closed.json`
- Receipt state: `receipt_movement_resolved`
- Before/after: `actionable` -> `resolved`
- Attempted repair: A bounded Python packet edited only tests/test_pricing.py, strengthened the weak pytest smoke assertion, and left app/pricing.py forbidden.
- Evidence movement: resolved: ripr outcome compared before/after Python check-output snapshots and reported the canonical predicate-boundary gap closed.
- Operator note: This is static preview repair-routing dogfood: verify evidence and receipt movement are present, but ingest still requires human review and does not claim runtime adequacy or support-tier promotion.
- Must not change: `app/pricing.py, production Python files, preview support tier, CI gate defaults, provider integration, mutation execution`
- Raw evidence refs: `fixtures/first_successful_pr/python-preview-gap/expected/start-here.json, fixtures/first_successful_pr/python-preview-gap/expected/swarm-ingest/closed.json, fixtures/first_successful_pr/python-preview-gap/expected/outcome/closed.json, fixtures/first_successful_pr/python-preview-gap/inputs/reports/before-check.json, fixtures/first_successful_pr/python-preview-gap/inputs/reports/after-check.json`
- Receipt validation: pass

### Real Repair Attempt `local_member_boundary_operand_route_split`

- Canonical gap ID: `gap:activation-boundary-input-unresolved-local-member`
- Packet ID: `limitation:activation_boundary_input_unresolved:local_member:analysis-local-member-boundary-operand-resolution`
- Language: `unspecified`
- Evidence class: `unspecified`
- Source file: `unspecified`
- Target shape: `evidence-record assertions that local/computed predicate-boundary operands containing member access, such as call.name != owner_name, route to a precise analyzer backlog bucket while staying static limitations`
- Verify command: `cargo test -p ripr evidence_record_routes_member_boundary_operands_to_local_member_limitation --lib -- --test-threads=1`
- Verify result: `pass`
- Receipt command: `cargo xtask dogfood`
- Receipt path: `fixtures/real-repair-attempts/corpus.json`
- Receipt state: `receipt_movement_improved`
- Before/after: `static_limitation` -> `static_limitation`
- Attempted repair: Split member-access activation-boundary operands out of the generic local/computed boundary limitation route while preserving non-actionable status.
- Evidence movement: improved: sampled local/member boundary operands now route to analysis/local-member-boundary-operand-resolution with 33 signals, reducing the generic analysis/local-computed-boundary-operand-resolution bucket from 190 to 157 signals.
- Operator note: This is analyzer-route dogfood. It improves work allocation only; it does not infer member values, emit public repair packets, or move badge/gate counters.
- Must not change: `do not invent member or field values, do not promote local/member boundary operands to public repair packets, do not change gate or badge semantics, do not change iterator-derived or closure-derived boundary routes`
- Raw evidence refs: `crates/ripr/src/output/evidence_record.rs::tests::evidence_record_routes_member_boundary_operands_to_local_member_limitation, crates/ripr/src/output/evidence_record.rs::tests::member_boundary_operand_route_requires_identifier_member_tokens, target/ripr/reports/swarm-readiness.json:analysis/local-computed-boundary-operand-resolution, metrics/capabilities.toml:lane1_canonical_actionability_contract`
- Receipt validation: pass

## Python Real-Repo Eval Receipts

These receipts record Python repair-routing runs outside analyzer fixture goldens. They are advisory dogfood evidence for the repair card -> verify -> receipt loop, not support-tier promotion or runtime adequacy proof.

- Default CI blocking: no
- Receipt input: `fixtures/python-real-repo-evals/corpus.json`
- Repair cases: 26; closed gaps: 26; usable recommendations: 26; static-limit no-action cases: 11; ordinary no-action cases: 3

| Case | Repo shape | Source | Gap ID | Verify | Receipt | Movement | Usability |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `tiny_controlled_pytest_boundary_receipt` | `tiny_controlled_pytest` | `scratch_repo` | `gap:python:app/pricing.py:calculate_discount:predicate_boundary:predicate:amount>=threshold` | `pass` | `pass` | `closed` | `usable` |
| `no_config_pyproject_boundary_receipt` | `no_config_pyproject_pytest` | `local_repo` | `gap:python:src/promotions.py:trial_extension:predicate_boundary:predicate:days>=minimum_days` | `pass` | `pass` | `closed` | `usable` |
| `normal_pytest_app_boundary_receipt` | `normal_pytest_app` | `scratch_repo` | `gap:python:app/shipping.py:free_shipping_offer:predicate_boundary:predicate:subtotal>=free_shipping_minimum` | `pass` | `pass` | `closed` | `usable` |
| `src_layout_pytest_boundary_receipt` | `src_layout_package_pytest` | `local_repo` | `gap:python:src/shop_service/pricing.py:loyalty_discount:predicate_boundary:predicate:subtotal>=loyalty_threshold` | `pass` | `pass` | `closed` | `usable` |
| `multi_card_pytest_top3_receipt` | `normal_pytest_app_multi_gap` | `scratch_repo` | `gap:python:app/shipping.py:free_shipping_offer:predicate_boundary:predicate:subtotal>=free_shipping_minimum` | `pass` | `pass` | `closed` | `usable` |
| `async_return_pytest_receipt` | `async_return_pytest` | `scratch_repo` | `gap:python:app/loading.py:load_total:return_value:return_value:await_client.total+2` | `pass` | `pass` | `closed` | `usable` |
| `parametrized_boundary_pytest_receipt` | `parametrized_boundary_pytest` | `scratch_repo` | `gap:python:app/tax.py:tax_band:predicate_boundary:predicate:amount>=threshold` | `pass` | `pass` | `closed` | `usable` |
| `cli_output_pytest_receipt` | `cli_output_pytest` | `scratch_repo` | `gap:python:app/notify.py:notify_shipment:call_or_output_effect:side_effect:print_f_priority_shipping_for_order_id` | `pass` | `pass` | `closed` | `usable` |
| `log_output_pytest_receipt` | `log_output_pytest` | `scratch_repo` | `gap:python:app/notifications.py:warn_coupon:call_or_output_effect:side_effect:logger.warning_coupon_expired` | `pass` | `pass` | `closed` | `usable` |
| `argparse_cli_output_pytest_receipt` | `argparse_cli_output_pytest` | `scratch_repo` | `gap:python:src/commands.py:main:call_or_output_effect:side_effect:print_shipment_queued` | `pass` | `pass` | `closed` | `usable` |
| `click_cli_output_pytest_receipt` | `click_cli_output_pytest` | `scratch_repo` | `gap:python:src/commands.py:ship:call_or_output_effect:side_effect:click.echo_shipment_queued` | `pass` | `pass` | `closed` | `usable` |
| `typer_cli_output_pytest_receipt` | `typer_cli_output_pytest` | `scratch_repo` | `gap:python:src/commands.py:ship:call_or_output_effect:side_effect:typer.echo_shipment_queued` | `pass` | `pass` | `closed` | `usable` |
| `cli_exit_code_pytest_receipt` | `cli_exit_code_pytest` | `scratch_repo` | `gap:python:src/commands.py:ship:call_or_output_effect:side_effect:sys.exit_2` | `pass` | `pass` | `closed` | `usable` |
| `exception_path_pytest_receipt` | `exception_path_pytest` | `scratch_repo` | `gap:python:src/validation.py:require_positive:exception_path:error_path:valueerror_positive_required` | `pass` | `pass` | `closed` | `usable` |
| `custom_exception_pytest_receipt` | `custom_exception_pytest` | `scratch_repo` | `gap:python:app/coupons.py:apply_coupon:exception_path:error_path:expiredcouponerror_coupon_expired` | `pass` | `pass` | `closed` | `usable` |
| `unittest_exception_path_receipt` | `unittest_exception_path` | `scratch_repo` | `gap:python:src/validation.py:require_positive:exception_path:error_path:valueerror_positive_required` | `pass` | `pass` | `closed` | `usable` |
| `api_status_pytest_receipt` | `api_status_pytest` | `scratch_repo` | `gap:python:app/checkout.py:checkout:field_value:field_construction:response.status_code=422` | `pass` | `pass` | `closed` | `usable` |
| `api_json_detail_pytest_receipt` | `api_json_field_pytest` | `scratch_repo` | `gap:python:app/checkout.py:checkout:field_value:field_construction:return_detail_coupon_expired` | `pass` | `pass` | `closed` | `usable` |
| `flask_route_json_detail_pytest_receipt` | `flask_route_json_field_pytest` | `scratch_repo` | `gap:python:app/checkout.py:checkout:field_value:field_construction:return_detail_coupon_expired` | `pass` | `pass` | `closed` | `usable` |
| `fastapi_route_json_detail_pytest_receipt` | `fastapi_route_json_field_pytest` | `scratch_repo` | `gap:python:app/checkout.py:checkout:field_value:field_construction:return_detail_coupon_expired` | `pass` | `pass` | `closed` | `usable` |
| `api_exception_response_pytest_receipt` | `api_exception_response_pytest` | `scratch_repo` | `gap:python:app/checkout.py:checkout:field_value:field_construction:response=response_status_code=422_detail=coupon_expired` | `pass` | `pass` | `closed` | `usable` |
| `mixed_rust_python_pytest_receipt` | `mixed_rust_python_pytest` | `scratch_repo` | `gap:python:app/pricing.py:mixed_repo_discount:predicate_boundary:predicate:amount>=threshold` | `pass` | `pass` | `closed` | `usable` |
| `decorated_route_status_pytest_receipt` | `decorated_route_pytest` | `scratch_repo` | `gap:python:app/checkout.py:checkout:field_value:field_construction:response.status_code=422` | `pass` | `pass` | `closed` | `usable` |
| `unittest_return_value_receipt` | `unittest_service` | `scratch_repo` | `gap:python:app/invoice.py:invoice_total:return_value:return_value:subtotal+tax+service_fee` | `pass` | `pass` | `closed` | `usable` |
| `unittest_dict_field_receipt` | `unittest_service` | `scratch_repo` | `gap:python:src/profiles.py:build_profile:field_value:field_construction:return_name_name_status_active` | `pass` | `pass` | `closed` | `usable` |
| `model_field_pytest_receipt` | `dataclass_model_pytest` | `scratch_repo` | `gap:python:app/users.py:build_user:field_value:field_construction:return_user_active=true` | `pass` | `pass` | `closed` | `usable` |

## Python Repair-Routing Quality Metrics

These metrics are computed from the checked Python real-repo eval receipts. They measure top repair-card usefulness and closure evidence; they are not support-tier promotion and do not make Python gate eligible.

- Quality gate: `pass` - All checked top Python repair cards are usable, verifiable, placed, and receipt-backed without observed false actionability
- Top-3 actionable precision: 29 / 29 ranked findings - fewer-than-three eval outputs must document a stop reason.

- Full top-3 capture cases: 1 / 26 evals - at least one eval must preserve all three ranked repair cards.

- Static-limit no-action cases: 11 - tracked separately from repair-card success metrics.

- Ordinary no-action cases: 3 - tracked separately from repair-card success metrics and from static analyzer limitations.

| Metric | Passing / Checked |
| --- | --- |
| Top-1 actionable precision | 26 / 26 |
| Top-3 actionable precision | 29 / 29 |
| Full top-3 capture cases | 1 / 26 |
| Verify-command validity | 26 / 26 |
| Agent-packet boundary validity | 26 / 26 |
| Concrete discriminator rate | 26 / 26 |
| Related test-location rate | 26 / 26 |
| Receipt closure rate | 26 / 26 |
| False-actionable rate | 0 / 26 |
| Crash rate | 0 / 26 |

| Unsupported limitation | Cases |
| --- | --- |
| `dynamic_route_registration` | 1 |

| No-action static limitation | Cases |
| --- | --- |
| `decorator_indirection` | 2 |
| `dynamic_dispatch` | 1 |
| `generated_file` | 1 |
| `metaprogramming` | 1 |
| `missing_import_graph` | 1 |
| `mocked_module` | 1 |
| `opaque_custom_assertion_helper` | 1 |
| `property_based_test` | 1 |
| `unresolved_pytest_fixture` | 1 |
| `unsupported_syntax` | 1 |

| Ordinary no-action state | Cases |
| --- | --- |
| `already_observed` | 1 |
| `heuristic_only` | 1 |
| `no_related_test` | 1 |

### Python Real-Repo Eval `tiny_controlled_pytest_boundary_receipt`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/tiny-controlled-pytest-receipt`
- Command: `ripr check --root target/ripr/python-real-repo-evals/tiny-controlled-pytest-receipt --base HEAD~1 --format json` (114 ms)
- Top finding: Python predicate-boundary repair card for calculate_discount amount >= threshold
- Changed owner: `python:app/pricing.py::calculate_discount`
- Missing discriminator: `amount == threshold`
- Suggested test: `tests/test_pricing.py` / `test_calculate_discount_smoke`
- Repair card present: true; action: `strengthen_existing_test`
- Agent packet present: true; task: `Strengthen the existing pytest boundary test for amount == threshold.`
- Agent packet command: `ripr agent packet --root target/ripr/python-real-repo-evals/tiny-controlled-pytest-receipt --gap-ledger gap-ledger.json --gap-id gap:python:app/pricing.py:calculate_discount:predicate_boundary:predicate:amount>=threshold --json`
- Agent packet allowed files: `tests/test_pricing.py`
- Agent packet forbidden files: `app/pricing.py`
- Agent packet stop-if: `import cannot be resolved; expected boundary value is ambiguous; production code edit appears necessary`
- Verify command: `pytest tests/test_pricing.py::test_calculate_discount_smoke` (pass) - 1 passed in 0.30s
- After command: `ripr check --root target/ripr/python-real-repo-evals/tiny-controlled-pytest-receipt --base HEAD~1 --format json` (58 ms)
- Receipt command: `ripr outcome --before before-check.json --after after-check.json --format json` (pass)
- Gap movement: `closed`; closed gaps: 1
- False-positive notes: none observed
- Limitation notes: tiny controlled scratch repo only; scoped repair routing is usable alpha while broader Python static facts remain preview/advisory
- Ranked top-3 findings checked: 1
- Ranked top-3 limit reason: Focused eval emitted one Python repair card; no rank 2 or rank 3 repairable Python finding was present.
  - Rank 1: `gap:python:app/pricing.py:calculate_discount:predicate_boundary:predicate:amount>=threshold`; discriminator `amount == threshold`; verify `pytest tests/test_pricing.py::test_calculate_discount_smoke`; usability `usable`
- Claim boundary: `Broader Python static facts remain preview/advisory; No arbitrary imports or tests were run by RIPR; The human-run pytest command verified only the suggested test; No support-tier promotion`
- Reason: The run proves one non-fixture Python PR shape can produce a bounded repair card, pass the verify command, and close the canonical gap through an outcome receipt.
- Receipt validation: pass

### Python Real-Repo Eval `no_config_pyproject_boundary_receipt`

- Source: `local_repo` / `fixtures/python_no_config_repair_gap`
- Command: `ripr check --root fixtures/python_no_config_repair_gap/input --diff fixtures/python_no_config_repair_gap/diff.patch --format json` (64 ms)
- Top finding: Python no-config pyproject predicate-boundary repair card for trial_extension days >= minimum_days
- Changed owner: `python:src/promotions.py::trial_extension`
- Missing discriminator: `days == minimum_days`
- Suggested test: `tests/test_promotions.py` / `test_trial_extension_long_smoke`
- Repair card present: true; action: `strengthen_existing_test`
- Agent packet present: true; task: `Strengthen the existing pytest boundary test for days == minimum_days.`
- Agent packet command: `ripr agent packet --root fixtures/python_no_config_repair_gap/input --gap-ledger gap-ledger.json --gap-id gap:python:src/promotions.py:trial_extension:predicate_boundary:predicate:days>=minimum_days --json`
- Agent packet allowed files: `tests/test_promotions.py`
- Agent packet forbidden files: `src/promotions.py`
- Agent packet stop-if: `Python project marker cannot be resolved without ripr.toml; expected boundary value is ambiguous; production code edit appears necessary`
- Verify command: `pytest tests/test_promotions.py::test_trial_extension_long_smoke` (pass) - 1 passed in 0.41s
- After command: `ripr check --root fixtures/python_no_config_repair_gap/input --diff fixtures/python_no_config_repair_gap/diff.patch --format json` (72 ms)
- Receipt command: `ripr outcome --before before-check.json --after after-check.json --format json` (pass)
- Gap movement: `closed`; closed gaps: 1
- False-positive notes: none observed
- Limitation notes: no-config pyproject pytest fixture proves Python project markers can feed scoped repair routing without ripr.toml; broader Python static facts remain preview/advisory
- Ranked top-3 findings checked: 1
- Ranked top-3 limit reason: Focused no-config eval emitted one Python repair card; no rank 2 or rank 3 repairable Python finding was present.
  - Rank 1: `gap:python:src/promotions.py:trial_extension:predicate_boundary:predicate:days>=minimum_days`; discriminator `days == minimum_days`; verify `pytest tests/test_promotions.py::test_trial_extension_long_smoke`; usability `usable`
- Claim boundary: `Broader Python static facts remain preview/advisory; No arbitrary imports or tests were run by RIPR; The human-run pytest command verified only the suggested test; No support-tier promotion`
- Reason: The run proves a pyproject-based Python repo without ripr.toml can still route one bounded test-only repair, verify it with pytest, and receipt the canonical Python gap as closed.
- Receipt validation: pass

### Python Real-Repo Eval `normal_pytest_app_boundary_receipt`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/normal-pytest-app-boundary-receipt`
- Command: `ripr check --root target/ripr/python-real-repo-evals/normal-pytest-app-boundary-receipt --base HEAD~1 --format json` (47 ms)
- Top finding: Python predicate-boundary repair card for free_shipping_offer subtotal >= free_shipping_minimum
- Changed owner: `python:app/shipping.py::free_shipping_offer`
- Missing discriminator: `subtotal == free_shipping_minimum`
- Suggested test: `tests/test_shipping.py` / `test_free_shipping_high_value_smoke`
- Repair card present: true; action: `strengthen_existing_test`
- Agent packet present: true; task: `Strengthen the existing pytest boundary test for subtotal == free_shipping_minimum.`
- Agent packet command: `ripr agent packet --root target/ripr/python-real-repo-evals/normal-pytest-app-boundary-receipt --gap-ledger gap-ledger.json --gap-id gap:python:app/shipping.py:free_shipping_offer:predicate_boundary:predicate:subtotal>=free_shipping_minimum --json`
- Agent packet allowed files: `tests/test_shipping.py`
- Agent packet forbidden files: `app/shipping.py`
- Agent packet stop-if: `import cannot be resolved; expected boundary value is ambiguous; production code edit appears necessary`
- Verify command: `pytest tests/test_shipping.py::test_free_shipping_high_value_smoke` (pass) - 1 passed in 0.40s
- After command: `ripr check --root target/ripr/python-real-repo-evals/normal-pytest-app-boundary-receipt --base HEAD~1 --format json` (313 ms)
- Receipt command: `ripr outcome --before before-check.json --after after-check.json --format json` (pass)
- Gap movement: `closed`; closed gaps: 1
- False-positive notes: none observed
- Limitation notes: normal pytest app scratch repo with explicit Python preview config; scoped repair routing is usable alpha while broader Python static facts remain preview/advisory
- Ranked top-3 findings checked: 1
- Ranked top-3 limit reason: Focused eval emitted one Python repair card; no rank 2 or rank 3 repairable Python finding was present.
  - Rank 1: `gap:python:app/shipping.py:free_shipping_offer:predicate_boundary:predicate:subtotal>=free_shipping_minimum`; discriminator `subtotal == free_shipping_minimum`; verify `pytest tests/test_shipping.py::test_free_shipping_high_value_smoke`; usability `usable`
- Claim boundary: `Broader Python static facts remain preview/advisory; No arbitrary imports or tests were run by RIPR; The human-run pytest command verified only the suggested test; No support-tier promotion`
- Reason: The run proves a pyproject-based pytest app can produce a bounded strengthen-existing-test repair card, pass the verify command, and close the canonical Python gap through an outcome receipt.
- Receipt validation: pass

### Python Real-Repo Eval `src_layout_pytest_boundary_receipt`

- Source: `local_repo` / `fixtures/python_src_layout_repair_gap`
- Command: `ripr check --root fixtures/python_src_layout_repair_gap/input --diff fixtures/python_src_layout_repair_gap/diff.patch --format json` (428 ms)
- Top finding: Python src-layout package predicate-boundary repair card for loyalty_discount subtotal >= loyalty_threshold
- Changed owner: `python:src/shop_service/pricing.py::loyalty_discount`
- Missing discriminator: `subtotal == loyalty_threshold`
- Suggested test: `tests/test_pricing.py` / `test_loyalty_discount_high_value_smoke`
- Repair card present: true; action: `strengthen_existing_test`
- Agent packet present: true; task: `Strengthen the existing pytest boundary test for subtotal == loyalty_threshold.`
- Agent packet command: `ripr agent packet --root fixtures/python_src_layout_repair_gap/input --gap-ledger gap-ledger.json --gap-id gap:python:src/shop_service/pricing.py:loyalty_discount:predicate_boundary:predicate:subtotal>=loyalty_threshold --json`
- Agent packet allowed files: `tests/test_pricing.py`
- Agent packet forbidden files: `src/shop_service/pricing.py`
- Agent packet stop-if: `import cannot be resolved through the src-layout package path; expected boundary value is ambiguous; production code edit appears necessary`
- Verify command: `pytest tests/test_pricing.py::test_loyalty_discount_high_value_smoke` (pass) - 1 passed in 0.34s
- After command: `ripr check --root fixtures/python_src_layout_repair_gap/input --diff fixtures/python_src_layout_repair_gap/diff.patch --format json` (71 ms)
- Receipt command: `ripr outcome --before before-check.json --after after-check.json --format json` (pass)
- Gap movement: `closed`; closed gaps: 1
- False-positive notes: none observed
- Limitation notes: src-layout package fixture records external-repo-style post-promotion stability evidence; scoped repair routing is usable alpha while broader Python static facts remain preview/advisory
- Ranked top-3 findings checked: 1
- Ranked top-3 limit reason: Focused src-layout eval emitted one Python repair card; no rank 2 or rank 3 repairable Python finding was present.
  - Rank 1: `gap:python:src/shop_service/pricing.py:loyalty_discount:predicate_boundary:predicate:subtotal>=loyalty_threshold`; discriminator `subtotal == loyalty_threshold`; verify `pytest tests/test_pricing.py::test_loyalty_discount_high_value_smoke`; usability `usable`
- Claim boundary: `Broader Python static facts remain preview/advisory; No arbitrary imports or tests were run by RIPR; The human-run pytest command verified only the suggested test; No support-tier promotion`
- Reason: The run extends post-usable-alpha stability evidence to an external-repo-style src-layout package: RIPR routes one bounded test-only repair, verifies it with pytest, and receipts the canonical Python gap as closed.
- Receipt validation: pass

### Python Real-Repo Eval `multi_card_pytest_top3_receipt`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/multi-card-pytest-top3-receipt`
- Command: `ripr check --root target/ripr/python-real-repo-evals/multi-card-pytest-top3-receipt --base HEAD~1 --format json` (92 ms)
- Top finding: Python top-3 repair-card capture with boundary, return-value, and exception-path gaps
- Changed owner: `python:app/shipping.py::free_shipping_offer`
- Missing discriminator: `subtotal == free_shipping_minimum`
- Suggested test: `tests/test_shipping.py` / `test_free_shipping_high_value_smoke`
- Repair card present: true; action: `strengthen_existing_test`
- Agent packet present: true; task: `Strengthen the existing pytest boundary test for subtotal == free_shipping_minimum.`
- Agent packet command: `ripr agent packet --root target/ripr/python-real-repo-evals/multi-card-pytest-top3-receipt --gap-ledger gap-ledger.json --gap-id gap:python:app/shipping.py:free_shipping_offer:predicate_boundary:predicate:subtotal>=free_shipping_minimum --json`
- Agent packet allowed files: `tests/test_shipping.py`
- Agent packet forbidden files: `app/shipping.py`
- Agent packet stop-if: `import cannot be resolved; expected boundary value is ambiguous; production code edit appears necessary`
- Verify command: `pytest tests/test_shipping.py::test_free_shipping_high_value_smoke` (pass) - 1 passed in 0.36s
- After command: `ripr check --root target/ripr/python-real-repo-evals/multi-card-pytest-top3-receipt --base HEAD~1 --format json` (74 ms)
- Receipt command: `ripr outcome --before before-check.json --after after-check.json --format json` (pass)
- Gap movement: `closed`; closed gaps: 1
- False-positive notes: none observed
- Limitation notes: multi-gap scratch pytest app; only the top packet was attempted, while ranks 2 and 3 remain advisory repair-card precision evidence
- Ranked top-3 findings checked: 3
  - Rank 1: `gap:python:app/shipping.py:free_shipping_offer:predicate_boundary:predicate:subtotal>=free_shipping_minimum`; discriminator `subtotal == free_shipping_minimum`; verify `pytest tests/test_shipping.py::test_free_shipping_high_value_smoke`; usability `usable`
  - Rank 2: `gap:python:app/invoice.py:invoice_total:return_value:return_value:subtotal+tax+service_fee`; discriminator `return value == subtotal + tax + service_fee`; verify `pytest tests/test_invoice.py::test_invoice_total_smoke`; usability `usable`
  - Rank 3: `gap:python:app/validation.py:require_positive:exception_path:error_path:valueerror_positive_required`; discriminator `raises ValueError matching "positive required"`; verify `pytest tests/test_validation.py::test_rejects_zero_amount`; usability `usable`
- Claim boundary: `Broader Python static facts remain preview/advisory; No arbitrary imports or tests were run by RIPR; The human-run pytest command verified only the rank 1 suggested test; No support-tier promotion`
- Reason: The run proves the dogfood metric can preserve a full top-3 repair-card capture while delegating only the top packet and receipting that single canonical gap.
- Receipt validation: pass

### Python Real-Repo Eval `async_return_pytest_receipt`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/async-return-receipt`
- Command: `ripr check --root target/ripr/python-real-repo-evals/async-return-receipt --base HEAD~1 --format json` (481 ms)
- Top finding: Python async return-value repair card for load_total await client.total() + 2
- Changed owner: `python:app/loading.py::load_total`
- Missing discriminator: `return value == await client.total() + 2`
- Suggested test: `tests/test_loading.py` / `test_load_total_smoke`
- Repair card present: true; action: `strengthen_existing_test`
- Agent packet present: true; task: `Strengthen the existing pytest async-return test with an exact return-value assertion.`
- Agent packet command: `ripr agent packet --root target/ripr/python-real-repo-evals/async-return-receipt --gap-ledger gap-ledger.json --gap-id gap:python:app/loading.py:load_total:return_value:return_value:await_client.total+2 --json`
- Agent packet allowed files: `tests/test_loading.py`
- Agent packet forbidden files: `app/loading.py`
- Agent packet stop-if: `import cannot be resolved; expected async return value is ambiguous; production code edit appears necessary`
- Verify command: `pytest tests/test_loading.py::test_load_total_smoke` (pass) - 1 passed in 0.56s; scratch verification used asyncio.run in a normal pytest test instead of requiring pytest-asyncio
- After command: `ripr check --root target/ripr/python-real-repo-evals/async-return-receipt --base HEAD~1 --format json` (4084 ms)
- Receipt command: `ripr outcome --before before-check.json --after after-check.json --format json` (pass)
- Gap movement: `closed`; closed gaps: 1
- False-positive notes: none observed
- Limitation notes: async-return scratch repo keeps RIPR static/advisory: the human-run pytest command used asyncio.run for verification, while RIPR did not execute imports, tests, or event-loop behavior
- Ranked top-3 findings checked: 1
- Ranked top-3 limit reason: Focused async-return eval emitted one Python repair card; no rank 2 or rank 3 repairable Python finding was present.
  - Rank 1: `gap:python:app/loading.py:load_total:return_value:return_value:await_client.total+2`; discriminator `return value == await client.total() + 2`; verify `pytest tests/test_loading.py::test_load_total_smoke`; usability `usable`
- Claim boundary: `Broader Python static facts remain preview/advisory; No arbitrary imports or tests were run by RIPR; The human-run pytest command verified only the suggested test; Scratch verification used asyncio.run in a normal pytest test instead of requiring pytest-asyncio; No support-tier promotion`
- Reason: The run extends post-usable-alpha stability evidence to an async Python owner: RIPR detects the async return-value change, routes a weak asyncio.run pytest smoke assertion to a bounded strengthen-existing-test packet, verifies with focused pytest, and closes the canonical Python return-value gap through an outcome receipt.
- Receipt validation: pass

### Python Real-Repo Eval `parametrized_boundary_pytest_receipt`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/parametrized-boundary-receipt`
- Command: `ripr check --root target/ripr/python-real-repo-evals/parametrized-boundary-receipt --base HEAD~1 --format json` (1979 ms)
- Top finding: Python parametrized boundary repair card for tax_band amount >= threshold
- Changed owner: `python:app/tax.py::tax_band`
- Missing discriminator: `amount == threshold`
- Suggested test: `tests/test_tax.py` / `test_tax_band_tiers`
- Repair card present: true; action: `strengthen_existing_test`
- Agent packet present: true; task: `Strengthen the existing pytest parametrized boundary test for amount == threshold.`
- Agent packet command: `ripr agent packet --root target/ripr/python-real-repo-evals/parametrized-boundary-receipt --gap-ledger gap-ledger.json --gap-id gap:python:app/tax.py:tax_band:predicate_boundary:predicate:amount>=threshold --json`
- Agent packet allowed files: `tests/test_tax.py`
- Agent packet forbidden files: `app/tax.py`
- Agent packet stop-if: `import cannot be resolved; expected boundary value is ambiguous; production code edit appears necessary; below/above parameterized row expected values are unclear`
- Verify command: `pytest tests/test_tax.py::test_tax_band_tiers` (pass) - 3 passed in 0.49s
- After command: `ripr check --root target/ripr/python-real-repo-evals/parametrized-boundary-receipt --base HEAD~1 --format json` (235 ms)
- Receipt command: `ripr outcome --before before-check.json --after after-check.json --format json` (pass)
- Gap movement: `closed`; closed gaps: 1
- False-positive notes: none observed
- Limitation notes: parametrized-boundary scratch repo records optional pytest parameterized shape; scoped repair routing is usable alpha while broader Python static facts remain preview/advisory
- Ranked top-3 findings checked: 1
- Ranked top-3 limit reason: Focused parametrized-boundary eval emitted one Python repair card; no rank 2 or rank 3 repairable Python finding was present.
  - Rank 1: `gap:python:app/tax.py:tax_band:predicate_boundary:predicate:amount>=threshold`; discriminator `amount == threshold`; verify `pytest tests/test_tax.py::test_tax_band_tiers`; usability `usable`
- Claim boundary: `Broader Python static facts remain preview/advisory; No arbitrary imports or tests were run by RIPR; The human-run pytest command verified only the suggested test; No support-tier promotion`
- Reason: The run extends post-usable-alpha stability evidence to a native pytest parameterized boundary repair: RIPR routes the equality-boundary discriminator to one existing parameterized test, exports a bounded test-only packet, verifies with focused pytest, and closes the canonical Python gap through an outcome receipt.
- Receipt validation: pass

### Python Real-Repo Eval `cli_output_pytest_receipt`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/cli-output-receipt`
- Command: `ripr check --root target/ripr/python-real-repo-evals/cli-output-receipt --base HEAD~1 --format json` (82 ms)
- Top finding: Python output/call-effect repair card for notify_shipment priority shipping output
- Changed owner: `python:app/notify.py::notify_shipment`
- Missing discriminator: `output contains "priority shipping for {order_id}"`
- Suggested test: `tests/test_notify.py` / `test_notify_expedited_smoke`
- Repair card present: true; action: `strengthen_existing_test`
- Agent packet present: true; task: `Strengthen the existing pytest CLI/output test with an exact priority-shipping output assertion.`
- Agent packet command: `ripr agent packet --root target/ripr/python-real-repo-evals/cli-output-receipt --gap-ledger gap-ledger.json --gap-id gap:python:app/notify.py:notify_shipment:call_or_output_effect:side_effect:print_f_priority_shipping_for_order_id --json`
- Agent packet allowed files: `tests/test_notify.py`
- Agent packet forbidden files: `app/notify.py`
- Agent packet stop-if: `import cannot be resolved; expected output text is ambiguous; production code edit appears necessary`
- Verify command: `pytest tests/test_notify.py::test_notify_expedited_smoke` (pass) - 1 passed in 0.34s
- After command: `ripr check --root target/ripr/python-real-repo-evals/cli-output-receipt --base HEAD~1 --format json` (54 ms)
- Receipt command: `ripr outcome --before before-check.json --after after-check.json --format json` (pass)
- Gap movement: `closed`; closed gaps: 1
- False-positive notes: none observed
- Limitation notes: simple pytest CLI/output scratch repo with capsys evidence; scoped repair routing is usable alpha while broader Python static facts remain preview/advisory
- Ranked top-3 findings checked: 1
- Ranked top-3 limit reason: Focused eval emitted one Python repair card; no rank 2 or rank 3 repairable Python finding was present.
  - Rank 1: `gap:python:app/notify.py:notify_shipment:call_or_output_effect:side_effect:print_f_priority_shipping_for_order_id`; discriminator `output contains "priority shipping for {order_id}"`; verify `pytest tests/test_notify.py::test_notify_expedited_smoke`; usability `usable`
- Claim boundary: `Broader Python static facts remain preview/advisory; No arbitrary imports or tests were run by RIPR; The human-run pytest command verified only the suggested test; No support-tier promotion`
- Reason: The run proves a Python CLI/output-style change can produce a bounded output/call-effect repair card, pass the focused pytest verify command, and close the canonical Python gap through an outcome receipt.
- Receipt validation: pass

### Python Real-Repo Eval `log_output_pytest_receipt`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/log-output-receipt`
- Command: `ripr check --root target/ripr/python-real-repo-evals/log-output-receipt --base HEAD~1 --format json` (900 ms)
- Top finding: Python log-output repair card for warn_coupon coupon-expired warning
- Changed owner: `python:app/notifications.py::warn_coupon`
- Missing discriminator: `log contains "coupon expired"`
- Suggested test: `tests/test_notifications.py` / `test_warn_coupon_logs_expired_message`
- Repair card present: true; action: `strengthen_existing_test`
- Agent packet present: true; task: `Strengthen the existing pytest caplog assertion for log contains "coupon expired".`
- Agent packet command: `ripr agent packet --root target/ripr/python-real-repo-evals/log-output-receipt --gap-ledger gap-ledger.json --gap-id gap:python:app/notifications.py:warn_coupon:call_or_output_effect:side_effect:logger.warning_coupon_expired --json`
- Agent packet allowed files: `tests/test_notifications.py`
- Agent packet forbidden files: `app/notifications.py`
- Agent packet stop-if: `import cannot be resolved; expected log text is ambiguous; production code edit appears necessary`
- Verify command: `pytest tests/test_notifications.py::test_warn_coupon_logs_expired_message` (pass) - 1 passed in 2.87s; scratch verification set the caplog handler formatter in the test so exact static log evidence matched pytest runtime output without editing production code
- After command: `ripr check --root target/ripr/python-real-repo-evals/log-output-receipt --base HEAD~1 --format json` (900 ms)
- Receipt command: `ripr outcome --before before-check.json --after after-check.json --format json` (pass)
- Gap movement: `closed`; closed gaps: 1
- False-positive notes: none observed
- Limitation notes: pytest caplog scratch repo with a logging.warning side effect; RIPR did not import logging, execute tests, or format runtime logs during analysis
- Ranked top-3 findings checked: 1
- Ranked top-3 limit reason: Focused log-output eval emitted one Python repair card; no rank 2 or rank 3 repairable Python finding was present.
  - Rank 1: `gap:python:app/notifications.py:warn_coupon:call_or_output_effect:side_effect:logger.warning_coupon_expired`; discriminator `log contains "coupon expired"`; verify `pytest tests/test_notifications.py::test_warn_coupon_logs_expired_message`; usability `usable`
- Claim boundary: `Broader Python static facts remain preview/advisory; No arbitrary imports or tests were run by RIPR; The human-run pytest command verified only the suggested test; Human-run pytest configured caplog formatting in the test; RIPR did not execute logging; No support-tier promotion`
- Reason: The run extends post-usable-alpha stability evidence to a pytest caplog repair: RIPR routes the changed logger.warning output to one existing pytest test, exports a bounded test-only packet, verifies with focused pytest, and closes the canonical Python call/output-effect gap through an outcome receipt.
- Receipt validation: pass

### Python Real-Repo Eval `argparse_cli_output_pytest_receipt`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/argparse-cli-output-receipt`
- Command: `ripr check --root target/ripr/python-real-repo-evals/argparse-cli-output-receipt --base HEAD~1 --format json` (1100 ms)
- Top finding: Python argparse CLI output repair card for main command output
- Changed owner: `python:src/commands.py::main`
- Missing discriminator: `output contains "shipment queued"`
- Suggested test: `tests/test_commands.py` / `test_main_smoke`
- Repair card present: true; action: `strengthen_existing_test`
- Agent packet present: true; task: `Strengthen the existing pytest argparse CLI output assertion for output contains "shipment queued".`
- Agent packet command: `ripr agent packet --root target/ripr/python-real-repo-evals/argparse-cli-output-receipt --gap-ledger gap-ledger.json --gap-id gap:python:src/commands.py:main:call_or_output_effect:side_effect:print_shipment_queued --json`
- Agent packet allowed files: `tests/test_commands.py`
- Agent packet forbidden files: `src/commands.py`
- Agent packet stop-if: `import cannot be resolved; expected output text is ambiguous; production code edit appears necessary`
- Verify command: `pytest tests/test_commands.py::test_main_smoke` (pass) - 1 passed in 0.63s; scratch verification used stdlib argparse through the focused test while RIPR itself did not import argparse or execute command parsing
- After command: `ripr check --root target/ripr/python-real-repo-evals/argparse-cli-output-receipt --base HEAD~1 --format json` (900 ms)
- Receipt command: `ripr outcome --before before-check.json --after after-check.json --format json` (pass)
- Gap movement: `closed`; closed gaps: 1
- False-positive notes: none observed
- Limitation notes: argparse-shaped pytest CLI output scratch repo with capsys evidence; RIPR did not import argparse, execute parser setup, or require external dependencies
- Ranked top-3 findings checked: 1
- Ranked top-3 limit reason: Focused argparse CLI output eval emitted one Python repair card; no rank 2 or rank 3 repairable Python finding was present.
  - Rank 1: `gap:python:src/commands.py:main:call_or_output_effect:side_effect:print_shipment_queued`; discriminator `output contains "shipment queued"`; verify `pytest tests/test_commands.py::test_main_smoke`; usability `usable`
- Claim boundary: `Broader Python static facts remain preview/advisory; No arbitrary imports or tests were run by RIPR; The human-run pytest command verified only the suggested test; Human-run pytest exercised stdlib argparse; RIPR did not import argparse or execute command parsing; No support-tier promotion`
- Reason: The run extends post-usable-alpha stability evidence to an argparse-shaped Python CLI output repair: RIPR routes the changed print output to one existing pytest test, exports a bounded test-only packet, verifies with focused pytest, and closes the canonical Python call/output-effect gap through an outcome receipt.
- Receipt validation: pass

### Python Real-Repo Eval `click_cli_output_pytest_receipt`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/click-cli-output-receipt`
- Command: `ripr check --root target/ripr/python-real-repo-evals/click-cli-output-receipt --base HEAD~1 --format json` (809 ms)
- Top finding: Python Click CLI output repair card for ship command output
- Changed owner: `python:src/commands.py::ship`
- Missing discriminator: `output contains "shipment queued"`
- Suggested test: `tests/test_commands.py` / `test_ship_output_smoke`
- Repair card present: true; action: `strengthen_existing_test`
- Agent packet present: true; task: `Strengthen the existing pytest Click CLI output assertion for output contains "shipment queued".`
- Agent packet command: `ripr agent packet --root target/ripr/python-real-repo-evals/click-cli-output-receipt --gap-ledger gap-ledger.json --gap-id gap:python:src/commands.py:ship:call_or_output_effect:side_effect:click.echo_shipment_queued --json`
- Agent packet allowed files: `tests/test_commands.py`
- Agent packet forbidden files: `src/commands.py`
- Agent packet stop-if: `import cannot be resolved; expected output text is ambiguous; production code edit appears necessary`
- Verify command: `pytest tests/test_commands.py::test_ship_output_smoke` (pass) - 1 passed in 0.37s; scratch verification set PYTHONPATH=. and used a local click.py stub instead of installing Click
- After command: `ripr check --root target/ripr/python-real-repo-evals/click-cli-output-receipt --base HEAD~1 --format json` (522 ms)
- Receipt command: `ripr outcome --before before-check.json --after after-check.json --format json` (pass)
- Gap movement: `closed`; closed gaps: 1
- False-positive notes: none observed
- Limitation notes: Click-shaped pytest CLI output scratch repo with a local click.py stub for verification; RIPR did not import Click, execute command decorators, or require external dependencies
- Ranked top-3 findings checked: 1
- Ranked top-3 limit reason: Focused Click CLI output eval emitted one Python repair card; no rank 2 or rank 3 repairable Python finding was present.
  - Rank 1: `gap:python:src/commands.py:ship:call_or_output_effect:side_effect:click.echo_shipment_queued`; discriminator `output contains "shipment queued"`; verify `pytest tests/test_commands.py::test_ship_output_smoke`; usability `usable`
- Claim boundary: `Broader Python static facts remain preview/advisory; No arbitrary imports or tests were run by RIPR; The human-run pytest command verified only the suggested test; Scratch verification used a local Click-shaped stub instead of adding a dependency; No support-tier promotion`
- Reason: The run extends post-usable-alpha stability evidence to a Click-shaped Python CLI output repair: RIPR routes the changed click.echo output to one existing pytest test, exports a bounded test-only packet, verifies with focused pytest, and closes the canonical Python call/output-effect gap through an outcome receipt.
- Receipt validation: pass

### Python Real-Repo Eval `typer_cli_output_pytest_receipt`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/typer-cli-output-receipt`
- Command: `ripr check --root target/ripr/python-real-repo-evals/typer-cli-output-receipt --base HEAD~1 --format json` (174 ms)
- Top finding: Python Typer CLI output repair card for ship command output
- Changed owner: `python:src/commands.py::ship`
- Missing discriminator: `output contains "shipment queued"`
- Suggested test: `tests/test_commands.py` / `test_ship_output_smoke`
- Repair card present: true; action: `strengthen_existing_test`
- Agent packet present: true; task: `Strengthen the existing pytest Typer CLI output assertion for output contains "shipment queued".`
- Agent packet command: `ripr agent packet --root target/ripr/python-real-repo-evals/typer-cli-output-receipt --gap-ledger gap-ledger.json --gap-id gap:python:src/commands.py:ship:call_or_output_effect:side_effect:typer.echo_shipment_queued --json`
- Agent packet allowed files: `tests/test_commands.py`
- Agent packet forbidden files: `src/commands.py`
- Agent packet stop-if: `import cannot be resolved; expected output text is ambiguous; production code edit appears necessary`
- Verify command: `pytest tests/test_commands.py::test_ship_output_smoke` (pass) - 1 passed in 0.49s; scratch verification set PYTHONPATH=. and used a local typer.py stub instead of installing Typer
- After command: `ripr check --root target/ripr/python-real-repo-evals/typer-cli-output-receipt --base HEAD~1 --format json` (1426 ms)
- Receipt command: `ripr outcome --before before-check.json --after after-check.json --format json` (pass)
- Gap movement: `closed`; closed gaps: 1
- False-positive notes: none observed
- Limitation notes: Typer-shaped pytest CLI output scratch repo with a local typer.py stub for verification; RIPR did not import Typer, execute app.command decorators, or require external dependencies
- Ranked top-3 findings checked: 1
- Ranked top-3 limit reason: Focused Typer CLI output eval emitted one Python repair card; no rank 2 or rank 3 repairable Python finding was present.
  - Rank 1: `gap:python:src/commands.py:ship:call_or_output_effect:side_effect:typer.echo_shipment_queued`; discriminator `output contains "shipment queued"`; verify `pytest tests/test_commands.py::test_ship_output_smoke`; usability `usable`
- Claim boundary: `Broader Python static facts remain preview/advisory; No arbitrary imports or tests were run by RIPR; The human-run pytest command verified only the suggested test; Scratch verification used a local Typer-shaped stub instead of adding a dependency; No support-tier promotion`
- Reason: The run extends post-usable-alpha stability evidence to a Typer-shaped Python CLI output repair: RIPR routes the changed typer.echo output to one existing pytest test, exports a bounded test-only packet, verifies with focused pytest, and closes the canonical Python call/output-effect gap through an outcome receipt.
- Receipt validation: pass

### Python Real-Repo Eval `cli_exit_code_pytest_receipt`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/cli-exit-code-receipt`
- Command: `ripr check --root target/ripr/python-real-repo-evals/cli-exit-code-receipt --base HEAD~1 --format json` (1376 ms)
- Top finding: Python CLI exit-code repair card for ship failure exit status
- Changed owner: `python:src/commands.py::ship`
- Missing discriminator: `exit_code == 2`
- Suggested test: `tests/test_commands.py` / `test_ship_failure_smoke`
- Repair card present: true; action: `strengthen_existing_test`
- Agent packet present: true; task: `Strengthen the existing pytest CLI exit-code assertion for exit_code == 2.`
- Agent packet command: `ripr agent packet --root target/ripr/python-real-repo-evals/cli-exit-code-receipt --gap-ledger gap-ledger.json --gap-id gap:python:src/commands.py:ship:call_or_output_effect:side_effect:sys.exit_2 --json`
- Agent packet allowed files: `tests/test_commands.py`
- Agent packet forbidden files: `src/commands.py`
- Agent packet stop-if: `import cannot be resolved; expected exit code is ambiguous; production code edit appears necessary`
- Verify command: `pytest tests/test_commands.py::test_ship_failure_smoke` (pass) - 1 passed in 0.38s; scratch verification set PYTHONPATH=. so pytest could import the local src namespace
- After command: `ripr check --root target/ripr/python-real-repo-evals/cli-exit-code-receipt --base HEAD~1 --format json` (313 ms)
- Receipt command: `ripr outcome --before before-check.json --after after-check.json --format json` (pass)
- Gap movement: `closed`; closed gaps: 1
- False-positive notes: none observed
- Limitation notes: simple pytest CLI exit-code scratch repo with broad SystemExit evidence; focused verify needed PYTHONPATH=. for the scratch src namespace, while the analyzer-suggested command remains the same bounded pytest node
- Ranked top-3 findings checked: 1
- Ranked top-3 limit reason: Focused eval emitted one Python repair card; no rank 2 or rank 3 repairable Python finding was present.
  - Rank 1: `gap:python:src/commands.py:ship:call_or_output_effect:side_effect:sys.exit_2`; discriminator `exit_code == 2`; verify `pytest tests/test_commands.py::test_ship_failure_smoke`; usability `usable`
- Claim boundary: `Broader Python static facts remain preview/advisory; No arbitrary imports or tests were run by RIPR; The human-run pytest command verified only the suggested test; No support-tier promotion`
- Reason: The run proves a Python CLI exit-code change can produce a bounded output/call-effect repair card, pass the focused pytest verify command, and close the canonical Python gap through an outcome receipt.
- Receipt validation: pass

### Python Real-Repo Eval `exception_path_pytest_receipt`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/exception-path-receipt`
- Command: `ripr check --root target/ripr/python-real-repo-evals/exception-path-receipt --base HEAD~1 --format json` (94 ms)
- Top finding: Python exception-path repair card for require_positive ValueError message
- Changed owner: `python:src/validation.py::require_positive`
- Missing discriminator: `raises ValueError matching "positive required"`
- Suggested test: `tests/test_validation.py` / `test_rejects_zero_value`
- Repair card present: true; action: `strengthen_existing_test`
- Agent packet present: true; task: `Strengthen the existing pytest exception test with pytest.raises(ValueError, match="positive required").`
- Agent packet command: `ripr agent packet --root target/ripr/python-real-repo-evals/exception-path-receipt --gap-ledger gap-ledger.json --gap-id gap:python:src/validation.py:require_positive:exception_path:error_path:valueerror_positive_required --json`
- Agent packet allowed files: `tests/test_validation.py`
- Agent packet forbidden files: `src/validation.py`
- Agent packet stop-if: `import cannot be resolved; expected exception message is ambiguous; production code edit appears necessary`
- Verify command: `pytest tests/test_validation.py::test_rejects_zero_value` (pass) - 1 passed in 0.29s
- After command: `ripr check --root target/ripr/python-real-repo-evals/exception-path-receipt --base HEAD~1 --format json` (61 ms)
- Receipt command: `ripr outcome --before before-check.json --after after-check.json --format json` (pass)
- Gap movement: `closed`; closed gaps: 1
- False-positive notes: none observed
- Limitation notes: lightweight pytest exception-path scratch repo with exact pytest.raises match evidence; scoped repair routing is usable alpha while broader Python static facts remain preview/advisory
- Ranked top-3 findings checked: 1
- Ranked top-3 limit reason: Focused exception-path eval emitted one Python repair card; no rank 2 or rank 3 repairable Python finding was present.
  - Rank 1: `gap:python:src/validation.py:require_positive:exception_path:error_path:valueerror_positive_required`; discriminator `raises ValueError matching "positive required"`; verify `pytest tests/test_validation.py::test_rejects_zero_value`; usability `usable`
- Claim boundary: `Broader Python static facts remain preview/advisory; No arbitrary imports or tests were run by RIPR; The human-run pytest command verified only the suggested test; No support-tier promotion`
- Reason: The run extends post-usable-alpha stability evidence to a Python exception-path repair: RIPR routes the missing exact ValueError message discriminator to one existing pytest method, exports a bounded test-only packet, verifies with pytest, and closes the canonical Python gap through an outcome receipt.
- Receipt validation: pass

### Python Real-Repo Eval `custom_exception_pytest_receipt`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/custom-exception-receipt`
- Command: `ripr check --root target/ripr/python-real-repo-evals/custom-exception-receipt --base HEAD~1 --format json` (1000 ms)
- Top finding: Python custom exception-path repair card for apply_coupon ExpiredCouponError message
- Changed owner: `python:app/coupons.py::apply_coupon`
- Missing discriminator: `raises ExpiredCouponError matching "coupon expired"`
- Suggested test: `tests/test_coupons.py` / `test_expired_coupon_rejected`
- Repair card present: true; action: `strengthen_existing_test`
- Agent packet present: true; task: `Strengthen the existing pytest custom-exception test with pytest.raises(ExpiredCouponError, match="coupon expired").`
- Agent packet command: `ripr agent packet --root target/ripr/python-real-repo-evals/custom-exception-receipt --gap-ledger gap-ledger.json --gap-id gap:python:app/coupons.py:apply_coupon:exception_path:error_path:expiredcouponerror_coupon_expired --json`
- Agent packet allowed files: `tests/test_coupons.py`
- Agent packet forbidden files: `app/coupons.py`
- Agent packet stop-if: `import cannot be resolved; expected exception message is ambiguous; production code edit appears necessary`
- Verify command: `pytest tests/test_coupons.py::test_expired_coupon_rejected` (pass) - 1 passed in 0.33s
- After command: `ripr check --root target/ripr/python-real-repo-evals/custom-exception-receipt --base HEAD~1 --format json` (800 ms)
- Receipt command: `ripr outcome --before before-check.json --after after-check.json --format json` (pass)
- Gap movement: `closed`; closed gaps: 1
- False-positive notes: none observed
- Limitation notes: custom exception-path scratch repo includes baseline pytest import setup; scoped repair routing is usable alpha while broader Python static facts remain preview/advisory
- Ranked top-3 findings checked: 1
- Ranked top-3 limit reason: Focused custom-exception eval emitted one Python repair card; no rank 2 or rank 3 repairable Python finding was present.
  - Rank 1: `gap:python:app/coupons.py:apply_coupon:exception_path:error_path:expiredcouponerror_coupon_expired`; discriminator `raises ExpiredCouponError matching "coupon expired"`; verify `pytest tests/test_coupons.py::test_expired_coupon_rejected`; usability `usable`
- Claim boundary: `Broader Python static facts remain preview/advisory; No arbitrary imports or tests were run by RIPR; The human-run pytest command verified only the suggested test; No support-tier promotion`
- Reason: The run extends post-usable-alpha stability evidence to a custom Python exception type: RIPR routes the missing exact ExpiredCouponError message discriminator to one existing pytest method, exports a bounded test-only packet, verifies with pytest, and closes the canonical Python gap through an outcome receipt.
- Receipt validation: pass

### Python Real-Repo Eval `unittest_exception_path_receipt`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/unittest-exception-receipt`
- Command: `ripr check --root target/ripr/python-real-repo-evals/unittest-exception-receipt --base HEAD~1 --format json` (118 ms)
- Top finding: Python unittest exception-path repair card for require_positive ValueError message
- Changed owner: `python:src/validation.py::require_positive`
- Missing discriminator: `raises ValueError matching "positive required"`
- Suggested test: `tests/test_validation.py` / `test_rejects_zero_value`
- Repair card present: true; action: `strengthen_existing_test`
- Agent packet present: true; task: `Strengthen the existing unittest exception test with self.assertRaisesRegex(ValueError, "positive required").`
- Agent packet command: `ripr agent packet --root target/ripr/python-real-repo-evals/unittest-exception-receipt --gap-ledger gap-ledger.json --gap-id gap:python:src/validation.py:require_positive:exception_path:error_path:valueerror_positive_required --json`
- Agent packet allowed files: `tests/test_validation.py`
- Agent packet forbidden files: `src/validation.py`
- Agent packet stop-if: `import cannot be resolved; expected exception message is ambiguous; production code edit appears necessary`
- Verify command: `python -m unittest tests.test_validation.TestValidation.test_rejects_zero_value` (pass) - 1 unittest test passed in 0.00s
- After command: `ripr check --root target/ripr/python-real-repo-evals/unittest-exception-receipt --base HEAD~1 --format json` (74 ms)
- Receipt command: `ripr outcome --before before-check.json --after after-check.json --format json` (pass)
- Gap movement: `closed`; closed gaps: 1
- False-positive notes: none observed
- Limitation notes: lightweight unittest exception-path scratch repo with exact assertRaisesRegex evidence; scoped repair routing is usable alpha while broader Python static facts remain preview/advisory
- Ranked top-3 findings checked: 1
- Ranked top-3 limit reason: Focused unittest exception-path eval emitted one Python repair card; no rank 2 or rank 3 repairable Python finding was present.
  - Rank 1: `gap:python:src/validation.py:require_positive:exception_path:error_path:valueerror_positive_required`; discriminator `raises ValueError matching "positive required"`; verify `python -m unittest tests.test_validation.TestValidation.test_rejects_zero_value`; usability `usable`
- Claim boundary: `Broader Python static facts remain preview/advisory; No arbitrary imports or tests were run by RIPR; The human-run unittest command verified only the suggested test; No support-tier promotion`
- Reason: The run extends post-usable-alpha stability evidence to a Python unittest exception-path repair: RIPR routes the missing exact ValueError message discriminator to one existing unittest method, exports a bounded test-only packet, verifies with python -m unittest, and closes the canonical Python exception gap through an outcome receipt.
- Receipt validation: pass

### Python Real-Repo Eval `api_status_pytest_receipt`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/api-status-receipt`
- Command: `ripr check --root target/ripr/python-real-repo-evals/api-status-receipt --base HEAD~1 --format json` (270 ms)
- Top finding: Python API response status repair card for checkout expired coupon status_code
- Changed owner: `python:app/checkout.py::checkout`
- Missing discriminator: `response.status_code == 422`
- Suggested test: `tests/test_checkout.py` / `test_expired_coupon_response_smoke`
- Repair card present: true; action: `strengthen_existing_test`
- Agent packet present: true; task: `Strengthen the existing pytest API-status test with response.status_code == 422.`
- Agent packet command: `ripr agent packet --root target/ripr/python-real-repo-evals/api-status-receipt --gap-ledger gap-ledger.json --gap-id gap:python:app/checkout.py:checkout:field_value:field_construction:response.status_code=422 --json`
- Agent packet allowed files: `tests/test_checkout.py`
- Agent packet forbidden files: `app/checkout.py`
- Agent packet stop-if: `import cannot be resolved; expected status code is ambiguous; production code edit appears necessary`
- Verify command: `pytest tests/test_checkout.py::test_expired_coupon_response_smoke` (pass) - 1 passed in 0.33s
- After command: `ripr check --root target/ripr/python-real-repo-evals/api-status-receipt --base HEAD~1 --format json` (296 ms)
- Receipt command: `ripr outcome --before before-check.json --after after-check.json --format json` (pass)
- Gap movement: `closed`; closed gaps: 1
- False-positive notes: none observed
- Limitation notes: lightweight pytest API-handler scratch repo with response.status_code evidence; scoped repair routing is usable alpha while broader Python static facts remain preview/advisory
- Ranked top-3 findings checked: 1
- Ranked top-3 limit reason: Focused eval emitted one Python repair card; no rank 2 or rank 3 repairable Python finding was present.
  - Rank 1: `gap:python:app/checkout.py:checkout:field_value:field_construction:response.status_code=422`; discriminator `response.status_code == 422`; verify `pytest tests/test_checkout.py::test_expired_coupon_response_smoke`; usability `usable`
- Claim boundary: `Broader Python static facts remain preview/advisory; No arbitrary imports or tests were run by RIPR; The human-run pytest command verified only the suggested test; No support-tier promotion`
- Reason: The run proves a Python API-style status-code change can produce a bounded field/object repair card, pass the focused pytest verify command, and close the canonical Python gap through an outcome receipt.
- Receipt validation: pass

### Python Real-Repo Eval `api_json_detail_pytest_receipt`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/api-json-detail-receipt`
- Command: `ripr check --root target/ripr/python-real-repo-evals/api-json-detail-receipt --base HEAD~1 --format json` (249 ms)
- Top finding: Python API response JSON field repair card for checkout expired coupon detail
- Changed owner: `python:app/checkout.py::checkout`
- Missing discriminator: `response.json()["detail"] == "coupon expired"`
- Suggested test: `tests/test_checkout.py` / `test_expired_coupon_response_smoke`
- Repair card present: true; action: `strengthen_existing_test`
- Agent packet present: true; task: `Strengthen the existing pytest API JSON test with response.json()["detail"] == "coupon expired".`
- Agent packet command: `ripr agent packet --root target/ripr/python-real-repo-evals/api-json-detail-receipt --gap-ledger gap-ledger.json --gap-id gap:python:app/checkout.py:checkout:field_value:field_construction:return_detail_coupon_expired --json`
- Agent packet allowed files: `tests/test_checkout.py`
- Agent packet forbidden files: `app/checkout.py`
- Agent packet stop-if: `import cannot be resolved; expected response JSON field is ambiguous; production code edit appears necessary`
- Verify command: `pytest tests/test_checkout.py::test_expired_coupon_response_smoke` (pass) - 1 passed in 0.34s
- After command: `ripr check --root target/ripr/python-real-repo-evals/api-json-detail-receipt --base HEAD~1 --format json` (286 ms)
- Receipt command: `ripr outcome --before before-check.json --after after-check.json --format json` (pass)
- Gap movement: `closed`; closed gaps: 1
- False-positive notes: none observed
- Limitation notes: lightweight pytest API-handler scratch repo with response JSON field evidence; scoped repair routing is usable alpha while broader Python static facts remain preview/advisory
- Ranked top-3 findings checked: 1
- Ranked top-3 limit reason: Focused eval emitted one Python repair card; no rank 2 or rank 3 repairable Python finding was present.
  - Rank 1: `gap:python:app/checkout.py:checkout:field_value:field_construction:return_detail_coupon_expired`; discriminator `response.json()["detail"] == "coupon expired"`; verify `pytest tests/test_checkout.py::test_expired_coupon_response_smoke`; usability `usable`
- Claim boundary: `Broader Python static facts remain preview/advisory; No arbitrary imports or tests were run by RIPR; The human-run pytest command verified only the suggested test; No support-tier promotion`
- Reason: The run extends post-usable-alpha stability evidence to an API response JSON repair: RIPR routes the missing response detail discriminator to one existing pytest method, exports a bounded test-only packet, verifies with pytest, and closes the canonical Python gap through an outcome receipt.
- Receipt validation: pass

### Python Real-Repo Eval `flask_route_json_detail_pytest_receipt`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/flask-route-json-receipt`
- Command: `ripr check --root target/ripr/python-real-repo-evals/flask-route-json-receipt --base HEAD~1 --format json` (55 ms)
- Top finding: Python Flask-style route JSON field repair card for checkout expired coupon detail
- Changed owner: `python:app/checkout.py::checkout`
- Missing discriminator: `response.json()["detail"] == "coupon expired"`
- Suggested test: `tests/test_checkout.py` / `test_expired_coupon_response_smoke`
- Repair card present: true; action: `strengthen_existing_test`
- Agent packet present: true; task: `Strengthen the existing pytest Flask-style API JSON test with response.json()["detail"] == "coupon expired".`
- Agent packet command: `ripr agent packet --root target/ripr/python-real-repo-evals/flask-route-json-receipt --gap-ledger gap-ledger.json --gap-id gap:python:app/checkout.py:checkout:field_value:field_construction:return_detail_coupon_expired --json`
- Agent packet allowed files: `tests/test_checkout.py`
- Agent packet forbidden files: `app/checkout.py`
- Agent packet stop-if: `import cannot be resolved; expected response JSON field is ambiguous; production code edit appears necessary`
- Verify command: `pytest tests/test_checkout.py::test_expired_coupon_response_smoke` (pass) - 1 passed in 0.48s; scratch verification set PYTHONPATH=. and used a local Flask-shaped stub instead of installing Flask
- After command: `ripr check --root target/ripr/python-real-repo-evals/flask-route-json-receipt --base HEAD~1 --format json` (820 ms)
- Receipt command: `ripr outcome --before before-check.json --after after-check.json --format json` (pass)
- Gap movement: `closed`; closed gaps: 1
- False-positive notes: none observed
- Limitation notes: Flask-style route decorator scratch repo with a local Flask-shaped stub; scoped repair routing is usable alpha while broader Python static facts remain preview/advisory
- Ranked top-3 findings checked: 1
- Ranked top-3 limit reason: Focused eval emitted one Python repair card; no rank 2 or rank 3 repairable Python finding was present.
  - Rank 1: `gap:python:app/checkout.py:checkout:field_value:field_construction:return_detail_coupon_expired`; discriminator `response.json()["detail"] == "coupon expired"`; verify `pytest tests/test_checkout.py::test_expired_coupon_response_smoke`; usability `usable`
- Claim boundary: `Broader Python static facts remain preview/advisory; No arbitrary imports or tests were run by RIPR; The human-run pytest command verified only the suggested test; No support-tier promotion`
- Reason: The run extends post-usable-alpha stability evidence to a Flask-style route decorator with a response JSON repair: RIPR treats @app.route("/checkout", methods=["POST"]) as static route metadata, links the client.post route test, exports a bounded test-only packet, verifies with pytest, and closes the canonical Python field/object gap through an outcome receipt.
- Receipt validation: pass

### Python Real-Repo Eval `fastapi_route_json_detail_pytest_receipt`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/fastapi-route-json-receipt`
- Command: `ripr check --root target/ripr/python-real-repo-evals/fastapi-route-json-receipt --base HEAD~1 --format json` (206 ms)
- Top finding: Python FastAPI-style route JSON field repair card for checkout expired coupon detail
- Changed owner: `python:app/checkout.py::checkout`
- Missing discriminator: `response.json()["detail"] == "coupon expired"`
- Suggested test: `tests/test_checkout.py` / `test_expired_coupon_response_smoke`
- Repair card present: true; action: `strengthen_existing_test`
- Agent packet present: true; task: `Strengthen the existing pytest FastAPI-style API JSON test with response.json()["detail"] == "coupon expired".`
- Agent packet command: `ripr agent packet --root target/ripr/python-real-repo-evals/fastapi-route-json-receipt --gap-ledger gap-ledger.json --gap-id gap:python:app/checkout.py:checkout:field_value:field_construction:return_detail_coupon_expired --json`
- Agent packet allowed files: `tests/test_checkout.py`
- Agent packet forbidden files: `app/checkout.py`
- Agent packet stop-if: `import cannot be resolved; expected response JSON field is ambiguous; production code edit appears necessary`
- Verify command: `pytest tests/test_checkout.py::test_expired_coupon_response_smoke` (pass) - 1 passed in 0.35s; scratch verification used a local FastAPI-shaped stub instead of installing FastAPI
- After command: `ripr check --root target/ripr/python-real-repo-evals/fastapi-route-json-receipt --base HEAD~1 --format json` (387 ms)
- Receipt command: `ripr outcome --before before-check.json --after after-check.json --format json` (pass)
- Gap movement: `closed`; closed gaps: 1
- False-positive notes: none observed
- Limitation notes: FastAPI-style route decorator scratch repo with a local FastAPI-shaped stub; scoped repair routing is usable alpha while broader Python static facts remain preview/advisory
- Ranked top-3 findings checked: 1
- Ranked top-3 limit reason: Focused FastAPI-style API JSON eval emitted one Python repair card; no rank 2 or rank 3 repairable Python finding was present.
  - Rank 1: `gap:python:app/checkout.py:checkout:field_value:field_construction:return_detail_coupon_expired`; discriminator `response.json()["detail"] == "coupon expired"`; verify `pytest tests/test_checkout.py::test_expired_coupon_response_smoke`; usability `usable`
- Claim boundary: `Broader Python static facts remain preview/advisory; No arbitrary imports or tests were run by RIPR; The human-run pytest command verified only the suggested test; Scratch verification used a local FastAPI-shaped stub instead of adding a dependency; No support-tier promotion`
- Reason: The run extends post-usable-alpha stability evidence to a FastAPI-style route decorator with a response JSON repair: RIPR treats @app.post("/checkout") as static route metadata, links the client.post route test, exports a bounded test-only packet, verifies with pytest, and closes the canonical Python field/object gap through an outcome receipt.
- Receipt validation: pass

### Python Real-Repo Eval `api_exception_response_pytest_receipt`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/api-exception-response-receipt`
- Command: `ripr check --root target/ripr/python-real-repo-evals/api-exception-response-receipt --base HEAD~1 --format json` (748 ms)
- Top finding: Python API exception-to-response repair card for checkout expired coupon status_code
- Changed owner: `python:app/checkout.py::checkout`
- Missing discriminator: `response.status_code == 422`
- Suggested test: `tests/test_checkout.py` / `test_expired_coupon_response_smoke`
- Repair card present: true; action: `strengthen_existing_test`
- Agent packet present: true; task: `Strengthen the existing pytest API exception-response test with response.status_code == 422.`
- Agent packet command: `ripr agent packet --root target/ripr/python-real-repo-evals/api-exception-response-receipt --gap-ledger gap-ledger.json --gap-id gap:python:app/checkout.py:checkout:field_value:field_construction:response=response_status_code=422_detail=coupon_expired --json`
- Agent packet allowed files: `tests/test_checkout.py`
- Agent packet forbidden files: `app/checkout.py`
- Agent packet stop-if: `import cannot be resolved; expected status code is ambiguous; production code edit appears necessary`
- Verify command: `pytest tests/test_checkout.py::test_expired_coupon_response_smoke` (pass) - 1 passed in 0.52s
- After command: `ripr check --root target/ripr/python-real-repo-evals/api-exception-response-receipt --base HEAD~1 --format json` (430 ms)
- Receipt command: `ripr outcome --before before-check.json --after after-check.json --format json` (pass)
- Gap movement: `closed`; closed gaps: 2
- False-positive notes: none observed
- Limitation notes: lightweight pytest API-handler scratch repo with exception-to-response constructor evidence; scoped repair routing is usable alpha while broader Python static facts remain preview/advisory
- Ranked top-3 findings checked: 2
- Ranked top-3 limit reason: Focused API exception-to-response eval emitted two Python repair cards; no rank 3 repairable Python finding was present.
  - Rank 1: `gap:python:app/checkout.py:checkout:field_value:field_construction:response=response_status_code=422_detail=coupon_expired`; discriminator `response.status_code == 422`; verify `pytest tests/test_checkout.py::test_expired_coupon_response_smoke`; usability `usable`
  - Rank 2: `gap:python:app/checkout.py:checkout:return_value:return_value:response`; discriminator `return value == response`; verify `pytest tests/test_checkout.py::test_expired_coupon_response_smoke`; usability `usable`
- Claim boundary: `Broader Python static facts remain preview/advisory; No arbitrary imports or tests were run by RIPR; The human-run pytest command verified only the suggested test; No support-tier promotion`
- Reason: The run extends framework-shaped Python repair routing to a route that catches an application exception and constructs a response object: RIPR routes the missing status-code discriminator to one existing pytest method, exports a bounded test-only packet, verifies with pytest, and closes the canonical Python field/object gap through an outcome receipt.
- Receipt validation: pass

### Python Real-Repo Eval `mixed_rust_python_pytest_receipt`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/mixed-rust-python-receipt`
- Command: `ripr check --root target/ripr/python-real-repo-evals/mixed-rust-python-receipt --base HEAD~1 --format json` (361 ms)
- Top finding: Python predicate-boundary repair card in a mixed Rust/Python repo for mixed_repo_discount amount >= threshold
- Changed owner: `python:app/pricing.py::mixed_repo_discount`
- Missing discriminator: `amount == threshold`
- Suggested test: `tests/test_pricing.py` / `test_mixed_repo_discount_smoke`
- Repair card present: true; action: `strengthen_existing_test`
- Agent packet present: true; task: `Strengthen the existing pytest boundary test in the mixed repo for amount == threshold.`
- Agent packet command: `ripr agent packet --root target/ripr/python-real-repo-evals/mixed-rust-python-receipt --gap-ledger gap-ledger.json --gap-id gap:python:app/pricing.py:mixed_repo_discount:predicate_boundary:predicate:amount>=threshold --json`
- Agent packet allowed files: `tests/test_pricing.py`
- Agent packet forbidden files: `app/pricing.py`
- Agent packet stop-if: `import cannot be resolved; expected boundary value is ambiguous; production code edit appears necessary`
- Verify command: `pytest tests/test_pricing.py::test_mixed_repo_discount_smoke` (pass) - 1 passed in 0.75s
- After command: `ripr check --root target/ripr/python-real-repo-evals/mixed-rust-python-receipt --base HEAD~1 --format json` (299 ms)
- Receipt command: `ripr outcome --before before-check.json --after after-check.json --format json` (pass)
- Gap movement: `closed`; closed gaps: 1
- False-positive notes: none observed
- Limitation notes: mixed Rust/Python scratch repo with Cargo and pyproject metadata; scoped repair routing is usable alpha while broader Python static facts remain preview/advisory
- Ranked top-3 findings checked: 1
- Ranked top-3 limit reason: Focused eval emitted one Python repair card; no rank 2 or rank 3 repairable Python finding was present.
  - Rank 1: `gap:python:app/pricing.py:mixed_repo_discount:predicate_boundary:predicate:amount>=threshold`; discriminator `amount == threshold`; verify `pytest tests/test_pricing.py::test_mixed_repo_discount_smoke`; usability `usable`
- Claim boundary: `Broader Python static facts remain preview/advisory; No arbitrary imports or tests were run by RIPR; The human-run pytest command verified only the suggested test; No support-tier promotion`
- Reason: The run proves a mixed Rust/Python repo can route a changed Python predicate boundary to a bounded repair card, pass the focused pytest verify command, and close the canonical Python gap through an outcome receipt without colliding with Cargo/Rust project metadata.
- Receipt validation: pass

### Python Real-Repo Eval `decorated_route_status_pytest_receipt`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/decorated-route-receipt`
- Command: `ripr check --root target/ripr/python-real-repo-evals/decorated-route-receipt --base HEAD~1 --format json` (605 ms)
- Top finding: Python decorated route status-code repair card for checkout expired coupon response.status_code
- Changed owner: `python:app/checkout.py::checkout`
- Missing discriminator: `response.status_code == 422`
- Suggested test: `tests/test_checkout.py` / `test_expired_coupon_response_smoke`
- Repair card present: true; action: `strengthen_existing_test`
- Agent packet present: true; task: `Strengthen the existing decorated-route pytest test with response.status_code == 422.`
- Agent packet command: `ripr agent packet --root target/ripr/python-real-repo-evals/decorated-route-receipt --gap-ledger gap-ledger.json --gap-id gap:python:app/checkout.py:checkout:field_value:field_construction:response.status_code=422 --json`
- Agent packet allowed files: `tests/test_checkout.py`
- Agent packet forbidden files: `app/checkout.py`
- Agent packet stop-if: `import cannot be resolved; expected status code is ambiguous; production code edit appears necessary`
- Verify command: `pytest tests/test_checkout.py::test_expired_coupon_response_smoke` (pass) - 1 passed in 0.32s
- After command: `ripr check --root target/ripr/python-real-repo-evals/decorated-route-receipt --base HEAD~1 --format json` (1116 ms)
- Receipt command: `ripr outcome --before before-check.json --after after-check.json --format json` (pass)
- Gap movement: `closed`; closed gaps: 1
- False-positive notes: none observed
- Limitation notes: simple FastAPI/Flask-shaped route decorator scratch repo; dynamic route registration remains unsupported, scoped repair routing is usable alpha, and broader Python static facts remain preview/advisory
- Unsupported limitations: `dynamic_route_registration`
- Ranked top-3 findings checked: 1
- Ranked top-3 limit reason: Focused eval emitted one Python repair card; no rank 2 or rank 3 repairable Python finding was present.
  - Rank 1: `gap:python:app/checkout.py:checkout:field_value:field_construction:response.status_code=422`; discriminator `response.status_code == 422`; verify `pytest tests/test_checkout.py::test_expired_coupon_response_smoke`; usability `usable`
- Claim boundary: `Broader Python static facts remain preview/advisory; No arbitrary imports or tests were run by RIPR; The human-run pytest command verified only the suggested test; No support-tier promotion`
- Reason: The run proves a simple decorated Python route can route a changed status-code assignment to a bounded field/object repair card, pass the focused pytest verify command, and close the canonical Python gap through an outcome receipt without treating arbitrary decorators as transparent.
- Receipt validation: pass

### Python Real-Repo Eval `unittest_return_value_receipt`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/unittest-return-receipt`
- Command: `ripr check --root target/ripr/python-real-repo-evals/unittest-return-receipt --base HEAD~1 --format json` (184 ms)
- Top finding: Python unittest return-value repair card for invoice_total subtotal + tax + service_fee
- Changed owner: `python:app/invoice.py::invoice_total`
- Missing discriminator: `return value == subtotal + tax + service_fee`
- Suggested test: `tests/test_invoice.py` / `TestInvoiceTotals.test_invoice_total_smoke`
- Repair card present: true; action: `strengthen_existing_test`
- Agent packet present: true; task: `Strengthen the existing unittest return-value test with an exact invoice_total assertion.`
- Agent packet command: `ripr agent packet --root target/ripr/python-real-repo-evals/unittest-return-receipt --gap-ledger gap-ledger.json --gap-id gap:python:app/invoice.py:invoice_total:return_value:return_value:subtotal+tax+service_fee --json`
- Agent packet allowed files: `tests/test_invoice.py`
- Agent packet forbidden files: `app/invoice.py`
- Agent packet stop-if: `import cannot be resolved; expected return value is ambiguous; production code edit appears necessary`
- Verify command: `python -m unittest tests.test_invoice.TestInvoiceTotals.test_invoice_total_smoke` (pass) - 1 unittest test passed in 0.01s
- After command: `ripr check --root target/ripr/python-real-repo-evals/unittest-return-receipt --base HEAD~1 --format json` (91 ms)
- Receipt command: `ripr outcome --before before-check.json --after after-check.json --format json` (pass)
- Gap movement: `closed`; closed gaps: 1
- False-positive notes: none observed
- Limitation notes: unittest scratch repo adds post-promotion stability evidence; scoped repair routing is usable alpha while broader Python static facts remain preview/advisory
- Ranked top-3 findings checked: 1
- Ranked top-3 limit reason: Focused unittest eval emitted one Python repair card; no rank 2 or rank 3 repairable Python finding was present.
  - Rank 1: `gap:python:app/invoice.py:invoice_total:return_value:return_value:subtotal+tax+service_fee`; discriminator `return value == subtotal + tax + service_fee`; verify `python -m unittest tests.test_invoice.TestInvoiceTotals.test_invoice_total_smoke`; usability `usable`
- Claim boundary: `Broader Python static facts remain preview/advisory; No arbitrary imports or tests were run by RIPR; The human-run unittest command verified only the suggested test; No support-tier promotion`
- Reason: The run extends post-usable-alpha stability evidence to a unittest return-value repair: RIPR routes the missing exact return discriminator to one existing unittest method, exports a bounded test-only packet, verifies with python -m unittest, and closes the canonical Python gap through an outcome receipt.
- Receipt validation: pass

### Python Real-Repo Eval `unittest_dict_field_receipt`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/unittest-field-receipt`
- Command: `ripr check --root target/ripr/python-real-repo-evals/unittest-field-receipt --base HEAD~1 --format json` (210 ms)
- Top finding: Python unittest returned-dict field repair card for build_profile status field
- Changed owner: `python:src/profiles.py::build_profile`
- Missing discriminator: `status == "active"`
- Suggested test: `tests/test_profiles.py` / `test_build_profile_status`
- Repair card present: true; action: `strengthen_existing_test`
- Agent packet present: true; task: `Strengthen the existing unittest returned-dict field test with an exact status assertion.`
- Agent packet command: `ripr agent packet --root target/ripr/python-real-repo-evals/unittest-field-receipt --gap-ledger gap-ledger.json --gap-id gap:python:src/profiles.py:build_profile:field_value:field_construction:return_name_name_status_active --json`
- Agent packet allowed files: `tests/test_profiles.py`
- Agent packet forbidden files: `src/profiles.py`
- Agent packet stop-if: `import cannot be resolved; expected field value is ambiguous; production code edit appears necessary`
- Verify command: `python -m unittest tests.test_profiles.TestBuildProfile.test_build_profile_status` (pass) - 1 unittest test passed in 0.00s
- After command: `ripr check --root target/ripr/python-real-repo-evals/unittest-field-receipt --base HEAD~1 --format json` (210 ms)
- Receipt command: `ripr outcome --before before-check.json --after after-check.json --format json` (pass)
- Gap movement: `closed`; closed gaps: 1
- False-positive notes: none observed
- Limitation notes: unittest returned-dict field scratch repo adds post-promotion stability evidence; scoped repair routing is usable alpha while broader Python static facts remain preview/advisory
- Ranked top-3 findings checked: 1
- Ranked top-3 limit reason: Focused unittest dict-field eval emitted one Python repair card; no rank 2 or rank 3 repairable Python finding was present.
  - Rank 1: `gap:python:src/profiles.py:build_profile:field_value:field_construction:return_name_name_status_active`; discriminator `status == "active"`; verify `python -m unittest tests.test_profiles.TestBuildProfile.test_build_profile_status`; usability `usable`
- Claim boundary: `Broader Python static facts remain preview/advisory; No arbitrary imports or tests were run by RIPR; The human-run unittest command verified only the suggested test; No support-tier promotion`
- Reason: The run extends post-usable-alpha stability evidence to a unittest returned-dict field repair: RIPR routes the changed literal field to the missing discriminator `status == "active"`, exports a bounded test-only packet, verifies with python -m unittest, and closes the canonical Python gap through an outcome receipt.
- Receipt validation: pass

### Python Real-Repo Eval `model_field_pytest_receipt`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/model-field-receipt`
- Command: `ripr check --root target/ripr/python-real-repo-evals/model-field-receipt --base HEAD~1 --format json` (290 ms)
- Top finding: Python dataclass model field repair card for build_user active field
- Changed owner: `python:app/users.py::build_user`
- Missing discriminator: `result.active == True`
- Suggested test: `tests/test_users.py` / `test_build_user_smoke`
- Repair card present: true; action: `strengthen_existing_test`
- Agent packet present: true; task: `Strengthen the existing pytest model field test with an exact active-field assertion.`
- Agent packet command: `ripr agent packet --root target/ripr/python-real-repo-evals/model-field-receipt --gap-ledger gap-ledger.json --gap-id gap:python:app/users.py:build_user:field_value:field_construction:return_user_active=true --json`
- Agent packet allowed files: `tests/test_users.py`
- Agent packet forbidden files: `app/users.py`
- Agent packet stop-if: `import cannot be resolved; expected field value is ambiguous; production code edit appears necessary; returned object does not expose the constructor keyword as a public field or attribute`
- Verify command: `pytest tests/test_users.py::test_build_user_smoke` (pass) - 1 pytest test passed in 0.46s
- After command: `ripr check --root target/ripr/python-real-repo-evals/model-field-receipt --base HEAD~1 --format json` (18 ms)
- Receipt command: `ripr outcome --before before-check.json --after after-check.json --format json` (pass)
- Gap movement: `closed`; closed gaps: 1
- False-positive notes: none observed
- Limitation notes: dataclass model-field scratch repo adds post-promotion stability evidence; scoped repair routing is usable alpha while broader Python static facts remain preview/advisory
- Ranked top-3 findings checked: 1
- Ranked top-3 limit reason: Focused model-field eval emitted one Python repair card; no rank 2 or rank 3 repairable Python finding was present.
  - Rank 1: `gap:python:app/users.py:build_user:field_value:field_construction:return_user_active=true`; discriminator `result.active == True`; verify `pytest tests/test_users.py::test_build_user_smoke`; usability `usable`
- Claim boundary: `Broader Python static facts remain preview/advisory; No arbitrary imports or tests were run by RIPR; The human-run pytest command verified only the suggested test; No support-tier promotion`
- Reason: The run extends post-usable-alpha stability evidence to a dataclass/model field repair: RIPR routes the missing exact field discriminator to one existing pytest method, exports a bounded test-only packet, verifies with pytest, and closes the canonical Python gap through an outcome receipt.
- Receipt validation: pass

## Python Static-Limit Eval Receipts

These checked no-action cases record where Python repair routing stops instead of emitting a repair card or agent packet. They protect the fail-closed side of the lane and are not counted as successful repair recommendations.

| Case | Repo shape | Static limit | Finding | Related test | Packet | Reason |
| --- | --- | --- | --- | --- | --- | --- |
| `dynamic_dispatch_no_packet_eval` | `dynamic_dispatch_pytest` | `dynamic_dispatch` | `probe:src_runtime.py:2:python_preview` | `tests/test_runtime.py` / `test_dispatch_total` | `none` | static limit dynamic_dispatch prevents bounded repair routing from a dynamic registry call to a safe test input |
| `decorator_indirection_no_packet_eval` | `decorator_indirection_pytest` | `decorator_indirection` | `probe:src_retry_total.py:3:python_preview` | `tests/test_retry_total.py` / `test_fetch_total_uses_retry_path` | `none` | static limit decorator_indirection prevents bounded repair routing because the changed owner is wrapped by a runtime decorator whose call behavior is not resolved |
| `missing_import_graph_no_packet_eval` | `missing_import_graph_pytest` | `missing_import_graph` | `probe:src_totals.py:4:python_preview` | `tests/test_totals.py` / `test_total_includes_tax` | `none` | static limit missing_import_graph prevents bounded repair routing because the changed behavior depends on an imported implementation whose semantics are not resolved |
| `metaprogramming_no_packet_eval` | `metaprogramming_pytest` | `metaprogramming` | `probe:src_models.py:2:python_preview` | `tests/test_models.py` / `test_build_model_sets_name` | `none` | static limit metaprogramming prevents bounded repair routing because the changed behavior depends on runtime-created class semantics |
| `mocked_module_no_packet_eval` | `mocked_module_pytest` | `mocked_module` | `probe:src_service.py:2:python_preview` | `tests/test_service.py` / `test_fetch_total_uses_client_total` | `none` | static limit mocked_module prevents bounded repair routing because the related pytest test uses patch/mock/monkeypatch module substitution |
| `opaque_custom_helper_no_packet_eval` | `opaque_custom_helper_pytest` | `opaque_custom_assertion_helper` | `probe:src_pricing.py:2:python_preview` | `tests/test_pricing.py` / `test_apply_discount_custom_helper` | `none` | static limit opaque_custom_assertion_helper prevents bounded repair routing because the related pytest test observes behavior through an assert_* helper whose body is not inspected |
| `property_based_no_packet_eval` | `property_based_pytest` | `property_based_test` | `probe:src_pricing.py:2:python_preview` | `tests/test_pricing.py` / `test_apply_discount_property` | `none` | static limit property_based_test prevents bounded repair routing because the related pytest test uses generated inputs whose discriminator coverage cannot be determined statically |
| `unresolved_fixture_no_packet_eval` | `fixture_sourced_pytest` | `unresolved_pytest_fixture` | `probe:src_pricing.py:2:python_preview` | `tests/test_pricing.py` / `test_apply_discount_fixture_case` | `none` | static limit unresolved_pytest_fixture prevents bounded repair routing because the related pytest test depends on fixture-sourced input and expected values |
| `generated_file_no_packet_eval` | `generated_python_file` | `generated_file` | `excluded:src/schema_pb2.py:generated_file` | `tests/test_schema.py` / `test_status_generated_value` | `none` | static limit generated_file prevents bounded repair routing because the changed file is detectable generated Python code |
| `unsupported_syntax_no_packet_eval` | `lambda_return_pytest` | `unsupported_syntax` | `probe:src_mapper.py:2:python_preview` | `tests/test_mapper.py` / `test_make_mapper_adds_two` | `none` | static limit unsupported_syntax prevents bounded repair routing because the changed lambda-return expression is outside the preview adapter's precise syntax model |
| `werkzeug_multiple_cookie_headers_2065_historical_limit` | `upstream_werkzeug_src_layout_cached_property` | `decorator_indirection` | `probe:src_werkzeug_sansio_request.py:python_preview:b3a84bb3` | `tests/sansio/test_request.py` / `test_cookies` | `none` | The historical exact-3e900f analyzer reports decorator_indirection for cached_property and only an uncertain same-stem test link; no valid actionable target is established. |

### Python Static-Limit Eval `dynamic_dispatch_no_packet_eval`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/dynamic-dispatch-limit`
- Command: `ripr check --root target/ripr/python-real-repo-evals/dynamic-dispatch-limit --base HEAD~1 --format json` (230 ms)
- Changed owner: `python:src/runtime.py::dispatch`
- Static limit: `dynamic_dispatch`; classification: `static_unknown`
- Stop reasons: `dynamic_dispatch_unresolved`
- Related test: `tests/test_runtime.py` / `test_dispatch_total`
- Why not actionable: static limit dynamic_dispatch prevents bounded repair routing from a dynamic registry call to a safe test input
- Repair card present: false; agent packet present: false
- Verify: `not_applicable_static_limit` (not_applicable)
- Receipt: `not_applicable_static_limit` (not_applicable)
- Gap movement: `no_receipt`
- False-positive notes: none observed
- Limitation notes: RIPR found the related pytest method and exact oracle but emitted no repair card or agent packet because the changed line uses registry[key]() dynamic dispatch; no human or agent test-repair task is routed.
- Claim boundary: `Broader Python static facts remain preview/advisory; No repair packet emitted; No support-tier promotion; No arbitrary imports or tests were run by RIPR`
- Reason: The run records the fail-closed side of Python repair routing: a dynamic-dispatch Python change stays visible as a static limitation with a related test but does not enter the repair queue.
- Static-limit validation: pass

### Python Static-Limit Eval `decorator_indirection_no_packet_eval`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/decorator-indirection-limit`
- Command: `ripr check --root target/ripr/python-real-repo-evals/decorator-indirection-limit --base HEAD~1 --format json` (247 ms)
- Changed owner: `python:src/retry_total.py::fetch_total`
- Static limit: `decorator_indirection`; classification: `static_unknown`
- Stop reasons: `static_probe_unknown`
- Related test: `tests/test_retry_total.py` / `test_fetch_total_uses_retry_path`
- Why not actionable: static limit decorator_indirection prevents bounded repair routing because the changed owner is wrapped by a runtime decorator whose call behavior is not resolved
- Repair card present: false; agent packet present: false
- Verify: `not_applicable_static_limit` (not_applicable)
- Receipt: `not_applicable_static_limit` (not_applicable)
- Gap movement: `no_receipt`
- False-positive notes: none observed
- Limitation notes: RIPR found the related pytest method and exact oracle but emitted no repair card or agent packet because the changed owner is decorated with retry; no human or agent test-repair task is routed.
- Claim boundary: `Broader Python static facts remain preview/advisory; No repair packet emitted; No support-tier promotion; No arbitrary imports or tests were run by RIPR`
- Reason: The run records another fail-closed Python repair-routing shape: decorator-modified call behavior stays visible as a static limitation while excluded from repair-card, packet, verify, and receipt success metrics.
- Static-limit validation: pass

### Python Static-Limit Eval `missing_import_graph_no_packet_eval`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/missing-import-graph-limit`
- Command: `ripr check --root target/ripr/python-real-repo-evals/missing-import-graph-limit --base HEAD~1 --format json` (85 ms)
- Changed owner: `python:src/totals.py::total`
- Static limit: `missing_import_graph`; classification: `static_unknown`
- Stop reasons: `static_probe_unknown`
- Related test: `tests/test_totals.py` / `test_total_includes_tax`
- Why not actionable: static limit missing_import_graph prevents bounded repair routing because the changed behavior depends on an imported implementation whose semantics are not resolved
- Repair card present: false; agent packet present: false
- Verify: `not_applicable_static_limit` (not_applicable)
- Receipt: `not_applicable_static_limit` (not_applicable)
- Gap movement: `no_receipt`
- False-positive notes: none observed
- Limitation notes: RIPR found the related pytest method and exact oracle but emitted no repair card or agent packet because the changed call targets an imported remote_total implementation outside the static preview import graph; no human or agent test-repair task is routed.
- Claim boundary: `Broader Python static facts remain preview/advisory; No repair packet emitted; No support-tier promotion; No arbitrary imports or tests were run by RIPR`
- Reason: The run records another fail-closed Python repair-routing shape: unresolved imported-call behavior stays visible as a static limitation while excluded from repair-card, packet, verify, and receipt success metrics.
- Static-limit validation: pass

### Python Static-Limit Eval `metaprogramming_no_packet_eval`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/metaprogramming-limit`
- Command: `ripr check --root target/ripr/python-real-repo-evals/metaprogramming-limit --base HEAD~1 --format json` (280 ms)
- Changed owner: `python:src/models.py::build_model`
- Static limit: `metaprogramming`; classification: `static_unknown`
- Stop reasons: `static_probe_unknown`
- Related test: `tests/test_models.py` / `test_build_model_sets_name`
- Why not actionable: static limit metaprogramming prevents bounded repair routing because the changed behavior depends on runtime-created class semantics
- Repair card present: false; agent packet present: false
- Verify: `not_applicable_static_limit` (not_applicable)
- Receipt: `not_applicable_static_limit` (not_applicable)
- Gap movement: `no_receipt`
- False-positive notes: none observed
- Limitation notes: RIPR found the related pytest method and exact oracle but emitted no repair card or agent packet because runtime-created class behavior is outside the static preview model; no human or agent test-repair task is routed.
- Claim boundary: `Broader Python static facts remain preview/advisory; No repair packet emitted; No support-tier promotion; No arbitrary imports or tests were run by RIPR`
- Reason: The run records another fail-closed Python repair-routing shape: runtime-created class behavior stays visible as a static limitation while excluded from repair-card, packet, verify, and receipt success metrics.
- Static-limit validation: pass

### Python Static-Limit Eval `mocked_module_no_packet_eval`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/mocked-module-limit`
- Command: `ripr check --root target/ripr/python-real-repo-evals/mocked-module-limit --base HEAD~1 --format json` (125 ms)
- Changed owner: `python:src/service.py::fetch_total`
- Static limit: `mocked_module`; classification: `static_unknown`
- Stop reasons: `static_probe_unknown`
- Related test: `tests/test_service.py` / `test_fetch_total_uses_client_total`
- Why not actionable: static limit mocked_module prevents bounded repair routing because the related pytest test uses patch/mock/monkeypatch module substitution
- Repair card present: false; agent packet present: false
- Verify: `not_applicable_static_limit` (not_applicable)
- Receipt: `not_applicable_static_limit` (not_applicable)
- Gap movement: `no_receipt`
- False-positive notes: none observed
- Limitation notes: RIPR found the related pytest method and exact oracle but emitted no repair card or agent packet because the test depends on unittest.mock.patch runtime substitution; no human or agent test-repair task is routed.
- Claim boundary: `Broader Python static facts remain preview/advisory; No repair packet emitted; No support-tier promotion; No arbitrary imports or tests were run by RIPR`
- Reason: The run records another fail-closed Python repair-routing shape: mocked-module pytest evidence stays visible as a static limitation while excluded from repair-card, packet, verify, and receipt success metrics.
- Static-limit validation: pass

### Python Static-Limit Eval `opaque_custom_helper_no_packet_eval`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/opaque-helper-limit`
- Command: `ripr check --root target/ripr/python-real-repo-evals/opaque-helper-limit --base HEAD~1 --format json` (1456 ms)
- Changed owner: `python:src/pricing.py::apply_discount`
- Static limit: `opaque_custom_assertion_helper`; classification: `static_unknown`
- Stop reasons: `static_probe_unknown`
- Related test: `tests/test_pricing.py` / `test_apply_discount_custom_helper`
- Why not actionable: static limit opaque_custom_assertion_helper prevents bounded repair routing because the related pytest test observes behavior through an assert_* helper whose body is not inspected
- Repair card present: false; agent packet present: false
- Verify: `not_applicable_static_limit` (not_applicable)
- Receipt: `not_applicable_static_limit` (not_applicable)
- Gap movement: `no_receipt`
- False-positive notes: none observed
- Limitation notes: RIPR found the related pytest method and opaque custom helper oracle but emitted no repair card or agent packet because the helper body may or may not observe the changed discriminator; no human or agent test-repair task is routed.
- Claim boundary: `Broader Python static facts remain preview/advisory; No repair packet emitted; No support-tier promotion; No arbitrary imports or tests were run by RIPR`
- Reason: The run records another fail-closed Python repair-routing shape: opaque custom assertion helpers stay visible as static limitations while excluded from repair-card, packet, verify, and receipt success metrics.
- Static-limit validation: pass

### Python Static-Limit Eval `property_based_no_packet_eval`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/property-based-limit`
- Command: `ripr check --root target/ripr/python-real-repo-evals/property-based-limit --base HEAD~1 --format json` (399 ms)
- Changed owner: `python:src/pricing.py::apply_discount`
- Static limit: `property_based_test`; classification: `static_unknown`
- Stop reasons: `static_probe_unknown`
- Related test: `tests/test_pricing.py` / `test_apply_discount_property`
- Why not actionable: static limit property_based_test prevents bounded repair routing because the related pytest test uses generated inputs whose discriminator coverage cannot be determined statically
- Repair card present: false; agent packet present: false
- Verify: `not_applicable_static_limit` (not_applicable)
- Receipt: `not_applicable_static_limit` (not_applicable)
- Gap movement: `no_receipt`
- False-positive notes: none observed
- Limitation notes: RIPR found the related pytest method and weak relational oracle but emitted no repair card or agent packet because the Hypothesis-style generated inputs may or may not include the changed discriminator; no human or agent test-repair task is routed.
- Claim boundary: `Broader Python static facts remain preview/advisory; No repair packet emitted; No support-tier promotion; No arbitrary imports or tests were run by RIPR`
- Reason: The run records another fail-closed Python repair-routing shape: property-based generated inputs stay visible as static limitations while excluded from repair-card, packet, verify, and receipt success metrics.
- Static-limit validation: pass

### Python Static-Limit Eval `unresolved_fixture_no_packet_eval`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/unresolved-fixture-limit`
- Command: `ripr check --root target/ripr/python-real-repo-evals/unresolved-fixture-limit --base HEAD~1 --format json` (7700 ms)
- Changed owner: `python:src/pricing.py::apply_discount`
- Static limit: `unresolved_pytest_fixture`; classification: `static_unknown`
- Stop reasons: `static_probe_unknown`
- Related test: `tests/test_pricing.py` / `test_apply_discount_fixture_case`
- Why not actionable: static limit unresolved_pytest_fixture prevents bounded repair routing because the related pytest test depends on fixture-sourced input and expected values
- Repair card present: false; agent packet present: false
- Verify: `not_applicable_static_limit` (not_applicable)
- Receipt: `not_applicable_static_limit` (not_applicable)
- Gap movement: `no_receipt`
- False-positive notes: none observed
- Limitation notes: RIPR found the related pytest method and exact oracle but emitted no repair card or agent packet because the test obtains the changed discriminator and expected value from a pytest fixture; no human or agent test-repair task is routed.
- Claim boundary: `Broader Python static facts remain preview/advisory; No repair packet emitted; No support-tier promotion; No arbitrary imports or tests were run by RIPR`
- Reason: The run records a second fail-closed Python repair-routing shape: fixture-sourced pytest values keep the static limitation visible while excluding the finding from repair-card, packet, verify, and receipt success metrics.
- Static-limit validation: pass

### Python Static-Limit Eval `generated_file_no_packet_eval`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/generated-file-limit`
- Command: `ripr check --root target/ripr/python-real-repo-evals/generated-file-limit --base HEAD~1 --format json` (4513 ms)
- Changed owner: `python:src/schema_pb2.py::<generated>`
- Static limit: `generated_file`; classification: `excluded`
- Stop reasons: `generated_file_excluded`
- Related test: `tests/test_schema.py` / `test_status_generated_value`
- Why not actionable: static limit generated_file prevents bounded repair routing because the changed file is detectable generated Python code
- Repair card present: false; agent packet present: false
- Verify: `not_applicable_static_limit` (not_applicable)
- Receipt: `not_applicable_static_limit` (not_applicable)
- Gap movement: `no_receipt`
- False-positive notes: none observed
- Limitation notes: RIPR excluded the generated src/schema_pb2.py diff before repair routing, so no repair card, agent packet, verify success, or receipt movement is claimed even though a related pytest import exists.
- Claim boundary: `Broader Python static facts remain preview/advisory; No repair packet emitted; No support-tier promotion; No arbitrary imports or tests were run by RIPR`
- Reason: The run records the generated-code fail-closed side of Python repair routing: detectable generated Python files stay out of repair-card, packet, verify, and receipt success metrics instead of becoming noisy work.
- Static-limit validation: pass

### Python Static-Limit Eval `unsupported_syntax_no_packet_eval`

- Source: `scratch_repo` / `target/ripr/python-real-repo-evals/unsupported-syntax-limit`
- Command: `ripr check --root target/ripr/python-real-repo-evals/unsupported-syntax-limit --base HEAD~1 --format json` (663 ms)
- Changed owner: `python:src/mapper.py::make_mapper`
- Static limit: `unsupported_syntax`; classification: `static_unknown`
- Stop reasons: `static_probe_unknown`
- Related test: `tests/test_mapper.py` / `test_make_mapper_adds_two`
- Why not actionable: static limit unsupported_syntax prevents bounded repair routing because the changed lambda-return expression is outside the preview adapter's precise syntax model
- Repair card present: false; agent packet present: false
- Verify: `not_applicable_static_limit` (not_applicable)
- Receipt: `not_applicable_static_limit` (not_applicable)
- Gap movement: `no_receipt`
- False-positive notes: none observed
- Limitation notes: RIPR found the related pytest method and exact oracle but emitted no repair card or agent packet because the changed lambda-return syntax is intentionally outside the preview adapter's safe repair-routing model; no human or agent test-repair task is routed.
- Claim boundary: `Broader Python static facts remain preview/advisory; No repair packet emitted; No support-tier promotion; No arbitrary imports or tests were run by RIPR`
- Reason: The run records another fail-closed Python repair-routing shape: unsupported lambda-return syntax stays visible as a static limitation while excluded from repair-card, packet, verify, and receipt success metrics.
- Static-limit validation: pass

### Python Static-Limit Eval `werkzeug_multiple_cookie_headers_2065_historical_limit`

- Source: `external_repo` / `fixtures/python-real-repo-evals/werkzeug-multiple-cookie`
- Command: `ripr check --root <owned copy of werkzeug-multiple-cookie/input> --diff fixtures/python-real-repo-evals/werkzeug-multiple-cookie/production.patch --mode fast --format json` (563 ms)
- Changed owner: `python:src/werkzeug/sansio/request.py::Request.cookies`
- Static limit: `decorator_indirection`; classification: `static_unknown`
- Stop reasons: `static_probe_unknown`
- Related test: `tests/sansio/test_request.py` / `test_cookies`
- Why not actionable: The historical exact-3e900f analyzer reports decorator_indirection for cached_property and only an uncertain same-stem test link; no valid actionable target is established.
- Repair card present: false; agent packet present: false
- Verify: `not_applicable_static_limit` (not_applicable)
- Receipt: `not_applicable_static_limit` (not_applicable)
- Gap movement: `no_receipt`
- False-positive notes: none observed
- Limitation notes: Historical 3e900f source-built CLI emits two static_unknown probes with either effective or weakened tests. Separate native original/retained replay proves the actual list discriminator. This observation is not a permanent desired product outcome or current-main qualification.
- Claim boundary: `Broader Python static facts remain preview/advisory; No repair packet emitted; No support-tier promotion; No arbitrary imports or tests were run by RIPR; Independent pytest execution is evidence about this historical upstream bug only; Curated upstream regression; no blind operator, real-opportunity denominator, package, installed journey or release credit`
- Reason: Retain a genuine upstream production/test/license/provenance case and a native oracle before assessing static output. Future capability work must distinguish the effective getlist assertion from the retained ineffective first-value assertion, with established descriptor identity and negative rebound/unknown-decorator controls.
- Static-limit validation: pass

## Python No-Action Eval Receipts

These checked no-action cases record ordinary routing stops where no bounded repair card or agent packet is safe even though the analyzer did not hit a static limitation. They keep visible no-path evidence separate from repair-card success metrics and static-limit dogfood.

| Case | Repo shape | No-action state | Finding | Related test | Packet | Reason |
| --- | --- | --- | --- | --- | --- | --- |
| `no_related_test_no_packet_eval` | `unrelated_pytest_text_mention` | `no_related_test` | `probe:src_pricing.py:2:python_preview` | `not_applicable_no_related_test` / `not_applicable_no_related_test` | `none` | no_action no_related_test prevents bounded repair routing because no related pytest or unittest test statically calls the changed owner |
| `already_observed_no_packet_eval` | `strong_pytest_oracle` | `already_observed` | `probe:src_discount.py:2:python_preview` | `tests/test_discount.py` / `test_apply_discount_boundary` | `none` | no_action already_observed prevents bounded repair routing because the related pytest test already carries an exact-value oracle for the changed behavior |
| `heuristic_only_no_packet_eval` | `pytest_test_name_similarity` | `heuristic_only` | `probe:src_pricing.py:2:python_preview` | `tests/test_checkout.py` / `test_apply_discount_boundary_case` | `none` | no_action heuristic_only prevents bounded repair routing because the related pytest test is linked only by test-name similarity and does not statically call the changed owner |

### Python No-Action Eval `no_related_test_no_packet_eval`

- Source: `local_repo` / `fixtures/python_unrelated_test_mention`
- Command: `ripr check --root fixtures/python_unrelated_test_mention/input --diff fixtures/python_unrelated_test_mention/diff.patch --format json` (7181 ms)
- Changed owner: `python:src/pricing.py::apply_discount`
- No-action state: `no_related_test`; classification: `no_static_path`
- Stop reasons: `no_related_test`
- Related test: `not_applicable_no_related_test` / `not_applicable_no_related_test`
- Why not actionable: no_action no_related_test prevents bounded repair routing because no related pytest or unittest test statically calls the changed owner
- Repair card present: false; agent packet present: false
- Verify: `not_applicable_no_action` (not_applicable)
- Receipt: `not_applicable_no_action` (not_applicable)
- Gap movement: `no_receipt`
- False-positive notes: none observed
- Limitation notes: RIPR reports no_static_path for the changed apply_discount return because the only pytest text mention is unrelated; no repair card, agent packet, verify success, or receipt movement is claimed.
- Claim boundary: `Broader Python static facts remain preview/advisory; No repair packet emitted; No support-tier promotion; No arbitrary imports or tests were run by RIPR`
- Reason: The run records an ordinary Python no-action state separate from static analyzer limitations: without a statically related test, RIPR keeps the finding visible but does not route a bounded human or agent test-repair task.
- No-action validation: pass

### Python No-Action Eval `already_observed_no_packet_eval`

- Source: `local_repo` / `fixtures/python_strong_oracle`
- Command: `ripr check --root fixtures/python_strong_oracle/input --diff fixtures/python_strong_oracle/diff.patch --format json` (327 ms)
- Changed owner: `python:src/discount.py::apply_discount`
- No-action state: `already_observed`; classification: `exposed`
- Stop reasons: `already_observed_strong_oracle`
- Related test: `tests/test_discount.py` / `test_apply_discount_boundary`
- Why not actionable: no_action already_observed prevents bounded repair routing because the related pytest test already carries an exact-value oracle for the changed behavior
- Repair card present: false; agent packet present: false
- Verify: `not_applicable_no_action` (not_applicable)
- Receipt: `not_applicable_no_action` (not_applicable)
- Gap movement: `no_receipt`
- False-positive notes: none observed
- Limitation notes: RIPR reports exposed for the changed apply_discount predicate because the related pytest boundary assertion already has an exact-value oracle; no repair card, agent packet, verify success, or receipt movement is claimed because there is no missing proof to route.
- Claim boundary: `Broader Python static facts remain preview/advisory; No repair packet emitted; No support-tier promotion; No arbitrary imports or tests were run by RIPR`
- Reason: The run records an ordinary Python no-action state separate from static analyzer limitations: when exact current test evidence already observes the changed behavior, RIPR keeps the finding visible but does not route redundant human or agent test-repair work.
- No-action validation: pass

### Python No-Action Eval `heuristic_only_no_packet_eval`

- Source: `local_repo` / `fixtures/python_related_test_name_similarity`
- Command: `ripr check --root fixtures/python_related_test_name_similarity/input --diff fixtures/python_related_test_name_similarity/diff.patch --format json` (361 ms)
- Changed owner: `python:src/pricing.py::apply_discount`
- No-action state: `heuristic_only`; classification: `weakly_exposed`
- Stop reasons: `heuristic_only_related_test_link`
- Related test: `tests/test_checkout.py` / `test_apply_discount_boundary_case`
- Why not actionable: no_action heuristic_only prevents bounded repair routing because the related pytest test is linked only by test-name similarity and does not statically call the changed owner
- Repair card present: false; agent packet present: false
- Verify: `not_applicable_no_action` (not_applicable)
- Receipt: `not_applicable_no_action` (not_applicable)
- Gap movement: `no_receipt`
- False-positive notes: none observed
- Limitation notes: RIPR reports weakly_exposed for the changed apply_discount return because only heuristic pytest test-name similarity links the test file; no repair card, agent packet, verify success, or receipt movement is claimed because the relation is not safe enough to route.
- Claim boundary: `Broader Python static facts remain preview/advisory; No repair packet emitted; No support-tier promotion; No arbitrary imports or tests were run by RIPR`
- Reason: The run records an ordinary Python no-action state separate from static analyzer limitations: heuristic-only related-test proximity stays visible as weak evidence but does not route bounded human or agent test-repair work.
- No-action validation: pass

## User Surface Projection Alignment Receipts

These receipts validate that badge, LSP, PR comment, and CI projection examples consume canonical repair state for full runs and canonical runtime state for limited runs instead of independently interpreting raw findings. They keep all four surfaces advisory by default and require limited/stale state visibility.

- Default CI blocking: no
- Receipt input: `fixtures/user-surface-projection-alignment/corpus.json`

| Case | Surface | Headline | Basis | Gap ID | Packet | Advisory | Raw headline |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `badge_actionable_count_from_canonical_state` | `badge` | `ripr: 8 actionable` | `canonical_actionable_gap` | `gap:boundary-discriminator-004` | `boundary-discriminator-004` | yes | no |
| `badge_limited_state_from_canonical_runtime` | `badge` | `ripr: limited` | `canonical_runtime_status` | `` | `` | yes | no |
| `lsp_status_uses_top_packet_from_canonical_state` | `lsp` | `Top gap: boundary discriminator missing` | `canonical_actionable_gap` | `gap:boundary-discriminator-004` | `boundary-discriminator-004` | yes | no |
| `lsp_stale_state_from_canonical_runtime` | `lsp` | `RIPR evidence stale` | `canonical_runtime_status` | `` | `` | yes | no |
| `pr_comment_limited_sampled_input_from_readiness` | `pr_comment` | `RIPR evidence limited: sampled input` | `canonical_runtime_status` | `` | `` | yes | no |
| `pr_comment_limited_incomplete_input_from_readiness` | `pr_comment` | `RIPR evidence limited: incomplete input` | `canonical_runtime_status` | `` | `` | yes | no |
| `pr_comment_delta_uses_canonical_outcome_state` | `pr_comment` | `Actionable gaps: 8 -> 7; top remaining packet boundary-discriminator-004` | `canonical_actionable_gap` | `gap:boundary-discriminator-004` | `boundary-discriminator-004` | yes | no |
| `badge_route_quality_degraded_from_canonical_outcome` | `badge` | `ripr: route-quality attention` | `canonical_actionable_gap` | `gap:output-observer-009` | `output-observer-009` | yes | no |
| `lsp_route_quality_degraded_from_canonical_outcome` | `lsp` | `Top gap needs route-quality attention` | `canonical_actionable_gap` | `gap:output-observer-009` | `output-observer-009` | yes | no |
| `pr_comment_route_quality_degraded_from_canonical_outcome` | `pr_comment` | `Repair route quality needs attention: add_output_observer unchanged` | `canonical_actionable_gap` | `gap:output-observer-009` | `output-observer-009` | yes | no |
| `ci_route_quality_degraded_from_canonical_outcome` | `ci` | `RIPR advisory: repair route quality needs attention` | `canonical_actionable_gap` | `gap:output-observer-009` | `output-observer-009` | yes | no |
| `pr_comment_missing_receipt_from_canonical_outcome` | `pr_comment` | `RIPR evidence needs receipt: error-variant-012` | `canonical_actionable_gap` | `gap:error-variant-012` | `error-variant-012` | yes | no |
| `ci_summary_remains_advisory_over_canonical_state` | `ci` | `RIPR advisory: top repair packet boundary-discriminator-004` | `canonical_actionable_gap` | `gap:boundary-discriminator-004` | `boundary-discriminator-004` | yes | no |
| `badge_static_limitation_backlog_from_canonical_state` | `badge` | `ripr: static limitations` | `canonical_limitation_backlog` | `gap:activation-owner-call-absent-assertion-target-affinity` | `limitation:activation-owner-call-absent-assertion-target-affinity:assertion-target-affinity-owner-call-tracing` | yes | no |
| `lsp_static_limitation_backlog_from_canonical_state` | `lsp` | `Top limitation: assertion-target affinity owner-call tracing` | `canonical_limitation_backlog` | `gap:activation-owner-call-absent-assertion-target-affinity` | `limitation:activation-owner-call-absent-assertion-target-affinity:assertion-target-affinity-owner-call-tracing` | yes | no |
| `pr_comment_static_limitation_backlog_from_canonical_state` | `pr_comment` | `Static limitation route: analysis/assertion-target-affinity-owner-call-tracing` | `canonical_limitation_backlog` | `gap:activation-owner-call-absent-assertion-target-affinity` | `limitation:activation-owner-call-absent-assertion-target-affinity:assertion-target-affinity-owner-call-tracing` | yes | no |
| `ci_static_limitation_backlog_from_canonical_state` | `ci` | `RIPR advisory: static limitation route needs analyzer work` | `canonical_limitation_backlog` | `gap:activation-owner-call-absent-assertion-target-affinity` | `limitation:activation-owner-call-absent-assertion-target-affinity:assertion-target-affinity-owner-call-tracing` | yes | no |

### User Surface Projection `badge_actionable_count_from_canonical_state`

- Surface: `badge`
- Artifact: `badge-basis`
- Run status: `full`
- Top next action: `attempt_ready_packet`
- Repair kind: `add_boundary_assertion`
- Verify command: `cargo test -p ripr-swarm boundary_discriminator_004`
- Receipt command: `cargo xtask receipts write --packet boundary-discriminator-004 --canonical-gap gap:boundary-discriminator-004`
- Source alignment case: `receipt_improved_top_next_action_alignment`
- Limitation category: ``
- Runtime repair command: ``
- Counts: actionable 8, raw findings 312
- Consumes canonical state: yes; reinterprets raw findings: no; blocking default: no
- Limited visible: yes; stale visible: yes
- Reason: Badge projection should show unresolved actionable canonical gaps or limited/stale, never raw finding volume.
- Receipt validation: pass

### User Surface Projection `badge_limited_state_from_canonical_runtime`

- Surface: `badge`
- Artifact: `badge-basis`
- Run status: `limited_large_cache_skip`
- Top next action: `resolve_limited_runtime_status`
- Repair kind: ``
- Verify command: ``
- Receipt command: ``
- Source alignment case: ``
- Limitation category: `limited_large_cache_skip`
- Runtime repair command: `cargo xtask cache report && cargo xtask cache gc --dry-run`
- Counts: actionable 8, raw findings 312
- Consumes canonical state: yes; reinterprets raw findings: no; blocking default: no
- Limited visible: yes; stale visible: yes
- Reason: Badge projection should show limited runtime status instead of an actionable count when canonical evidence is not downstream-consumable.
- Receipt validation: pass

### User Surface Projection `lsp_status_uses_top_packet_from_canonical_state`

- Surface: `lsp`
- Artifact: `editor actionable gap queue`
- Run status: `full`
- Top next action: `attempt_ready_packet`
- Repair kind: `add_boundary_assertion`
- Verify command: `cargo test -p ripr-swarm boundary_discriminator_004`
- Receipt command: `cargo xtask receipts write --packet boundary-discriminator-004 --canonical-gap gap:boundary-discriminator-004`
- Source alignment case: `receipt_improved_top_next_action_alignment`
- Limitation category: ``
- Runtime repair command: ``
- Counts: actionable 8, raw findings 312
- Consumes canonical state: yes; reinterprets raw findings: no; blocking default: no
- Limited visible: yes; stale visible: yes
- Reason: LSP status should show one safe next action or fail-closed reason from canonical queue state.
- Receipt validation: pass

### User Surface Projection `lsp_stale_state_from_canonical_runtime`

- Surface: `lsp`
- Artifact: `editor actionable gap queue`
- Run status: `limited_stale_input`
- Top next action: `resolve_limited_runtime_status`
- Repair kind: ``
- Verify command: ``
- Receipt command: ``
- Source alignment case: ``
- Limitation category: `limited_stale_input`
- Runtime repair command: `cargo xtask lane1-evidence-audit`
- Counts: actionable 8, raw findings 312
- Consumes canonical state: yes; reinterprets raw findings: no; blocking default: no
- Limited visible: yes; stale visible: yes
- Reason: LSP status should show stale canonical evidence instead of routing an agent to attempt a packet from stale input.
- Receipt validation: pass

### User Surface Projection `pr_comment_limited_sampled_input_from_readiness`

- Surface: `pr_comment`
- Artifact: `pr evidence summary`
- Run status: `limited_sampled_input`
- Top next action: `resolve_limited_runtime_status`
- Repair kind: ``
- Verify command: ``
- Receipt command: ``
- Source alignment case: ``
- Limitation category: `lane1_repo_exposure_sampled`
- Runtime repair command: `cargo xtask lane1-evidence-audit`
- Counts: actionable 8, raw findings 312
- Consumes canonical state: yes; reinterprets raw findings: no; blocking default: no
- Limited visible: yes; stale visible: yes
- Reason: PR comments should name sampled Lane 1 input from readiness instead of projecting stale packet identity.
- Receipt validation: pass

### User Surface Projection `pr_comment_limited_incomplete_input_from_readiness`

- Surface: `pr_comment`
- Artifact: `pr evidence summary`
- Run status: `limited_incomplete_input`
- Top next action: `resolve_limited_runtime_status`
- Repair kind: ``
- Verify command: ``
- Receipt command: ``
- Source alignment case: ``
- Limitation category: `lane1_repo_exposure_incomplete`
- Runtime repair command: `cargo xtask lane1-evidence-audit`
- Counts: actionable 8, raw findings 312
- Consumes canonical state: yes; reinterprets raw findings: no; blocking default: no
- Limited visible: yes; stale visible: yes
- Reason: PR comments should name incomplete Lane 1 input from readiness instead of projecting stale packet identity.
- Receipt validation: pass

### User Surface Projection `pr_comment_delta_uses_canonical_outcome_state`

- Surface: `pr_comment`
- Artifact: `pr evidence summary`
- Run status: `full`
- Top next action: `attempt_ready_packet`
- Repair kind: `add_boundary_assertion`
- Verify command: `cargo test -p ripr-swarm boundary_discriminator_004`
- Receipt command: `cargo xtask receipts write --packet boundary-discriminator-004 --canonical-gap gap:boundary-discriminator-004`
- Source alignment case: `receipt_improved_top_next_action_alignment`
- Limitation category: ``
- Runtime repair command: ``
- Counts: actionable 8, raw findings 312
- Consumes canonical state: yes; reinterprets raw findings: no; blocking default: no
- Limited visible: yes; stale visible: yes
- Reason: PR comments should show canonical/actionable/outcome deltas rather than raw-count churn.
- Receipt validation: pass

### User Surface Projection `badge_route_quality_degraded_from_canonical_outcome`

- Surface: `badge`
- Artifact: `badge-basis`
- Run status: `full`
- Top next action: `improve_repair_route_quality`
- Repair kind: `add_output_observer`
- Verify command: `cargo test -p ripr-swarm output_observer_009`
- Receipt command: `cargo xtask receipts write --packet output-observer-009 --canonical-gap gap:output-observer-009`
- Source alignment case: `receipt_unchanged_route_quality_alignment`
- Limitation category: ``
- Runtime repair command: ``
- Counts: actionable 8, raw findings 312
- Consumes canonical state: yes; reinterprets raw findings: no; blocking default: no
- Limited visible: yes; stale visible: yes
- Reason: Badge projection should preserve route-quality attention from canonical outcome state without falling back to raw finding counts.
- Receipt validation: pass

### User Surface Projection `lsp_route_quality_degraded_from_canonical_outcome`

- Surface: `lsp`
- Artifact: `editor actionable gap queue`
- Run status: `full`
- Top next action: `improve_repair_route_quality`
- Repair kind: `add_output_observer`
- Verify command: `cargo test -p ripr-swarm output_observer_009`
- Receipt command: `cargo xtask receipts write --packet output-observer-009 --canonical-gap gap:output-observer-009`
- Source alignment case: `receipt_unchanged_route_quality_alignment`
- Limitation category: ``
- Runtime repair command: ``
- Counts: actionable 8, raw findings 312
- Consumes canonical state: yes; reinterprets raw findings: no; blocking default: no
- Limited visible: yes; stale visible: yes
- Reason: LSP projection should preserve route-quality attention from canonical outcome state instead of routing another blind packet attempt.
- Receipt validation: pass

### User Surface Projection `pr_comment_route_quality_degraded_from_canonical_outcome`

- Surface: `pr_comment`
- Artifact: `pr evidence summary`
- Run status: `full`
- Top next action: `improve_repair_route_quality`
- Repair kind: `add_output_observer`
- Verify command: `cargo test -p ripr-swarm output_observer_009`
- Receipt command: `cargo xtask receipts write --packet output-observer-009 --canonical-gap gap:output-observer-009`
- Source alignment case: `receipt_unchanged_route_quality_alignment`
- Limitation category: ``
- Runtime repair command: ``
- Counts: actionable 8, raw findings 312
- Consumes canonical state: yes; reinterprets raw findings: no; blocking default: no
- Limited visible: yes; stale visible: yes
- Reason: PR comments should preserve canonical non-success route-quality state instead of converting every full actionable row into another attempt-ready packet.
- Receipt validation: pass

### User Surface Projection `ci_route_quality_degraded_from_canonical_outcome`

- Surface: `ci`
- Artifact: `generated CI summary`
- Run status: `full`
- Top next action: `improve_repair_route_quality`
- Repair kind: `add_output_observer`
- Verify command: `cargo test -p ripr-swarm output_observer_009`
- Receipt command: `cargo xtask receipts write --packet output-observer-009 --canonical-gap gap:output-observer-009`
- Source alignment case: `receipt_unchanged_route_quality_alignment`
- Limitation category: ``
- Runtime repair command: ``
- Counts: actionable 8, raw findings 312
- Consumes canonical state: yes; reinterprets raw findings: no; blocking default: no
- Limited visible: yes; stale visible: yes
- Reason: Default CI projection should preserve route-quality attention from canonical outcome state while remaining advisory.
- Receipt validation: pass

### User Surface Projection `pr_comment_missing_receipt_from_canonical_outcome`

- Surface: `pr_comment`
- Artifact: `pr evidence summary`
- Run status: `full`
- Top next action: `collect_missing_attempt_receipts`
- Repair kind: `add_exact_error_variant_observer`
- Verify command: `cargo test -p ripr-swarm error_variant_012`
- Receipt command: `cargo xtask receipts write --packet error-variant-012 --canonical-gap gap:error-variant-012`
- Source alignment case: `attempted_no_receipt_route_quality_alignment`
- Limitation category: ``
- Runtime repair command: ``
- Counts: actionable 8, raw findings 312
- Consumes canonical state: yes; reinterprets raw findings: no; blocking default: no
- Limited visible: yes; stale visible: yes
- Reason: PR comments should preserve missing-receipt route-quality state instead of claiming a completed or improved repair attempt.
- Receipt validation: pass

### User Surface Projection `ci_summary_remains_advisory_over_canonical_state`

- Surface: `ci`
- Artifact: `generated CI summary`
- Run status: `full`
- Top next action: `attempt_ready_packet`
- Repair kind: `add_boundary_assertion`
- Verify command: `cargo test -p ripr-swarm boundary_discriminator_004`
- Receipt command: `cargo xtask receipts write --packet boundary-discriminator-004 --canonical-gap gap:boundary-discriminator-004`
- Source alignment case: `receipt_improved_top_next_action_alignment`
- Limitation category: ``
- Runtime repair command: ``
- Counts: actionable 8, raw findings 312
- Consumes canonical state: yes; reinterprets raw findings: no; blocking default: no
- Limited visible: yes; stale visible: yes
- Reason: Default CI should summarize canonical repair state while keeping blocking disabled unless explicitly configured.
- Receipt validation: pass

### User Surface Projection `badge_static_limitation_backlog_from_canonical_state`

- Surface: `badge`
- Artifact: `badge-basis`
- Run status: `full`
- Top next action: `route_static_limitation_backlog`
- Repair kind: `sharpen_static_limitation_route`
- Verify command: `cargo test -p ripr evidence_record_splits_assertion_target_owner_call_absence_limitation`
- Receipt command: `cargo xtask check-fixture-contracts`
- Source alignment case: `static_limitation_backlog_alignment`
- Limitation category: `activation_owner_call_absent_assertion_target_affinity`
- Runtime repair command: ``
- Counts: actionable 0, raw findings 1
- Consumes canonical state: yes; reinterprets raw findings: no; blocking default: no
- Limited visible: yes; stale visible: yes
- Reason: Badge projection should preserve static-limitation backlog state without showing it as an actionable repair count.
- Receipt validation: pass

### User Surface Projection `lsp_static_limitation_backlog_from_canonical_state`

- Surface: `lsp`
- Artifact: `editor actionable gap queue`
- Run status: `full`
- Top next action: `route_static_limitation_backlog`
- Repair kind: `sharpen_static_limitation_route`
- Verify command: `cargo test -p ripr evidence_record_splits_assertion_target_owner_call_absence_limitation`
- Receipt command: `cargo xtask check-fixture-contracts`
- Source alignment case: `static_limitation_backlog_alignment`
- Limitation category: `activation_owner_call_absent_assertion_target_affinity`
- Runtime repair command: ``
- Counts: actionable 0, raw findings 1
- Consumes canonical state: yes; reinterprets raw findings: no; blocking default: no
- Limited visible: yes; stale visible: yes
- Reason: LSP projection should surface the analyzer backlog route when no safe repair packet exists.
- Receipt validation: pass

### User Surface Projection `pr_comment_static_limitation_backlog_from_canonical_state`

- Surface: `pr_comment`
- Artifact: `pr evidence summary`
- Run status: `full`
- Top next action: `route_static_limitation_backlog`
- Repair kind: `sharpen_static_limitation_route`
- Verify command: `cargo test -p ripr evidence_record_splits_assertion_target_owner_call_absence_limitation`
- Receipt command: `cargo xtask check-fixture-contracts`
- Source alignment case: `static_limitation_backlog_alignment`
- Limitation category: `activation_owner_call_absent_assertion_target_affinity`
- Runtime repair command: ``
- Counts: actionable 0, raw findings 1
- Consumes canonical state: yes; reinterprets raw findings: no; blocking default: no
- Limited visible: yes; stale visible: yes
- Reason: PR comments should show the named limitation route as analyzer backlog work, not as a resolved or actionable repair packet.
- Receipt validation: pass

### User Surface Projection `ci_static_limitation_backlog_from_canonical_state`

- Surface: `ci`
- Artifact: `generated CI summary`
- Run status: `full`
- Top next action: `route_static_limitation_backlog`
- Repair kind: `sharpen_static_limitation_route`
- Verify command: `cargo test -p ripr evidence_record_splits_assertion_target_owner_call_absence_limitation`
- Receipt command: `cargo xtask check-fixture-contracts`
- Source alignment case: `static_limitation_backlog_alignment`
- Limitation category: `activation_owner_call_absent_assertion_target_affinity`
- Runtime repair command: ``
- Counts: actionable 0, raw findings 1
- Consumes canonical state: yes; reinterprets raw findings: no; blocking default: no
- Limited visible: yes; stale visible: yes
- Reason: Default CI should preserve static-limitation backlog routing as advisory analyzer work rather than blocking on raw findings.
- Receipt validation: pass

## PR Inline Comment Publisher Receipts

These receipts validate checked `comment-publish-plan.{json,md}` fixture outputs for the documented Campaign 26 inline-comment publisher routes. They verify opt-in modes, safe publish flags, summary-only exclusion, cap behavior, dedupe/upsert, stale-existing cleanup planning, fork or token blockers, missing-input blockers, and advisory limits without posting real PR comments.

- Default CI blocking: no
- Default inline comments: off
- Receipt outputs: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/<case>/comment-publish-plan.{json,md}`

| Case | Mode | Status | Publishable | Skipped | Blocked | Safe | Operations | Skip reasons | Block reasons |
| --- | --- | --- | ---: | ---: | ---: | --- | --- | --- | --- |
| `publishable_changed_line` | `plan` | `advisory` | 1 | 0 | 0 | no | `create` | `` | `` |
| `repair_card_copy` | `plan` | `advisory` | 1 | 0 | 0 | no | `create` | `` | `` |
| `summary_only_excluded` | `plan` | `advisory` | 0 | 1 | 0 | no | `` | `summary_only` | `` |
| `cap_overflow` | `plan` | `advisory` | 3 | 1 | 0 | no | `create` | `inline_comment_cap_reached` | `` |
| `dedupe_upsert` | `plan` | `advisory` | 2 | 0 | 0 | no | `keep, update` | `` | `` |
| `stale_existing` | `plan` | `advisory` | 0 | 0 | 0 | no | `delete` | `` | `` |
| `fork_or_no_token` | `inline` | `advisory` | 0 | 0 | 2 | no | `` | `` | `fork_untrusted, missing_token` |
| `missing_input` | `plan` | `advisory` | 0 | 0 | 1 | no | `` | `` | `missing_pr_guidance` |

### PR Inline Comment `publishable_changed_line`

- Mode: `plan`
- Expected mode: `plan`
- Status: `advisory`
- Expected status: `advisory`
- Counts: publishable 1, skipped 0, blocked 0
- Expected counts: publishable 1, skipped 0, blocked 0
- Safe to publish: false (expected false)
- Operations: `create`
- Expected operations: `create`
- Skip reasons: ``
- Expected skip reasons: ``
- Block reasons: ``
- Expected block reasons: ``
- Receipt JSON: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/publishable-changed-line/comment-publish-plan.json`
- Receipt Markdown: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/publishable-changed-line/comment-publish-plan.md`
- Expected report: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/publishable-changed-line/comment-publish-plan.json`
- Expected Markdown: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/publishable-changed-line/comment-publish-plan.md`
- Actual outputs: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/publishable-changed-line`
- Reason: plan mode with one changed-line comment eligible for create: Plan mode should show exactly what would be created while posting nothing.
- Errors: none

### PR Inline Comment `repair_card_copy`

- Mode: `plan`
- Expected mode: `plan`
- Status: `advisory`
- Expected status: `advisory`
- Counts: publishable 1, skipped 0, blocked 0
- Expected counts: publishable 1, skipped 0, blocked 0
- Safe to publish: false (expected false)
- Operations: `create`
- Expected operations: `create`
- Skip reasons: ``
- Expected skip reasons: ``
- Block reasons: ``
- Expected block reasons: ``
- Receipt JSON: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/repair-card-copy/comment-publish-plan.json`
- Receipt Markdown: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/repair-card-copy/comment-publish-plan.md`
- Expected report: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/repair-card-copy/comment-publish-plan.json`
- Expected Markdown: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/repair-card-copy/comment-publish-plan.md`
- Actual outputs: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/repair-card-copy`
- Reason: plan mode with a GapRecord-backed repair card: Inline planning should render GapRecord repair cards as bounded repair comments.
- Errors: none

### PR Inline Comment `summary_only_excluded`

- Mode: `plan`
- Expected mode: `plan`
- Status: `advisory`
- Expected status: `advisory`
- Counts: publishable 0, skipped 1, blocked 0
- Expected counts: publishable 0, skipped 1, blocked 0
- Safe to publish: false (expected false)
- Operations: ``
- Expected operations: ``
- Skip reasons: `summary_only`
- Expected skip reasons: `summary_only`
- Block reasons: ``
- Expected block reasons: ``
- Receipt JSON: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/summary-only-excluded/comment-publish-plan.json`
- Receipt Markdown: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/summary-only-excluded/comment-publish-plan.md`
- Expected report: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/summary-only-excluded/comment-publish-plan.json`
- Expected Markdown: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/summary-only-excluded/comment-publish-plan.md`
- Actual outputs: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/summary-only-excluded`
- Reason: summary-only guidance remains visible but is never inline-publishable: Summary-only recommendations should stay in Markdown/artifacts instead of being forced onto a line.
- Errors: none

### PR Inline Comment `cap_overflow`

- Mode: `plan`
- Expected mode: `plan`
- Status: `advisory`
- Expected status: `advisory`
- Counts: publishable 3, skipped 1, blocked 0
- Expected counts: publishable 3, skipped 1, blocked 0
- Safe to publish: false (expected false)
- Operations: `create`
- Expected operations: `create`
- Skip reasons: `inline_comment_cap_reached`
- Expected skip reasons: `inline_comment_cap_reached`
- Block reasons: ``
- Expected block reasons: ``
- Receipt JSON: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/cap-overflow/comment-publish-plan.json`
- Receipt Markdown: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/cap-overflow/comment-publish-plan.md`
- Expected report: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/cap-overflow/comment-publish-plan.json`
- Expected Markdown: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/cap-overflow/comment-publish-plan.md`
- Actual outputs: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/cap-overflow`
- Reason: more changed-line comments than the default cap: The default cap should keep durable review-thread noise bounded.
- Errors: none

### PR Inline Comment `dedupe_upsert`

- Mode: `plan`
- Expected mode: `plan`
- Status: `advisory`
- Expected status: `advisory`
- Counts: publishable 2, skipped 0, blocked 0
- Expected counts: publishable 2, skipped 0, blocked 0
- Safe to publish: false (expected false)
- Operations: `keep, update`
- Expected operations: `keep, update`
- Skip reasons: ``
- Expected skip reasons: ``
- Block reasons: ``
- Expected block reasons: ``
- Receipt JSON: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/dedupe-upsert/comment-publish-plan.json`
- Receipt Markdown: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/dedupe-upsert/comment-publish-plan.md`
- Expected report: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/dedupe-upsert/comment-publish-plan.json`
- Expected Markdown: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/dedupe-upsert/comment-publish-plan.md`
- Actual outputs: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/dedupe-upsert`
- Reason: existing comments match planned dedupe keys: Dedupe keys should produce update or keep operations instead of duplicate comments.
- Errors: none

### PR Inline Comment `stale_existing`

- Mode: `plan`
- Expected mode: `plan`
- Status: `advisory`
- Expected status: `advisory`
- Counts: publishable 0, skipped 0, blocked 0
- Expected counts: publishable 0, skipped 0, blocked 0
- Safe to publish: false (expected false)
- Operations: `delete`
- Expected operations: `delete`
- Skip reasons: ``
- Expected skip reasons: ``
- Block reasons: ``
- Expected block reasons: ``
- Receipt JSON: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/stale-existing/comment-publish-plan.json`
- Receipt Markdown: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/stale-existing/comment-publish-plan.md`
- Expected report: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/stale-existing/comment-publish-plan.json`
- Expected Markdown: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/stale-existing/comment-publish-plan.md`
- Actual outputs: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/stale-existing`
- Reason: existing RIPR comment no longer has a matching publishable recommendation: Stale prior RIPR comments should stay visible as planned cleanup instead of accumulating forever.
- Errors: none

### PR Inline Comment `fork_or_no_token`

- Mode: `inline`
- Expected mode: `inline`
- Status: `advisory`
- Expected status: `advisory`
- Counts: publishable 0, skipped 0, blocked 2
- Expected counts: publishable 0, skipped 0, blocked 2
- Safe to publish: false (expected false)
- Operations: ``
- Expected operations: ``
- Skip reasons: ``
- Expected skip reasons: ``
- Block reasons: `fork_untrusted, missing_token`
- Expected block reasons: `fork_untrusted, missing_token`
- Receipt JSON: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/fork-or-no-token/comment-publish-plan.json`
- Receipt Markdown: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/fork-or-no-token/comment-publish-plan.md`
- Expected report: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/fork-or-no-token/comment-publish-plan.json`
- Expected Markdown: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/fork-or-no-token/comment-publish-plan.md`
- Actual outputs: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/fork-or-no-token`
- Reason: inline mode requested without safe same-repo token context: Inline mode must no-op visibly when fork trust or token requirements are missing.
- Errors: none

### PR Inline Comment `missing_input`

- Mode: `plan`
- Expected mode: `plan`
- Status: `advisory`
- Expected status: `advisory`
- Counts: publishable 0, skipped 0, blocked 1
- Expected counts: publishable 0, skipped 0, blocked 1
- Safe to publish: false (expected false)
- Operations: ``
- Expected operations: ``
- Skip reasons: ``
- Expected skip reasons: ``
- Block reasons: `missing_pr_guidance`
- Expected block reasons: `missing_pr_guidance`
- Receipt JSON: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/missing-input/comment-publish-plan.json`
- Receipt Markdown: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/missing-input/comment-publish-plan.md`
- Expected report: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/missing-input/comment-publish-plan.json`
- Expected Markdown: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/missing-input/comment-publish-plan.md`
- Actual outputs: `fixtures/boundary_gap/expected/pr-inline-comment-publisher/missing-input`
- Reason: publish plan requested without review-comments input: Missing guidance input should produce a repairable no-op plan, not hidden success.
- Errors: none

## Gate Adoption Receipts

These receipts run `ripr gate evaluate` against checked boundary-gap PR guidance and calibration evidence. They are repo-local dogfood for explicit gate modes; generated CI still leaves `RIPR_GATE_MODE` unset unless the repository configures it.

- Default CI blocking: no
- Receipt outputs: `target/ripr/dogfood/gate-adoption/<case>/gate-decision.{json,md}`

| Case | Mode | Status | Blocking | Acknowledged | Advisory | Exit | Expected exit |
| --- | --- | --- | ---: | ---: | ---: | --- | --- |
| `visible-only-advisory` | `visible-only` | `advisory` | 0 | 0 | 1 | success | success |
| `acknowledged-waiver` | `acknowledgeable` | `acknowledged` | 0 | 1 | 0 | success | success |
| `baseline-check-existing` | `baseline-check` | `advisory` | 0 | 0 | 1 | success | success |
| `baseline-check-new-gap` | `baseline-check` | `blocked` | 1 | 0 | 0 | non-zero | non-zero |
| `calibrated-high-confidence-new-gap` | `calibrated-gate` | `blocked` | 1 | 0 | 0 | non-zero | non-zero |
| `missing-baseline-config` | `baseline-check` | `config_error` | 0 | 0 | 0 | non-zero | non-zero |

### Gate `visible-only-advisory`

- Mode: `visible-only`
- Status: `advisory`
- Expected status: `advisory`
- Decision counts: blocking 0, acknowledged 0, advisory 1
- Expected counts: blocking 0, acknowledged 0, advisory 1
- Gate decision JSON: `target/ripr/dogfood/gate-adoption/visible-only-advisory/gate-decision.json`
- Gate decision Markdown: `target/ripr/dogfood/gate-adoption/visible-only-advisory/gate-decision.md`
- Duration: 7 ms
- Errors: none

### Gate `acknowledged-waiver`

- Mode: `acknowledgeable`
- Status: `acknowledged`
- Expected status: `acknowledged`
- Decision counts: blocking 0, acknowledged 1, advisory 0
- Expected counts: blocking 0, acknowledged 1, advisory 0
- Gate decision JSON: `target/ripr/dogfood/gate-adoption/acknowledged-waiver/gate-decision.json`
- Gate decision Markdown: `target/ripr/dogfood/gate-adoption/acknowledged-waiver/gate-decision.md`
- Duration: 6 ms
- Errors: none

### Gate `baseline-check-existing`

- Mode: `baseline-check`
- Status: `advisory`
- Expected status: `advisory`
- Decision counts: blocking 0, acknowledged 0, advisory 1
- Expected counts: blocking 0, acknowledged 0, advisory 1
- Gate decision JSON: `target/ripr/dogfood/gate-adoption/baseline-check-existing/gate-decision.json`
- Gate decision Markdown: `target/ripr/dogfood/gate-adoption/baseline-check-existing/gate-decision.md`
- Duration: 5 ms
- Errors: none

### Gate `baseline-check-new-gap`

- Mode: `baseline-check`
- Status: `blocked`
- Expected status: `blocked`
- Decision counts: blocking 1, acknowledged 0, advisory 0
- Expected counts: blocking 1, acknowledged 0, advisory 0
- Gate decision JSON: `target/ripr/dogfood/gate-adoption/baseline-check-new-gap/gate-decision.json`
- Gate decision Markdown: `target/ripr/dogfood/gate-adoption/baseline-check-new-gap/gate-decision.md`
- Duration: 6 ms
- Errors: none

### Gate `calibrated-high-confidence-new-gap`

- Mode: `calibrated-gate`
- Status: `blocked`
- Expected status: `blocked`
- Decision counts: blocking 1, acknowledged 0, advisory 0
- Expected counts: blocking 1, acknowledged 0, advisory 0
- Gate decision JSON: `target/ripr/dogfood/gate-adoption/calibrated-high-confidence-new-gap/gate-decision.json`
- Gate decision Markdown: `target/ripr/dogfood/gate-adoption/calibrated-high-confidence-new-gap/gate-decision.md`
- Duration: 7 ms
- Errors: none

### Gate `missing-baseline-config`

- Mode: `baseline-check`
- Status: `config_error`
- Expected status: `config_error`
- Decision counts: blocking 0, acknowledged 0, advisory 0
- Expected counts: blocking 0, acknowledged 0, advisory 0
- Gate decision JSON: `target/ripr/dogfood/gate-adoption/missing-baseline-config/gate-decision.json`
- Gate decision Markdown: `target/ripr/dogfood/gate-adoption/missing-baseline-config/gate-decision.md`
- Duration: 6 ms
- Errors: none


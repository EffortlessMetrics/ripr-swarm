# RIPR PR Review

Status: blocked

Start here:
- State: actionable
- Source: first_useful_action
- Identity: 67fc764ba37d77bd
- File: src/lib.rs:2
- Repair route: focused_test
- Class: weakly_exposed
- Missing discriminator: input that hits the boundary: amount >= discount_threshold
- Suggested focused test: add amount >= discount_threshold boundary assertion
- Related test: tests/pricing.rs::below_threshold_has_no_discount
- Verify command: `ripr agent verify --root fixtures/boundary_gap/input --before target/ripr/workflow/before.repo-exposure.json --after target/ripr/workflow/after.repo-exposure.json --json`
- Receipt: missing
- Boundary: advisory static evidence only; gate authority remains separate and no runtime, coverage, mutation, or merge approval is implied.

Movement:
- New policy-eligible gaps: 1
- Blocking candidates: 1
- Static movement: unknown
- Coverage/grip: not available

Policy:
- Mode: calibrated-gate
- Decision: blocked
- Gate authority: fixtures/boundary_gap/expected/pr-review-front-panel/blocked/gate-decision.md
- Acknowledgement label: ripr-waive

Repair:
- Agent handoff: `ripr agent start --root fixtures/boundary_gap/input --seam-id 67fc764ba37d77bd --out target/ripr/workflow`
- Verify: `ripr agent verify --root fixtures/boundary_gap/input --before target/ripr/workflow/before.repo-exposure.json --after target/ripr/workflow/after.repo-exposure.json --json`
- Receipt: missing

Artifacts:
- Start here: fixtures/boundary_gap/expected/pr-review-front-panel/blocked/pr-review-front-panel.md
- Policy: fixtures/boundary_gap/expected/pr-review-front-panel/blocked/gate-decision.md
- Repair: fixtures/boundary_gap/expected/first-useful-action/actionable/first-useful-action.md

Limits:
- Static RIPR evidence only.
- Does not run mutation testing.
- Does not edit source or generate tests.
- Gate evaluator remains pass/fail authority.

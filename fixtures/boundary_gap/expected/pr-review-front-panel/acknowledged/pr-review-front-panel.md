# RIPR PR Review

Status: acknowledged

Start here:
- State: actionable
- Source: gate_decision
- Identity: ack
- File: src/ack.rs:5
- Repair route: focused_test
- Class: weakly_exposed
- Missing discriminator: ack == 5
- Suggested focused test: assert_eq!(ack(), 5)
- Related test: tests/ack.rs::boundary
- Verify command: not_available
- Receipt: missing
- Boundary: advisory static evidence only; gate authority remains separate and no runtime, coverage, mutation, or merge approval is implied.

Movement:
- New policy-eligible gaps: 0
- Acknowledged gaps: 1
- Suppressed gaps: 0
- Static movement: unknown
- Coverage/grip: not available

Policy:
- Mode: acknowledgeable
- Decision: acknowledged
- Acknowledgement label: ripr-waive
- Finding remains visible
- Gate authority: fixtures/boundary_gap/expected/pr-evidence-ledger/mixed/gate-decision.json

Artifacts:
- Start here: fixtures/boundary_gap/expected/pr-review-front-panel/acknowledged/pr-review-front-panel.md
- Policy: fixtures/boundary_gap/expected/pr-evidence-ledger/mixed/gate-decision.json
- Evidence: fixtures/boundary_gap/expected/pr-evidence-ledger/mixed/pr-evidence-ledger.json

Limits:
- Static RIPR evidence only.
- Does not run mutation testing.
- Does not edit source or generate tests.
- Gate evaluator remains pass/fail authority.

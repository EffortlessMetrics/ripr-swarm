# RIPR PR Review

Status: incomplete

Start here:
- State: missing required evidence
- Safe next action: regenerate the missing assistant proof artifact before acting on this panel.
- Missing input: fixtures/boundary_gap/input/missing-agent-receipt.json
- Boundary: advisory static evidence only; no gate, runtime, coverage, or mutation proof is implied.

Movement:
- New policy-eligible gaps: 0
- Baseline gaps still present: 0
- Baseline gaps resolved: 0
- Static movement: unknown
- Coverage/grip: not available

Policy:
- Decision: advisory
- Gate authority: not configured

Artifacts:
- Start here: fixtures/boundary_gap/expected/pr-review-front-panel/missing-receipt-with-valid-proof/pr-review-front-panel.md
- Repair: fixtures/boundary_gap/expected/first-useful-action/unchanged-after-attempt/assistant-proof.md
- Evidence: fixtures/boundary_gap/expected/first-useful-action/unchanged-after-attempt/first-useful-action.md

Limits:
- Static RIPR evidence only.
- Does not run mutation testing.
- Does not edit source or generate tests.
- Gate evaluator remains pass/fail authority.

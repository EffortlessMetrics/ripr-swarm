# RIPR Editor LSP Workflow Hover

Seam: `67fc764ba37d77bd`
File: `src/lib.rs:2`
Class: `weakly_gripped`
Kind: `predicate_boundary`

## Evidence Path

- reach: yes
- activate: yes
- propagate: yes
- observe: yes
- discriminate: yes

## Missing discriminator

- `discount_threshold (equality boundary)` — observed values do not include the equality-boundary case for this predicate

## Related tests

- `tests/pricing.rs::below_threshold_has_no_discount`
- `tests/pricing.rs::far_above_threshold_discounts`

## Suggested test shape

- file: `tests/pricing.rs`
- name: `discounted_total_boundary_discriminator`
- candidate value: `discount_threshold (equality boundary)`
- assertion shape: assert_eq!(discounted_total(/* boundary input where amount >= discount_threshold */), /* expected */)
- assertion template: `assert_eq!(discounted_total(/* boundary input where amount >= discount_threshold */), /* expected */)`

## Handoff, verify, and receipt commands

- repair (start here): `ripr agent repair --root <root> --seam-id 67fc764ba37d77bd --phase before`
- packet: `ripr agent packet --root <root> --seam-id 67fc764ba37d77bd --json > <root>/target/ripr/agent/agent-packet.json`
- brief: `ripr agent brief --root <root> --seam-id 67fc764ba37d77bd --json > <root>/target/ripr/agent/agent-brief.json`
- after snapshot: `ripr check --root <root> --base origin/main --mode fast --format repo-exposure-json > <root>/target/ripr/pilot/after.repo-exposure.json`
- verify: `ripr agent verify --root <root> --before target/ripr/pilot/repo-exposure.json --after target/ripr/pilot/after.repo-exposure.json --json > <root>/target/ripr/agent/agent-verify.json`
- receipt: `ripr agent receipt --root <root> --verify-json target/ripr/agent/agent-verify.json --seam-id 67fc764ba37d77bd --json --out target/ripr/agent/agent-receipt.json`

## Status projection

- Matching first-useful-action report: show `ripr: first action`.
- Stale saved-workspace evidence: keep `ripr: stale` visible and tell the user to refresh before acting.
- Wrong-root, malformed, or unsupported report: fail closed without adding diagnostics.

## Limits

- Static evidence only.
- Does not run mutation testing.
- Does not edit source or generate tests.
- Does not make policy or gate decisions.

# Repair Funnel Control Fixture Corpus

Spec: RIPR-SPEC-0377

## Given

Issue #6308 needs planted receipts that prove the first-hour repair funnel
projection and denominator controls fail closed. This corpus commits
`first_hour_repair_funnel_inputs.v1` packets and expected control outcomes for
`cargo xtask repair-funnel-report` and `cargo xtask check-fixture-contracts`.

## When

The offline report loads `controls/corpus.json`, projects each inputs packet to
`first_hour_repair_funnel.v1`, and compares live control violations to the
committed expectations.

## Then

- Governed verified-improvement inputs record verification plus static movement.
- Verification-without-movement and verification-not-run cases stay non-positive.
- Planted negative controls emit the expected `control_*` violations.

## Must Not

- Claim support-tier promotion or release readiness from fixture rows alone.
- Execute governed real-repository repair attempts from this corpus.

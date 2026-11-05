# Repair Funnel Control Fixture Corpus

Spec: RIPR-SPEC-0410

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


## Retained installed producer input

`installed-41383028929.json` pins 38 original files (60,271 bytes) imported
unchanged from GitHub Actions artifact 13114861147,
[run 41383028929, attempt 1](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/41383028929/attempts/1).
The downloaded ZIP was 71,960 bytes, SHA-256
`1fae393a8b77736785285663a76f91ac2e913dd05b9f8dc963424fcaa8578113`;
its ZIP integrity check passed. The manifest records every imported file digest.
The subset includes before/after checks, unchanged subject/source identities,
wheel/runner/invocation/admission, native positive/zero/failed receipts and their
six supporting raw command/result/stream sets. Other artifact files are not
implied to be reproduced here. No private transfer references are retained.

Producer source: `a3847eed4c4c89d3b9c5ff21ccbbc9dd7ad44965`, tree
`68015793efe98797a2f0b151abd7073e5bc72e8e`, from #12404 using #11919's
strict installed fixture. Installed analyzer SHA-256:
`fe97d22cb32227976fbca98bc05d418c280f043caea4d2b767a82c25cd64a735`.
Runner SHA-256:
`9f0bc1f286a53eff6de94b380304dc1ee214a65cffce1bf815e479cd70b9bd4c`.
This is historical head-built installed deterministic evidence, not current-main
qualification, released artifact proof, a governed real repair or voluntary use.

The retained native zero-selection and wrong-expected records are adverse
controls from that original execution. Adapter tests also mutate admitted
identities/streams, remove stages, duplicate a view and mislabel the cohort.
Those mutations are planted adapter controls, never new genuine journeys.
The original controls corpus remains separate and unmodified.

# Packaged repair parity qualification fixture (#4648 / #2929)

External two-commit Rust repository shared with
`fixtures/blind_journey_installed_rust`. This fixture defines the cross-channel
comparison contract and deterministic parity receipt shape used to qualify one
canonical `RepairAttempt` through an installed CLI and a packaged VSIX.

Synthetic qualification rows are excluded from real-repository dogfood
denominators (#1560, #1579, #1702).

## Artifact identity

A qualified channel, its qualified required section, or a qualified parity
verdict requires a nonempty artifact inventory for the affected channel. Every
identity must contain a non-whitespace character. Contract-only validation
allows missing identities on channels with no qualification claim.

The `packaged_repair_receipt_matching_semantics_cannot_replace_artifact_identity`
and `packaged_repair_receipt_each_qualification_claim_requires_artifact_identity`
gate regressions exercise these rules. Artifact custody still verifies the
actual installed bytes independently.

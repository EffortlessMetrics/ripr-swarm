# Packaged repair parity qualification fixture (#4648 / #2929)

External two-commit Rust repository shared with
`fixtures/blind_journey_installed_rust`. This fixture defines the cross-channel
comparison contract and deterministic parity receipt shape used to qualify one
canonical `RepairAttempt` through an installed CLI and a packaged VSIX.

Synthetic qualification rows are excluded from real-repository dogfood
denominators (#1560, #1579, #1702).

<!-- section: Docs -->
- Added `docs/FROM_CARGO_MUTANTS.md`, a guide for teams switching from cargo-mutants: what maps to what in static terms, `ripr check --format missed-txt` as the `missed.txt` equivalent, and a verified two-command `acknowledgeable` gate (exit `0` pass, `3` blocked, `2` incomplete) replacing a cargo-mutants exit-`2` CI check (#13627, #13626)

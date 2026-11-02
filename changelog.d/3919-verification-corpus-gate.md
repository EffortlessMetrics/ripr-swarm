<!-- section: Internal / CI -->
- `cargo xtask check-verification-contracts --check` now validates every committed producer document bound by `VERIFICATION_CORPUS_BINDINGS` on the working tree and reports discovered versus validated counts, instead of relying on hand-maintained subject rows alone (#3919).

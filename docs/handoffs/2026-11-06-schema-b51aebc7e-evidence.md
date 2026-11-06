# Scoped schema proof for the 0.11 candidate

Tested candidate: `b51aebc7e4735529befd677ebf14071c7f09e477`.
Tested tree: `d3142dd59d4dcfea5e6b4981732f61c8516f3fa0`.

The five-producer live replay passed. Supplemental real CLI timeout-refusal
output and Rust/Python `review-comments --gap-ledger` route output also passed
the full schema validator through `--artifact`. The combined sweep validated
980 committed producer subjects and eight live artifacts. Its final rows digest
is `b1ecc45e0da4b1fcdf855123944b7ca29c634aa602743b404048b68796856c92`.
No combined-sweep row remained `not_run` or `authority_only`; reserved and
capability exemptions remain exemptions.

Scoped checks:

- `schema-producer-sweep --rev b51aebc7e4735529befd677ebf14071c7f09e477 --live-replay`
- `check-verification-contracts --check`
- `cargo test --locked -p xtask --bin xtask verification_contracts`: 71 passed.
- `cargo test --locked -p xtask --bin xtask schema_producer_sweep`: 29 passed.
- `cargo test --locked -p xtask --bin xtask gap_ledger_route_`: 2 passed.
- `cargo test --locked -p xtask --bin xtask schema_cut_replay`: 2 passed.

All 104 scoped tests passed with zero failures. The first two commands name
xtask subcommands. The final combined sweep additionally supplied the five
replayed producer artifacts, timeout-refusal output and both gap-ledger route
outputs through `--artifact`; the live replay alone does not cover those
supplemental routes.

These results establish scoped producer/schema conformance on the named
candidate. They do not establish analysis correctness, full-library
qualification, source-integration qualification, downstream consumer
compatibility, issue acceptance or release readiness. Acceptance remains
`scope_pending`; later integration must assess applicability to its actual
tree. Source attribution and the public-promotion holds in #1893 and draft
#1889 remain unchanged.

# Scoped schema proof for candidate a157a1d33

Tested subject: `a157a1d33fca88187e532cba4f1e3f49b97b0f02`.
Tested tree: `7d17ca7d4dcbd420cbfbd13426b8f6f580cc3773`.
The tested source remained clean.

All 104 scoped tests passed with zero failures:

- `cargo test --locked -p xtask --bin xtask verification_contracts`: 71 passed.
- `cargo test --locked -p xtask --bin xtask schema_producer_sweep`: 29 passed.
- `cargo test --locked -p xtask --bin xtask gap_ledger_route_`: 2 passed.
- `cargo test --locked -p xtask --bin xtask schema_cut_replay`: 2 passed.

The xtask commands `schema-producer-sweep --rev a157a1d33fca88187e532cba4f1e3f49b97b0f02 --live-replay`
and `check-verification-contracts --check` passed. Supplemental sweeps supplied
actual producer bytes through `--artifact`. The receipts validated 986 committed
producer subjects with these live-artifact counts and rows SHA256 digests:

| Receipt | Live artifacts | Rows SHA256 |
| --- | ---: | --- |
| Five-producer replay | 5 | `19c8f899fc2c1f057b2bec14a8be1fcec2ec42533ae1cca2168cbac370777204` |
| Replay plus timeout and Rust/Python gap-ledger outputs | 8 | `163eba48218fcc0a86c818cae25c8dbe08debcbe13ebbb561dcb606cab2f8707` |
| Extended same-key, observer and destructor outputs | 24 | `0c373285ef7ec1526612bafa5193dac1bee37f27d5206a8eec8b149371c208f4` |

The extended receipt includes six same-key cold/warm reports, two observer
reports and eight destructor reports in addition to the original eight
artifacts. Same-key findings agree at related-test totals 6, 8 and 9, and human
totals agree with JSON. The opaque observer retains its named `static_unknown`
limitation. Nontrivial destructor controls withhold exposure: implicit syntax
emits `static_unknown`, while explicit projection emits `weakly_exposed`.
Those distinctions and the original expectations were preserved. No schema
incompatibility was found in these bounded controls.

This is scoped evidence for the named preparation candidate, not final release
selection or successor qualification. Prior-candidate evidence remains distinct.
These results do not establish full-library or file-policy qualification,
accepted-main wording/golden refresh, analysis correctness, downstream consumer
compatibility, issue acceptance or release readiness. Acceptance remains
`scope_pending`. Reserved and capability exemptions remain exemptions; the two
combined receipts have no `not_run` or `authority_only` rows. Source attribution
and existing trust and promotion holds, including #1893 and draft #1889, remain
unchanged.

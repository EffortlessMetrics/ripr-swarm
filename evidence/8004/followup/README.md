# #8004 follow-up evidence

Additive validation packet for [Draft PR #8004](https://github.com/EffortlessMetrics/ripr-swarm/pull/8004). It contains no source changes or remote writes. Full frozen executables and the 1.17 MB hosted ZIP are omitted; `manifest.json` records their exact digests and sizes.

## Frozen local composition

Candidate head `f6aab7639b88be1a21c757be74baef833e636f17` (tree `81af6df...`) was tested with frozen main `0e8d006e9845339a58902bb2baa28dbc4fd23c18` as local composition commit `ba04f2af1a92caf404e0686f53caf2a7b4d85ac8`, tree `2ad0071df8e57d254ee7b52145f6135a45a74d6a`. The merge tree was clean; two paths overlapped. On this fixed composition, the focused default-feature lib tests passed 2/2 (10,836 filtered), the receiver integration suite passed 6/6, and `check-fast` passed all 14 gates; `check-fixture-contracts` was skipped as not applicable. These are targeted/frozen-base results, not a full workspace-suite claim or a moving-main claim.

## Hosted ub-review artifact

The hosted ZIP is identified by SHA-256 `2937e1bd32f553156d656ad86f8534db588a93b8bd15bcffa2fc317e26e990d9` (1,170,527 bytes). Its revision admission binds PR head `f6aab763...` to merge commit `7ab1edc51a1139390b5d819b2ba387018919b143`, tree `2ad0071...`, and action `55e4fbab879b3a83c31f35cdc9f7e5cd99f0a6c8`.

The artifact reports a `pass` gate, but its terminal status is `needs-reviewer-attention`: required sensor total is 0; 3 of 5 optional sensors completed, 2 failed; model coverage is 10 of 11 lanes usable. The two optional failures are preserved separately: `cargo-test --workspace --all-targets --locked` exited 101 after 10,836 passed, 1 failed, 1 ignored; `ripr check --root . --diff target/ub-review/input/diff.patch --mode ready --format badge-json` exited 2 and states that 1,229 indexed Rust files exceeded its 800-file limit, so analysis was not run. The exact test excerpt requests preparing canonical fixture dependencies. This packet does not infer a root cause or call either result inherited.

The separate `rust-only-hosted-projection.json` reports a successful no-default-features + `lang-rust` hosted lane. It is not the default-feature hosted artifact and is kept as a distinct evidence lane; its aggregate is a sum of per-binary summary lines, not a deduplicated census. `review/proof-lane-identities.json` keeps the earlier author default-feature producer and independent reviewer Rust-only producer separate, including the local reviewed commit versus published candidate source identity. The composition checks and hosted artifacts are also separate lanes.

## Later adversarial review

A later independent adversarial review at the same frozen composition found a counterexample: a unit struct declared in the method-call operand shadows an outer variable, but the analyzer still labels `value.as_object()` as `direct_owner_call` with `high` confidence. The new control receipt shows this in both diff and repo modes. A native Rust 1.95 witness confirms the call dispatches to blanket-implemented `ForeignView` using an atomic counter; the standalone control passes 3/3. A mutation changing the private `Object` arm to return `None` leaves the shadow-call and null controls passing but fails the actual private-object control (2 pass, 1 fail). See `review/` for sources, receipt projection, logs and raw red analyzer reports. Semantic clearance is withdrawn pending repair and independent review; this is not a Ready decision.

## Historical results and boundaries

Earlier default-feature SIGKILL receipts remain recorded in the terminal validation summary by hash. The two exact-published-head focused unit assertions passed 2/10,814 filtered; frozen-composition focused unit assertions passed 2/10,836 filtered. Neither is a full-suite result, and neither clears the later adversarial finding. The original golden check ran 425 fixtures with zero fixture errors but exited 1 on `owner_return_pin_fallible_literal`; normalized baseline and candidate JSON were equal after the normalizer removed `canonical_next_action`, no expected output was blessed, and the golden failure was reproduced at baseline. The hosted optional cargo-test failure above is separate; this packet asserts no inherited status or root cause for it.

This evidence does not establish native Windows, separate #7817/#7973 composition, exact Cargo-target or complete Rust type identity, type-name collision safety, constructor or alias/factory/parameter resolution, general value flow, nested/custom/file-scope macro expansion, arbitrary target roots, or general helper last-hop receiver identity. Unsupported evidence may remain unknown. #7415 remains open; no Ready or merge claim is made.

`composition/check-fast-report-summaries.json` contains the 14 check-fast report digests/statuses. `manifest.json` contains exact bundled and omitted-artifact hashes.

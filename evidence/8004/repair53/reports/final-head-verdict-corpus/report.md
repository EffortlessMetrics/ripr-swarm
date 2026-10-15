# Rust verdict corpus report

Spec: RIPR-SPEC-0219. Cases: 485.

| Rate | Count | Rate |
| --- | --- | --- |
| False verdicts (all cases) | 26/485 | 0.0536 |
| False actionable (of discriminated) | 23/238 | 0.0966 |
| False exposed (of not fully discriminated) | 3/247 | 0.0121 |
| False silent (of not fully discriminated) | 0/247 | 0.0000 |
| Ideal verdict | 279/485 | 0.5753 |
| Abstained (limited or silent where acceptable) | 180/485 | 0.3711 |
| Findings with a contradiction | 0/621 | 0.0000 |

By subject origin. Authored cases are written to fill cells the upstream cases leave empty, so only the upstream rates describe real-world tests.

| Origin | Cases | False verdicts | False actionable | False exposed | False silent | Ideal | Abstained |
| --- | --- | --- | --- | --- | --- | --- | --- |
| authored | 449 | 20/449 | 17/216 | 3/233 | 0/233 | 276/449 | 153/449 |
| upstream | 36 | 6/36 | 6/22 | 0/14 | 0/14 | 3/36 | 27/36 |

| Case | Origin | Truth | Ideal | Observed | Classes | Outcome | Changed since labeling | Contradictions |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `accounts-balance-add` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `accounts-last-byte-unchecked` | authored | not_discriminated | gap | limited | static_unknown | abstained | no | none |
| `accounts-parse-too-long-variant` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `accounts-trailer-crc` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `atuin-ai-history-output-capability` | upstream | not_discriminated | gap | gap | infection_unknown, weakly_exposed | ideal | no | none |
| `atuin-otel-traces-suffix-not` | upstream | not_discriminated | gap | limited | infection_unknown | abstained | no | none |
| `bytesize-as-kb-div` | upstream | not_discriminated | gap | limited | no_static_path | abstained | no | none |
| `bytesize-as-kib-div` | upstream | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `bytesize-as-mb-div` | upstream | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `bytesize-as-mib-div` | upstream | not_discriminated | gap | limited | no_static_path | abstained | no | none |
| `bytesize-format-unit-first-run` | upstream | discriminated | credited | limited | infection_unknown | abstained | no | none |
| `cell-0005-same-name-owner-tested` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `cell-0005-same-name-owner-unreached` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `cell-0005-shadowed-field-binding` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `cell-0005-wrapper-field-binding` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `cell-0094-const-token-confirmed` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `cell-0094-mixed-confirmation` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `cell-0096-drop-of-stored-result` | authored | discriminated | credited | limited | propagation_unknown | abstained | no | none |
| `cell-0096-eprintln-flag-only` | authored | not_discriminated | gap | limited | propagation_unknown | abstained | yes | none |
| `cell-0096-log-info-captured` | authored | discriminated | credited | limited | static_unknown | abstained | yes | none |
| `cell-0096-log-info-smoke-only` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `cell-0153-xtask-edit-seeds` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `cell-0166-cargo-bin-stderr-unread` | authored | not_discriminated | gap | limited | no_static_path | abstained | no | none |
| `cell-0166-cargo-bin-stdout` | authored | discriminated | credited | limited | no_static_path | abstained | no | none |
| `cell-0173-trial-callback-pin` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `cell-0173-trial-never-run` | authored | not_discriminated | gap | limited | no_static_path | abstained | no | none |
| `cell-0173-trial-unwrap-only` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `cell-0197-kept-default` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `cell-0197-ok-return-path` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `cell-0197-other-return-path` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `cell-0197-overridden-default` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `cell-0231-log-raw-literal-captured` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `cell-0231-log-targeted-other-record` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `cell-0231-log-targeted-record-captured` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `checkout-announce-stdout-sink` | authored | not_discriminated | gap | limited | propagation_unknown | abstained | no | none |
| `checkout-bulk-custom-assert-macro` | authored | discriminated | credited | limited | weakly_exposed | abstained | yes | none |
| `checkout-daily-limit-imported-const` | authored | discriminated | credited | credited | exposed, propagation_unknown | ideal | yes | none |
| `checkout-deposit-cap-happy-path-only` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `checkout-fee-assert-under-false-flag` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `checkout-fee-cfg-disabled-test` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `checkout-fee-closure-never-called` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `checkout-fee-err-return-guard` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `checkout-fee-unpolled-async-assert` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `checkout-minimum-same-file-const` | authored | discriminated | credited | credited | exposed, propagation_unknown | ideal | yes | none |
| `checkout-persist-swallowed-ok` | authored | discriminated | credited | limited | propagation_unknown | abstained | no | none |
| `checkout-rate-same-method-other-type` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `checkout-record-discarded-result` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `checkout-refund-matches-variant` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `checkout-region-literal-match-helper` | authored | discriminated | credited | limited | propagation_unknown | abstained | yes | none |
| `checkout-review-split-boundary-tests` | authored | not_discriminated | gap | gap | exposed, weakly_exposed | ideal | yes | none |
| `checkout-tax-self-computed-expected` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `checkout-withdraw-guarded-match-pin` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `checkout-withdraw-sibling-variant` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `config-bool-false-arm` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `config-default-host` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `config-duplicate-key-case` | authored | not_discriminated | gap | limited | infection_unknown | abstained | no | none |
| `config-empty-key-error` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `config-log-level-warn` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `config-missing-equals-line` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `config-port-zero` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `grid-arith-exact` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `grid-arith-helper` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `grid-arith-none` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `grid-arith-property` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `grid-arith-table` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `grid-bindings-computed-arg` | authored | discriminated | credited | limited | infection_unknown | abstained | yes | none |
| `grid-bindings-reassigned` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `grid-bindings-shadowed` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `grid-bits-length-loose` | authored | partially_discriminated | gap | limited | infection_unknown | abstained | yes | none |
| `grid-bits-length-pinned` | authored | discriminated | credited | limited | infection_unknown | abstained | yes | none |
| `grid-bits-or-loose` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `grid-bits-shift-pinned` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `grid-bool-exact` | authored | discriminated | credited | limited | infection_unknown, propagation_unknown | abstained | no | none |
| `grid-bool-helper` | authored | discriminated | credited | credited | exposed, propagation_unknown | ideal | yes | none |
| `grid-bool-none` | authored | not_discriminated | gap | limited | infection_unknown, propagation_unknown | abstained | no | none |
| `grid-bool-property` | authored | partially_discriminated | gap | limited | infection_unknown, propagation_unknown | abstained | no | none |
| `grid-bool-table` | authored | discriminated | credited | limited | infection_unknown, propagation_unknown | abstained | no | none |
| `grid-boundary-exact` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `grid-boundary-helper` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `grid-boundary-none` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `grid-boundary-property` | authored | partially_discriminated | gap | gap | exposed, weakly_exposed | ideal | yes | none |
| `grid-boundary-table` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `grid-delete-exact` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `grid-delete-helper` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `grid-delete-none` | authored | partially_discriminated | gap | limited | static_unknown | abstained | no | none |
| `grid-delete-property` | authored | partially_discriminated | gap | limited | static_unknown | abstained | no | none |
| `grid-delete-table` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `grid-early-exact` | authored | discriminated | credited | limited | infection_unknown | abstained | yes | none |
| `grid-early-helper` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `grid-early-none` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `grid-early-property` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `grid-early-table` | authored | discriminated | credited | limited | infection_unknown | abstained | yes | none |
| `grid-equality-exact` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `grid-equality-helper` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `grid-equality-none` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `grid-equality-property` | authored | partially_discriminated | gap | limited | infection_unknown, propagation_unknown | abstained | no | none |
| `grid-equality-table` | authored | discriminated | credited | gap | infection_unknown, weakly_exposed | false_actionable | yes | none |
| `grid-flow-else-loose` | authored | not_discriminated | gap | limited | static_unknown | abstained | no | none |
| `grid-flow-else-pinned` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `grid-flow-err-guard` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `grid-flow-field-direct` | authored | discriminated | credited | limited | infection_unknown, propagation_unknown | abstained | no | none |
| `grid-flow-field-opaque` | authored | discriminated | credited | credited | exposed, infection_unknown | ideal | yes | none |
| `grid-iter-exact` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `grid-iter-helper` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `grid-iter-none` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `grid-iter-property` | authored | partially_discriminated | gap | gap | infection_unknown, weakly_exposed | ideal | yes | none |
| `grid-iter-table` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `grid-loop-exact` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `grid-loop-helper` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `grid-loop-none` | authored | partially_discriminated | gap | limited | static_unknown | abstained | no | none |
| `grid-loop-property` | authored | partially_discriminated | gap | limited | static_unknown | abstained | no | none |
| `grid-loop-table` | authored | discriminated | credited | limited | static_unknown | abstained | yes | none |
| `grid-match-exact` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `grid-match-guard` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `grid-match-helper` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `grid-match-none` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `grid-match-property` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `grid-match-table` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `grid-results-routed-match` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `grid-results-swallowed-err` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `grid-returns-exact` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `grid-returns-helper` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `grid-returns-none` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `grid-returns-property` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `grid-returns-table` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `grid-try-exact` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `grid-try-helper` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `grid-try-none` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `grid-try-property` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `grid-try-table` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `hex-decode-slice-length` | upstream | partially_discriminated | gap | limited | infection_unknown | abstained | yes | none |
| `hex-decode-slice-odd` | upstream | discriminated | credited | limited | infection_unknown | abstained | yes | none |
| `hex-encode-slice-length` | upstream | partially_discriminated | gap | limited | infection_unknown | abstained | yes | none |
| `hex-from-hex-odd-variant` | upstream | discriminated | credited | credited | exposed | ideal | yes | none |
| `itoa-four-digit-loop` | upstream | not_discriminated | gap | limited | no_static_path | abstained | no | none |
| `itoa-two-digit-tail` | upstream | not_discriminated | gap | limited | no_static_path | abstained | no | none |
| `ledger-insufficient-available` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `ledger-receipt-remaining` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `ledger-receive-refresh-low-stock` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `ledger-ship-exact-stock` | authored | not_discriminated | gap | limited | infection_unknown | abstained | no | none |
| `ledger-ship-log-push` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `ledger-shipped-total` | authored | partially_discriminated | gap | limited | static_unknown | abstained | no | none |
| `ledger-sku-family-end` | authored | discriminated | credited | limited | infection_unknown | abstained | no | none |
| `ledger-sku-family-unsafe` | authored | discriminated | credited | credited | exposed, static_unknown | ideal | no | none |
| `ledger-sku-variant-unsafe` | authored | partially_discriminated | gap | gap | static_unknown, weakly_exposed | ideal | yes | none |
| `ledger-stock-insert` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `lib-approx-abs-diff-loose-epsilon` | authored | partially_discriminated | gap | limited | weakly_exposed | abstained | yes | none |
| `lib-approx-relative-eq-tight` | authored | discriminated | credited | limited | weakly_exposed | abstained | yes | none |
| `lib-assert-matches-guarded-arm-partial` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `lib-assert-matches-ok-wildcard-side-only` | authored | not_discriminated | gap | limited | static_unknown | abstained | no | none |
| `lib-assert-matches-variant-field-pinned` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `lib-claims-assert-ge-relational` | authored | partially_discriminated | gap | limited | reachable_unrevealed | abstained | no | none |
| `lib-claims-assert-ok-unwrap-then-eq` | authored | discriminated | credited | limited | reachable_unrevealed | abstained | no | none |
| `lib-claims-assert-some-eq-pinned` | authored | discriminated | credited | limited | infection_unknown, reachable_unrevealed | abstained | yes | none |
| `lib-claims-assert-some-side-only` | authored | not_discriminated | gap | limited | infection_unknown, reachable_unrevealed | abstained | yes | none |
| `lib-criterion-checksum-bench-only` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `lib-criterion-mix-unit-control` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `lib-criterion-window-bench-assert` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `lib-expect-test-assert-debug-eq-variant-payload` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `lib-expect-test-expect-file-golden` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `lib-expect-test-header-only-expectation` | authored | not_discriminated | gap | limited | static_unknown | abstained | no | none |
| `lib-expect-test-inline-assert-eq` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `lib-float-cmp-approx-eq-loose-in-assert` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `lib-float-cmp-assert-ulps` | authored | discriminated | credited | limited | weakly_exposed | abstained | yes | none |
| `lib-insta-debug-file-snapshot-tier-boundary` | authored | discriminated | credited | limited | infection_unknown | abstained | yes | none |
| `lib-insta-inline-snapshot-line-total` | authored | discriminated | credited | limited | propagation_unknown | abstained | yes | none |
| `lib-insta-json-redacted-id` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `lib-insta-json-unredacted-sibling-field` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `lib-mockall-body-unpinned-always` | authored | not_discriminated | gap | limited | static_unknown | abstained | no | none |
| `lib-mockall-grace-boundary-times` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `lib-mockall-recipient-predicate` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `lib-pretty-discount-assert-ne` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `lib-pretty-line-struct-literal` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `lib-pretty-render-shadowed-assert-eq` | authored | discriminated | credited | limited | static_unknown | abstained | yes | none |
| `lib-pretty-total-path-qualified` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `lib-proptest-encode-roundtrip` | authored | partially_discriminated | gap | limited | no_static_path | abstained | no | none |
| `lib-proptest-even-sum-model` | authored | discriminated | credited | limited | no_static_path | abstained | no | none |
| `lib-proptest-parse-no-panic` | authored | not_discriminated | gap | limited | no_static_path | abstained | no | none |
| `lib-proptest-percent-bound` | authored | partially_discriminated | gap | limited | no_static_path | abstained | no | none |
| `lib-quickcheck-macro-relational-bound` | authored | partially_discriminated | gap | gap | reachable_unrevealed | ideal | no | none |
| `lib-quickcheck-model-comparison` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `lib-quickcheck-roundtrip-involution` | authored | partially_discriminated | gap | gap | reachable_unrevealed | ideal | no | none |
| `lib-rstest-case-attribute-expected` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `lib-rstest-fixture-injected-receiver` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `lib-rstest-named-cases-skip-boundary` | authored | partially_discriminated | gap | gap | exposed, weakly_exposed | ideal | yes | none |
| `lib-rstest-values-matrix-relational` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `lib-serial-test-batch-default-relational` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `lib-serial-test-retries-default-arm` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `lib-serial-test-ticket-fetch-add` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `lib-test-case-args-body-assert-eq` | authored | discriminated | credited | limited | static_unknown | abstained | yes | none |
| `lib-test-case-arrow-expected-value` | authored | discriminated | credited | limited | static_unknown | abstained | yes | none |
| `lib-test-case-matches-err-wildcard` | authored | not_discriminated | gap | limited | infection_unknown | abstained | yes | none |
| `lib-test-case-with-closure-bound` | authored | partially_discriminated | gap | limited | static_unknown | abstained | yes | none |
| `lib-tokio-multi-thread-timeout-is-ok` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `lib-tokio-spawn-await-unwrap` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `lib-tokio-spawn-handle-dropped` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `lib-tokio-test-await-assert-eq` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `lib-trybuild-compile-fail-const-in-stderr` | authored | discriminated | credited | limited | no_static_path | abstained | no | none |
| `lib-trybuild-compile-fail-type-only` | authored | not_discriminated | gap | limited | no_static_path | abstained | no | none |
| `lib-trybuild-pass-file-runs-assert` | authored | discriminated | credited | limited | no_static_path | abstained | no | none |
| `mined-allowlist-listed-row-ftp` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `mined-allowlisted-row-gopher` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `mined-blanket-trait-negative-zero` | authored | discriminated | credited | limited | weakly_exposed | abstained | yes | none |
| `mined-bool-property-read-u32` | authored | discriminated | credited | limited | propagation_unknown | abstained | yes | none |
| `mined-debug-assert-after-index` | authored | not_discriminated | gap | limited | propagation_unknown | abstained | no | none |
| `mined-debug-assert-only-oracle` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `mined-doctest-ignored-read-u16-le` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `mined-doctest-only-read-u16` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `mined-hex-table-tested-row` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `mined-hex-table-unasserted-row` | authored | not_discriminated | gap | limited | static_unknown | abstained | no | none |
| `mined-macro-closure-header-length` | authored | discriminated | credited | limited | weakly_exposed | abstained | yes | none |
| `mined-multi-line-struct-literal-field` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `mined-one-line-struct-literal-field` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `mined-roundtrip-symmetric-mask` | authored | not_discriminated | gap | limited | static_unknown | abstained | no | none |
| `namesakes-delay-sibling-file-test` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `namesakes-render-qualified-other-module` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `namesakes-round-use-imported-rival` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `namesakes-snap-use-imported-owner` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `old-about-text-unobserved` | authored | not_discriminated | gap | limited | static_unknown | abstained | no | none |
| `old-code-too-long-exact-value-only` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `old-deps-same-type-other-dependency` | authored | not_discriminated | gap | limited | no_static_path | abstained | no | none |
| `old-deps-type-path-aliased-import` | authored | discriminated | credited | limited | no_static_path | abstained | no | none |
| `old-deps-type-path-direct-import` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `old-deps-type-path-glob-import` | authored | discriminated | credited | limited | no_static_path | abstained | no | none |
| `old-head-unsafe-shared-edge-line` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `old-net-self-equality` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `old-page-size-env-lookup-default` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `old-rate-macro-arg-no-mention` | authored | discriminated | credited | limited | propagation_unknown | abstained | yes | none |
| `old-ratio-assert-only-owner` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `old-retry-default-overridden` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `old-settle-mock-ignores-amount` | authored | not_discriminated | gap | limited | static_unknown | abstained | no | none |
| `old-share-assert-owner-with-caller` | authored | partially_discriminated | gap | limited | static_unknown | abstained | no | none |
| `old-shell-banner-sh-negative` | authored | not_discriminated | gap | limited | no_static_path | abstained | no | none |
| `old-slot-downcast-type-only` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `old-surcharge-call-expression-input` | authored | discriminated | credited | limited | infection_unknown | abstained | yes | none |
| `old-tag-boxed-into-wrapper` | authored | partially_discriminated | gap | limited | weakly_exposed | abstained | no | none |
| `old-tip-duplicative-tests` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `old-title-through-format-macro` | authored | discriminated | credited | limited | propagation_unknown | abstained | yes | none |
| `old-tool-build-script-limit` | authored | discriminated | credited | limited | no_static_path | abstained | no | none |
| `old-tool-exit-code-subprocess` | authored | discriminated | credited | limited | no_static_path | abstained | no | none |
| `old-unit-assert-matches-macro` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `old-unit-assert-matches-sibling-arm` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `old-usage-text-observed` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `old-version-text-printed-only` | authored | not_discriminated | gap | limited | static_unknown | abstained | no | none |
| `pricing-flat-shipping-fee` | authored | not_discriminated | gap | limited | static_unknown | abstained | no | none |
| `pricing-free-shipping-boundary` | authored | partially_discriminated | gap | gap | exposed, weakly_exposed | ideal | no | none |
| `pricing-gold-discount-rate` | authored | discriminated | credited | limited | propagation_unknown | abstained | yes | none |
| `pricing-gold-threshold` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `pricing-quote-total-field` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `pricing-tier-label-gold` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `regex-syntax-max-scalar-two-byte` | upstream | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `regex-syntax-word-byte` | upstream | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `roles-cfg-test-helper-input` | authored | discriminated | credited | silent | none | abstained | no | none |
| `roles-limit-const-value` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `rusqlite-singlethreaded-magic` | upstream | discriminated | credited | limited | no_static_path, static_unknown | abstained | no | none |
| `rust-field-projected-collection-arg` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `rust-guarded-push-unentered-branch` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `semver-caret-minor-ge` | upstream | discriminated | credited | limited | no_static_path | abstained | no | none |
| `semver-caret-zero-minor` | upstream | partially_discriminated | gap | limited | no_static_path | abstained | no | none |
| `semver-digit-upper-first-run` | upstream | discriminated | credited | limited | infection_unknown | abstained | no | none |
| `semver-digit-upper` | upstream | partially_discriminated | gap | limited | infection_unknown | abstained | no | none |
| `semver-digits-ten` | upstream | partially_discriminated | gap | limited | no_static_path | abstained | no | none |
| `semver-greater-patch-guard-ge` | upstream | discriminated | credited | limited | no_static_path | abstained | no | none |
| `semver-greater-patch` | upstream | discriminated | credited | limited | no_static_path | abstained | no | none |
| `semver-leading-zero` | upstream | discriminated | credited | limited | no_static_path | abstained | no | none |
| `semver-less-pre` | upstream | discriminated | credited | limited | infection_unknown, propagation_unknown | abstained | no | none |
| `semver-max-comparators` | upstream | partially_discriminated | gap | limited | no_static_path | abstained | no | none |
| `semver-op-ge-reads-greater` | upstream | discriminated | credited | limited | static_unknown | abstained | no | none |
| `semver-op-greater-eq` | upstream | discriminated | credited | limited | static_unknown | abstained | yes | none |
| `semver-req-separator` | upstream | discriminated | credited | limited | no_static_path | abstained | no | none |
| `semver-tilde-pre-ge` | upstream | discriminated | credited | limited | infection_unknown, propagation_unknown | abstained | no | none |
| `serde-derive-rename-field-upper` | upstream | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `serde-derive-rename-variant-lower` | upstream | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `serde-format-u8-hundreds` | upstream | discriminated | credited | limited | infection_unknown | abstained | yes | none |
| `serde-format-u8-tens` | upstream | discriminated | credited | limited | infection_unknown | abstained | yes | none |
| `shop-cap-literal-only-expected` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `shop-cart-add-other-collection-observed` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `shop-discount-path-dependent-test` | authored | discriminated | credited | limited | weakly_exposed | abstained | yes | none |
| `shop-gate-let-bound-input` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `shop-item-cents-trait-method` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `shop-item-total-associated-vs-free` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `shop-quote-total-result-field` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `shop-rebate-same-name-other-crate` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `shop-score-imported-across-crates` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `shop-tier-gold-arm-unreached` | authored | not_discriminated | gap | limited | infection_unknown | abstained | yes | none |
| `short-circuit-band-table` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `sibling-pin-insufficient-matches-arm` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0114-direct-unit-test` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `spec0114-helper-chain-public-api` | authored | discriminated | credited | limited | infection_unknown, propagation_unknown | abstained | no | none |
| `spec0114-test-calls-owner-macro` | authored | discriminated | credited | limited | no_static_path | abstained | no | none |
| `spec0115-integration-outer-inner` | authored | discriminated | credited | limited | propagation_unknown | abstained | yes | none |
| `spec0115-no-test-reaches-owner` | authored | not_discriminated | gap | limited | no_static_path | abstained | no | none |
| `spec0117-external-std-macro` | authored | discriminated | credited | limited | propagation_unknown | abstained | yes | none |
| `spec0117-lexical-path-beside-macro` | authored | discriminated | credited | limited | propagation_unknown | abstained | yes | none |
| `spec0117-macro-names-owner` | authored | discriminated | credited | limited | no_static_path | abstained | no | none |
| `spec0117-macro-without-owner-mention` | authored | discriminated | credited | limited | no_static_path | abstained | no | none |
| `spec0117-proximity-macro-only` | authored | discriminated | credited | limited | weakly_exposed | abstained | no | none |
| `spec0117-proximity-macro-plus-direct` | authored | discriminated | credited | limited | weakly_exposed | abstained | yes | none |
| `spec0118-semver-shaped-greater` | authored | discriminated | credited | limited | no_static_path | abstained | no | none |
| `spec0118-unit-helper-chain` | authored | discriminated | credited | limited | weakly_exposed | abstained | yes | none |
| `spec0120-builtin-assert-eq` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `spec0120-custom-assert-macro` | authored | discriminated | credited | limited | reachable_unrevealed | abstained | no | none |
| `spec0125-broad-error-control` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0224-parse-exact-arm-unobserved` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0225-wv-assert-ne-literal` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0225-wv-expected-binding` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `spec0225-wv-expected-helper` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `spec0225-wv-expected-reads-result-field` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `spec0225-wv-expected-via-field-binding` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0225-wv-field-type-manual-eq` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0225-wv-functional-update-default` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `spec0225-wv-let-binding-literal` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `spec0225-wv-let-mut-overwrite` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0225-wv-literal-derived` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `spec0225-wv-manual-eq-ignores-field` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0225-wv-nested-derived-eq` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `spec0225-wv-nested-manual-eq` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0225-wv-non-owner-call` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0225-wv-ok-wrapped-literal` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `spec0225-wv-sibling-field-read` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0225-wv-wrapper-manual-eq` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0226-bump-guarded-return` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0226-gate-split-boundary-agreement` | authored | partially_discriminated | gap | credited | exposed, propagation_unknown | false_exposed | no | none |
| `spec0226-journal-push-seeded-contains` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0226-ledger-push-exact-history` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `spec0226-parse-x-map-err-into` | authored | discriminated | credited | limited | weakly_exposed | abstained | no | none |
| `spec0227-check-guard-edge-is-err-only` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0227-check-guard-far-is-err` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0227-check-guard-same-test-flip` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `spec0227-check-guard-split-tests` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0227-check-ok-value-is-ok` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0227-check-variant-bare-unwrap-err` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `spec0227-check-variant-is-err` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0227-check-variant-should-panic` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0227-total-question-mark-earlier-err` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0227-total-question-mark-is-err` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `spec0227-total-question-mark-ok-input` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0228-collection-getter-read` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `spec0228-collection-misnamed-getter` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0228-deref-mut-param-write` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `spec0228-field-collection-push` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `spec0228-field-write-direct-read` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `spec0228-field-write-enum-other-field` | authored | not_discriminated | gap | limited | static_unknown | abstained | no | none |
| `spec0228-field-write-enum-variant` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `spec0228-field-write-getter-read` | authored | discriminated | credited | limited | static_unknown | abstained | no | none |
| `spec0228-field-write-misnamed-getter` | authored | not_discriminated | gap | limited | static_unknown | abstained | no | none |
| `spec0228-field-write-no-read` | authored | not_discriminated | gap | limited | static_unknown | abstained | no | none |
| `spec0228-field-write-reset-between` | authored | not_discriminated | gap | limited | static_unknown | abstained | no | none |
| `spec0228-field-write-sibling-read` | authored | not_discriminated | gap | limited | static_unknown | abstained | no | none |
| `spec0228-keyed-getter-other-field` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0228-keyed-getter-other-key` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0228-keyed-getter-refused-call` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0228-keyed-getter-rewritten-before-read` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0228-keyed-getter-same-key` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `spec0228-pushed-literal-len-projection` | authored | not_discriminated | gap | gap | exposed, weakly_exposed | ideal | yes | none |
| `spec0228-pushed-literal-whole-equality` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `spec0229-expected-side-variant` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0229-guarded-arm` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0229-let-made-input` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0229-range-widened-inside-input` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0229-range-widened-moved-input` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `spec0229-selected-none-arm` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `spec0229-unselected-none-arm` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0229-unselected-range-arm` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0229-unselected-wildcard-arm` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0231-bool-compared-inequality` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0231-bool-method-inequality` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0231-ensure-disjunction-not-exact` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0231-matches-nested-wildcard` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0231-matches-wildcard-conjunct` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0231-oracle-any-exact-string-line` | authored | discriminated | credited | limited | propagation_unknown | abstained | no | none |
| `spec0231-oracle-any-field-eq-literal` | authored | discriminated | credited | gap | exposed, weakly_exposed | false_actionable | no | none |
| `spec0231-oracle-any-threshold-not-pinned` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0231-oracle-assert-eq-err-bound-value` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `spec0231-oracle-assert-ne-err-variant` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0231-oracle-assert-ne-ok-value` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0231-oracle-assert-ne-scalar` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0231-oracle-assert-ne-struct-literal` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `spec0231-oracle-ensure-matches-ok-wildcard` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0231-oracle-ensure-not-equal` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0231-oracle-err-at-variant-binding` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `spec0231-oracle-err-at-wildcard-binding` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0231-oracle-err-ref-binding-guard` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0231-oracle-err-underscore-binding` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0231-oracle-events-not-empty` | authored | partially_discriminated | gap | limited | propagation_unknown | abstained | no | none |
| `spec0231-oracle-events-sent-contains` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0231-oracle-events-sent-len-eq` | authored | partially_discriminated | gap | credited | exposed | false_exposed | no | none |
| `spec0231-oracle-helper-assert-json-eq` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `spec0231-oracle-helper-assert-not-equal` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0231-oracle-is-ok-or-is-err` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0231-oracle-is-some-and-threshold` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0231-oracle-matches-err-binding-guard` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0231-oracle-matches-err-wildcard` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0231-oracle-matches-ok-literal` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `spec0231-oracle-matches-ok-wildcard` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0231-oracle-negated-matches-err-variant` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `spec0231-oracle-observer-substring-present` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0231-oracle-some-at-literal` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `spec0231-oracle-some-or-none-pattern` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0231-oracle-some-range-pattern` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0231-oracle-some-ref-binding` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0231-oracle-std-path-assert-eq` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `spec0231-pinning-chain-relation-kind` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0236-seam-boundary-direct-assert` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0236-seam-boundary-unasserted-caller` | authored | not_discriminated | gap | limited | propagation_unknown | abstained | no | none |
| `spec0236-seam-boundary-via-display-impl` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0236-seam-private-tier-integration-chain` | authored | partially_discriminated | gap | limited | no_static_path | abstained | no | none |
| `spec0236-seam-private-tier-macro-witness` | authored | partially_discriminated | gap | limited | no_static_path | abstained | no | none |
| `spec0236-seam-private-tier-mock-test-name` | authored | partially_discriminated | gap | limited | no_static_path | abstained | no | none |
| `spec0236-seam-private-tier-snapshot-test-name` | authored | partially_discriminated | gap | limited | no_static_path | abstained | no | none |
| `spec0236-seam-private-tier-src-test-chain` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `spec0236-seam-public-tier-no-test` | authored | not_discriminated | gap | limited | no_static_path | abstained | no | none |
| `spec0236-seam-static-closure-no-owner` | authored | partially_discriminated | gap | limited | infection_unknown, static_unknown | abstained | no | none |
| `spec0263-guard-entered-literal` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `spec0263-helper-calls-owner-back` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `spec0263-let-bound-input` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `spec0263-macro-calls-owner-back` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `spec0263-named-bound-guard` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `spec0263-one-call-enters-guard` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `spec0263-operator-impl-enters-guard` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `spec0263-recursive-owner-enters-guard` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `spec0263-renamed-import-enters-guard` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `spot-cap-generic-via-wrapper` | authored | discriminated | credited | limited | propagation_unknown | abstained | no | none |
| `spot-code-digit-helper-ok-or` | authored | discriminated | credited | credited | exposed, infection_unknown | ideal | yes | none |
| `spot-code-length-guard-query` | authored | discriminated | credited | limited | infection_unknown | abstained | yes | none |
| `spot-initials-computed-local-boundary` | authored | discriminated | credited | limited | infection_unknown | abstained | yes | none |
| `spot-window-clone-field-whole-equality` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `strsim-jaro-winkler-threshold-shift` | upstream | partially_discriminated | gap | limited | infection_unknown | abstained | yes | none |
| `strsim-sorensen-dice-equal` | upstream | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `tokens-add-fee-integration-api` | authored | discriminated | credited | limited | weakly_exposed | abstained | yes | none |
| `tokens-base-six-hop-chain` | authored | discriminated | credited | gap | weakly_exposed | false_actionable | no | none |
| `tokens-byte-at-unsafe-fn` | authored | discriminated | credited | credited | exposed, static_unknown | ideal | yes | none |
| `tokens-ext-start-map-or-binding` | authored | partially_discriminated | gap | limited | infection_unknown | abstained | yes | none |
| `tokens-fits-binding-predicate` | authored | discriminated | credited | limited | infection_unknown | abstained | yes | none |
| `tokens-inner-bonus-test-macro-call` | authored | discriminated | credited | limited | weakly_exposed | abstained | yes | none |
| `tokens-inner-rate-macro-reach` | authored | discriminated | credited | limited | weakly_exposed | abstained | yes | none |
| `tokens-long-flag-strip-prefix` | authored | discriminated | credited | credited | exposed, infection_unknown | ideal | yes | none |
| `tokens-normalize-helper-chain` | authored | partially_discriminated | gap | limited | propagation_unknown | abstained | no | none |
| `tokens-recursive-label-arm` | authored | discriminated | credited | limited | propagation_unknown | abstained | yes | none |
| `tokens-scanner-state-arm` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `tokens-word-start-helper` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `trap-area-sibling-not-called` | authored | not_discriminated | gap | limited | no_static_path | abstained | yes | none |
| `trap-bonus-stale-clone` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `trap-boost-detached-thread` | authored | not_discriminated | gap | gap | reachable_unrevealed | ideal | no | none |
| `trap-cap-min-clamp` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `trap-ceiling-wide-min-control` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `trap-charge-unbuilt-integration-test` | authored | not_discriminated | gap | limited | no_static_path | abstained | yes | none |
| `trap-const-fn-self-expected` | authored | not_discriminated | gap | credited | exposed | false_exposed | no | none |
| `trap-dampen-times-zero` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `trap-delay-borrowed-same-name` | authored | not_discriminated | gap | limited | static_unknown | abstained | yes | none |
| `trap-fee-name-only` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `trap-fine-declared-test-file-control` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `trap-glow-debug-nonempty` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `trap-halve-empty-table` | authored | not_discriminated | gap | gap | reachable_unrevealed | ideal | no | none |
| `trap-hoist-scoped-thread-control` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `trap-iter-boundary-missing` | authored | partially_discriminated | gap | gap | exposed, weakly_exposed | ideal | no | none |
| `trap-iter-empty-slice` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `trap-level-catch-unwind` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `trap-levy-registered-integration-control` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `trap-lift-joined-thread-control` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `trap-limit-nested-test` | authored | not_discriminated | gap | gap | reachable_unrevealed | ideal | no | none |
| `trap-muffle-times-one-control` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `trap-pause-asserted-arm-control` | authored | discriminated | credited | credited | exposed | ideal | no | none |
| `trap-pile-first-element-control` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `trap-quota-sibling-arm` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `trap-ramp-same-call-both-sides` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `trap-rate-ignored-test` | authored | not_discriminated | gap | gap | reachable_unrevealed | ideal | no | none |
| `trap-retries-unsigned-nonnegative` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `trap-round-other-module` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `trap-row-at-expected-panic-control` | authored | discriminated | credited | limited | static_unknown | abstained | yes | none |
| `trap-row-of-unrelated-panic` | authored | not_discriminated | gap | limited | static_unknown | abstained | yes | none |
| `trap-scale-message-only` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `trap-score-tautology` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `trap-shine-debug-exact-control` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `trap-short-circuit-skipped-operand` | authored | partially_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `trap-slot-of-reached-panic-control` | authored | discriminated | credited | limited | static_unknown | abstained | yes | none |
| `trap-snap-imported-owner-control` | authored | discriminated | credited | credited | exposed | ideal | yes | none |
| `trap-spare-undeclared-test-file` | authored | not_discriminated | gap | limited | no_static_path | abstained | yes | none |
| `trap-stack-container-length` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `trap-steps-early-return` | authored | not_discriminated | gap | gap | reachable_unrevealed | ideal | no | none |
| `trap-tally-reached-assert-control` | authored | discriminated | credited | limited | static_unknown | abstained | yes | none |
| `trap-tax-reference-helper` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | yes | none |
| `trap-tier-dropped-conjunct` | authored | not_discriminated | gap | limited | infection_unknown | abstained | yes | none |
| `trap-total-local-binding` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |
| `trap-widen-asserts-input` | authored | not_discriminated | gap | gap | weakly_exposed | ideal | no | none |

## Spec example coverage

Numbered acceptance examples of in-scope specs that at least one case cites in `spec_examples` (fixtures/rust-verdict-corpus/spec-coverage.toml). Covered 131/131 (1.0000); floor 131. Waived 15 and accounted (covered plus waived) 146/146. In-scope specs 17, out-of-scope specs 68.

Unmeasured in-scope specs (prose acceptance examples, not counted): RIPR-SPEC-0001, RIPR-SPEC-0002, RIPR-SPEC-0004, RIPR-SPEC-0005, RIPR-SPEC-0035, RIPR-SPEC-0040, RIPR-SPEC-0048, RIPR-SPEC-0093, RIPR-SPEC-0094, RIPR-SPEC-0096, RIPR-SPEC-0106, RIPR-SPEC-0107, RIPR-SPEC-0108, RIPR-SPEC-0150, RIPR-SPEC-0153, RIPR-SPEC-0154, RIPR-SPEC-0157, RIPR-SPEC-0158, RIPR-SPEC-0159, RIPR-SPEC-0163, RIPR-SPEC-0164, RIPR-SPEC-0165, RIPR-SPEC-0168, RIPR-SPEC-0169, RIPR-SPEC-0171, RIPR-SPEC-0172, RIPR-SPEC-0175, RIPR-SPEC-0186, RIPR-SPEC-0197, RIPR-SPEC-0219.

| Spec | In-scope examples | Covered | Waived | Uncovered |
| --- | --- | --- | --- | --- |
| RIPR-SPEC-0114 | 6 | 5 | 1 | none |
| RIPR-SPEC-0115 | 7 | 3 | 4 | none |
| RIPR-SPEC-0117 | 7 | 7 | 0 | none |
| RIPR-SPEC-0118 | 4 | 4 | 0 | none |
| RIPR-SPEC-0119 | 2 | 2 | 0 | none |
| RIPR-SPEC-0120 | 3 | 3 | 0 | none |
| RIPR-SPEC-0125 | 3 | 1 | 2 | none |
| RIPR-SPEC-0224 | 2 | 1 | 1 | none |
| RIPR-SPEC-0225 | 16 | 16 | 0 | none |
| RIPR-SPEC-0226 | 4 | 4 | 0 | none |
| RIPR-SPEC-0227 | 13 | 13 | 0 | none |
| RIPR-SPEC-0228 | 20 | 18 | 2 | none |
| RIPR-SPEC-0229 | 10 | 9 | 1 | none |
| RIPR-SPEC-0231 | 33 | 33 | 0 | none |
| RIPR-SPEC-0233 | 4 | 0 | 4 | none |
| RIPR-SPEC-0263 | 6 | 6 | 0 | none |
| RIPR-SPEC-0265 | 6 | 6 | 0 | none |

## Oracle accuracy

The related-test oracle kind and strength ripr reports on the anchored findings, against the value a spec adopts (`expected_oracle`). Scored apart from the verdict: a wrong oracle that leaves the verdict unchanged still reads `ideal` above. Labeled 44. Accuracy 44/44 (1.0000); overclaim 0/44 (0.0000). Match floor 44; overclaim ceiling 0 (fixtures/rust-verdict-corpus/spec-coverage.toml).

| Outcome | Cases |
| --- | --- |
| match | 44 |
| overclaim | 0 |
| underclaim | 0 |
| kind_mismatch | 0 |
| unobserved | 0 |

| Case | Spec | Expected | Observed | All observed | Outcome |
| --- | --- | --- | --- | --- | --- |
| `accounts-parse-too-long-variant` | RIPR-SPEC-0227#11 | broad_error / weak | broad_error / weak | too_many_digits_are_refused: broad_error / weak | match |
| `spec0227-check-guard-far-is-err` | RIPR-SPEC-0227#3 | broad_error / weak | broad_error / weak | far_is_refused: broad_error / weak | match |
| `spec0227-check-guard-split-tests` | RIPR-SPEC-0227#4 | broad_error / weak | broad_error / weak | split_refuses_far: broad_error / weak | match |
| `spec0227-check-variant-bare-unwrap-err` | RIPR-SPEC-0227#7 | broad_error / weak | broad_error / weak | unwrap_err_on_far_input: broad_error / weak | match |
| `spec0227-check-variant-is-err` | RIPR-SPEC-0227#5 | broad_error / weak | broad_error / weak | is_err_on_far_input: broad_error / weak | match |
| `spec0227-total-question-mark-earlier-err` | RIPR-SPEC-0227#13 | broad_error / weak | broad_error / weak | letter_is_refused: broad_error / weak | match |
| `spec0231-bool-compared-inequality` | RIPR-SPEC-0231#31 | relational_check / weak | relational_check / weak | score_is_nonzero: relational_check / weak | match |
| `spec0231-bool-method-inequality` | RIPR-SPEC-0231#32 | relational_check / weak | relational_check / weak | score_is_nonzero: relational_check / weak | match |
| `spec0231-ensure-disjunction-not-exact` | RIPR-SPEC-0231#13 | relational_check / weak | relational_check / weak | score_is_nonzero_or_pinned: relational_check / weak | match |
| `spec0231-matches-nested-wildcard` | RIPR-SPEC-0231#7 | relational_check / weak | relational_check / weak | check_accepts_five: relational_check / weak | match |
| `spec0231-matches-wildcard-conjunct` | RIPR-SPEC-0231#7 | relational_check / weak | relational_check / weak | check_accepts_five: relational_check / weak | match |
| `spec0231-oracle-any-exact-string-line` | RIPR-SPEC-0231#27 | exact_value / strong | exact_value / strong | audit_logs_the_count: exact_value / strong | match |
| `spec0231-oracle-any-field-eq-literal` | RIPR-SPEC-0231#28 | exact_value / strong | exact_value / strong | rows_carry_the_sum: exact_value / strong | match |
| `spec0231-oracle-any-threshold-not-pinned` | RIPR-SPEC-0231#29 | relational_check / weak | relational_check / weak | scale_grows_something: relational_check / weak | match |
| `spec0231-oracle-assert-eq-err-bound-value` | RIPR-SPEC-0231#5 | exact_error_variant / strong | exact_error_variant / strong | check_twenty_is_big: exact_error_variant / strong | match |
| `spec0231-oracle-assert-ne-err-variant` | RIPR-SPEC-0231#20 | relational_check / weak | relational_check / weak | check_twenty_is_not_bad: relational_check / weak | match |
| `spec0231-oracle-assert-ne-ok-value` | RIPR-SPEC-0231#2 | relational_check / weak | relational_check / weak | check_twenty_is_not_ok: relational_check / weak | match |
| `spec0231-oracle-assert-ne-scalar` | RIPR-SPEC-0231#1 | relational_check / weak | relational_check / weak | score_is_not_zero: relational_check / weak | match |
| `spec0231-oracle-assert-ne-struct-literal` | RIPR-SPEC-0231#14 | whole_object_equality / weak | whole_object_equality / weak | build_is_not_nine_retries: whole_object_equality / weak | match |
| `spec0231-oracle-ensure-matches-ok-wildcard` | RIPR-SPEC-0231#18 | smoke_only / smoke | smoke_only / smoke | check_accepts_five: smoke_only / smoke | match |
| `spec0231-oracle-ensure-not-equal` | RIPR-SPEC-0231#13 | relational_check / weak | relational_check / weak | score_is_not_zero: relational_check / weak | match |
| `spec0231-oracle-err-at-variant-binding` | RIPR-SPEC-0231#22 | exact_error_variant / strong | exact_error_variant / strong | check_twenty_is_bad: exact_error_variant / strong | match |
| `spec0231-oracle-err-at-wildcard-binding` | RIPR-SPEC-0231#21 | broad_error / weak | broad_error / weak | check_twenty_errs: broad_error / weak | match |
| `spec0231-oracle-err-ref-binding-guard` | RIPR-SPEC-0231#21 | broad_error / weak | broad_error / weak | check_twenty_has_message: broad_error / weak | match |
| `spec0231-oracle-err-underscore-binding` | RIPR-SPEC-0231#17 | broad_error / weak | broad_error / weak | check_rejects_twenty: broad_error / weak | match |
| `spec0231-oracle-events-not-empty` | RIPR-SPEC-0231#16 | mock_expectation / medium | mock_expectation / medium | emit_records_an_event: mock_expectation / medium | match |
| `spec0231-oracle-events-sent-contains` | RIPR-SPEC-0231#10 | mock_expectation / medium | mock_expectation / medium | send_records_the_id: mock_expectation / medium | match |
| `spec0231-oracle-events-sent-len-eq` | RIPR-SPEC-0231#9 | exact_value / strong | exact_value / strong | send_records_one_event: exact_value / strong | match |
| `spec0231-oracle-helper-assert-json-eq` | RIPR-SPEC-0231#12 | exact_value / strong | exact_value / strong | score_renders_as_json: exact_value / strong; score_renders_as_json: relational_check / weak | match |
| `spec0231-oracle-helper-assert-not-equal` | RIPR-SPEC-0231#11 | relational_check / weak | relational_check / weak | score_is_not_zero: relational_check / weak | match |
| `spec0231-oracle-is-ok-or-is-err` | RIPR-SPEC-0231#26 | relational_check / weak | relational_check / weak | check_five_is_a_result: relational_check / weak | match |
| `spec0231-oracle-is-some-and-threshold` | RIPR-SPEC-0231#15 | smoke_only / smoke | smoke_only / smoke | lookup_one_exceeds_one: smoke_only / smoke | match |
| `spec0231-oracle-matches-err-binding-guard` | RIPR-SPEC-0231#3 | broad_error / weak | broad_error / weak | check_twenty_has_an_error_message: broad_error / weak | match |
| `spec0231-oracle-matches-err-wildcard` | RIPR-SPEC-0231#4 | broad_error / weak | broad_error / weak | check_twenty_is_an_error: broad_error / weak | match |
| `spec0231-oracle-matches-ok-literal` | RIPR-SPEC-0231#6 | exact_value / strong | exact_value / strong | check_five_is_ok_five: exact_value / strong | match |
| `spec0231-oracle-matches-ok-wildcard` | RIPR-SPEC-0231#7 | smoke_only / smoke | smoke_only / smoke | check_five_is_ok: smoke_only / smoke | match |
| `spec0231-oracle-negated-matches-err-variant` | RIPR-SPEC-0231#19 | relational_check / weak | relational_check / weak | check_twenty_is_not_bad: relational_check / weak | match |
| `spec0231-oracle-observer-substring-present` | RIPR-SPEC-0231#8 | relational_check / weak | relational_check / weak | score_prints_one_digit: relational_check / weak | match |
| `spec0231-oracle-some-at-literal` | RIPR-SPEC-0231#23 | exact_value / strong | exact_value / strong | lookup_one_is_three: exact_value / strong | match |
| `spec0231-oracle-some-or-none-pattern` | RIPR-SPEC-0231#24 | relational_check / weak | relational_check / weak | lookup_one_is_an_option: relational_check / weak | match |
| `spec0231-oracle-some-range-pattern` | RIPR-SPEC-0231#25 | relational_check / weak | relational_check / weak | lookup_one_in_range: relational_check / weak | match |
| `spec0231-oracle-some-ref-binding` | RIPR-SPEC-0231#23 | smoke_only / smoke | smoke_only / smoke | lookup_one_is_some: smoke_only / smoke | match |
| `spec0231-oracle-std-path-assert-eq` | RIPR-SPEC-0231#30 | exact_value / strong | exact_value / strong | score_three_is_nine: exact_value / strong | match |
| `spec0231-pinning-chain-relation-kind` | RIPR-SPEC-0231#33 | exact_value / strong | exact_value / strong | score_pins_and_check_is_not_bad: exact_value / strong | match |

Non-claims:

- Rates describe this corpus only; they are not a population estimate for Rust code or for ripr in general.
- Authored cases were written to fill verdict, probe-family, test-shape and test-library oracle cells the upstream cases leave empty; their rates are reported separately under by_origin and are not real-world rates.
- Truth comes from the listed mutants, not from exhaustive mutation; a discriminated label means every listed mutant made the test command fail.
- The harness runs ripr on retained upstream excerpts and on whole authored crates; excerpt findings matched the full pinned checkout at labeling time and must be re-checked when a verdict changes.
- The harness does not run mutation testing, cargo test, or any network access.

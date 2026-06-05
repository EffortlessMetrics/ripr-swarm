# Handoff: TypeScript/Bun Preview Patch Proof

Date: 2026-06-05
Branch / PR: `release-typescript-bun-preview-patch-proof` / #988
Current work item: `release/typescript-bun-preview-patch-proof`

## Decision

The current TypeScript/Bun preview patch line is documented as a proof packet,
not a release. It records the calibrated Bun Blob / ArrayBuffer evidence,
follow-up cross-language graph rows, dogfood receipt status, validation
results, and non-claims needed before the next user-experience slice.

Authority remains:

```text
preview_advisory_only
repair_packet_ready = false
```

No source release, tag, publish, signing, marketplace, install-doc, support-tier
promotion, gate, badge, baseline, RIPR Zero, runtime proof, generated test, or
public repair packet is authorized by this packet.

## Evidence Summary

| Surface | Current proof |
| --- | --- |
| Bun Blob / ArrayBuffer calibration | `cargo xtask bun-ub-calibration` wrote `target/ripr/reports/bun-ub-calibration.{json,md}` with `Status: pass`, 7 cases, 7 passing cases, and 0 repair-packet-ready cases. |
| Complete Blob witness | `bun_blob_shared_and_resizable_present` observes `ts_discriminated` with no missing discriminators, no missing graph legs, and `repair_packet_ready=false`. |
| Missing Blob discriminators | Missing shared and/or resizable rows name the missing discriminator and use `test/js/web/fetch/blob.test.ts` only as advisory placement. |
| Mention-only rejection | `bun_blob_max_byte_length_mention_not_observer` remains `ts_mention_not_observer` with no suggested file and no repair packet. |
| Bridge unknown | `bun_blob_bridge_unknown_without_hint` remains `bridge_unknown` with missing `binding_or_ffi_edge` and no repair packet. |
| `copy_to_unshared` follow-up | `fixtures/cross-language-oracle-graph-corpus/corpus.json#bun_array_buffer_copy_to_unshared_configured_bridge_advisory` records a configured bridge plus TypeScript callsite and oracle samples for `src/jsc/array_buffer.rs:341`, but still has no public repair-packet fields. |
| MarkdownObject follow-up | `fixtures/cross-language-oracle-graph-corpus/corpus.json#bun_markdown_resizable_array_buffer_configured_bridge_advisory` records the configured `Bun.markdown` bridge to `src/runtime/api/MarkdownObject.rs:60`, but remains advisory and non-actionable. |
| FFI panic-boundary follow-up | `fixtures/cross-language-oracle-graph-corpus/corpus.json#bun_ffi_negative_offset_panic_boundary_limitation` and the dogfood receipt keep `public_reachable_panic_boundary_unrevealed` as a named limitation until the negative-offset panic oracle and safe external observer target are resolved. |
| Bun dogfood receipts | `target/ripr/reports/dogfood.md` includes 4 Bun UB cross-language witness receipts: one TS-discriminated witness, one missing discriminator, one mention-only rejection, and one FFI panic-boundary limitation; all have `repair_packet_ready=false`. |

## Calibrated States

`target/ripr/reports/bun-ub-calibration.md` currently reports:

| State bucket | Count |
| --- | ---: |
| Cases | 7 |
| Passing cases | 7 |
| Failing cases | 0 |
| TS discriminated cases | 1 |
| Missing resizable cases | 1 |
| Missing shared cases | 1 |
| Missing shared and resizable cases | 1 |
| Missing external oracle cases | 1 |
| Mention-not-observer cases | 1 |
| Bridge-unknown cases | 1 |
| Missing-discriminator cases | 3 |
| Public packet exclusions | 7 |
| Repair-packet-ready cases | 0 |

## Dogfood Status

The focused Bun dogfood receipt checks pass:

```text
cargo test -p xtask dogfood_bun_ub_cross_language -- --test-threads=1
```

The broad `cargo xtask dogfood` command currently exits non-zero while writing
`target/ripr/reports/dogfood.md` with `Status: warn`. This packet treats that
as a validation warning for the broad dogfood surface, not as a failure of the
focused Bun receipt corpus. The Bun-specific section in that report records:

| Case | Observed state | Operator action | Suggested file | Repair packet ready |
| --- | --- | --- | --- | --- |
| `bun_blob_31648_known_good` | `rust_ungripped_ts_discriminated` | `no_missing_bridge_discriminator` | `not_applicable` | no |
| `bun_blob_stripped_resizable` | `rust_ungripped_ts_missing_discriminator` | `suggest_resizable_array_buffer_blob_case` | `test/js/web/fetch/blob.test.ts` | no |
| `bun_blob_mention_only` | `ts_mention_not_observer` | `reject_token_mention` | `not_applicable` | no |
| `bun_ffi_negative_offset_panic_boundary` | `public_reachable_panic_boundary_unrevealed` | `keep_panic_boundary_limitation` | `not_applicable` | no |

## Validation Results

| Command | Result | Notes |
| --- | --- | --- |
| `cargo test -p xtask cross_language_oracle_graph -- --test-threads=1` | pass | 8 passed. |
| `cargo test -p xtask typescript_bun_ub_calibration -- --test-threads=1` | pass | 5 passed. |
| `cargo test -p xtask bun_ub_calibration -- --test-threads=1` | pass | 7 passed. |
| `cargo test -p xtask dogfood_bun_ub_cross_language -- --test-threads=1` | pass | 2 passed. |
| `cargo test -p ripr typescript_preview_card_projects_bun_cross_language_grip -- --test-threads=1` | pass | 1 passed. |
| `cargo xtask bun-ub-calibration` | pass | Wrote pass calibration reports under `target/ripr/reports/`. |
| `cargo xtask dogfood` | warn / non-zero | Wrote `target/ripr/reports/dogfood.md` with `Status: warn`; Bun receipt section is present and packet-ready count is 0. |
| `cargo xtask check-pr` | fail | Failed during `cargo test --workspace`: `reports::pr_evidence::tests::run_ripr_check_uses_fake_binary_success_output` timed out after 30 seconds. The same test passed focused with `cargo test -p xtask run_ripr_check_uses_fake_binary_success_output -- --test-threads=1`. |
| `git diff --check` | pass | No whitespace errors after the packet was added. |

## Non-Claims

- No stable TypeScript or JavaScript support claim.
- No full Bun binding graph.
- No generic cross-language support for every mixed-language repository.
- No runtime Bun, Jest, Vitest, `tsc`, `tsserver`, Miri, mutation, provider, or
  generated-test execution.
- No generated tests or source edits.
- No public repair packets from preview cross-language evidence.
- No verify command, receipt command, allowed edit surface, or must-not-change
  packet authority for cross-language preview rows.
- No default gates, badges, baselines, RIPR Zero, support-tier promotion,
  source release, publish, tag, signing, marketplace, or install-doc work.

## Next Work Item

`output/bun-ub-preview-summary`

Build the compact JSON and Markdown first-screen summary from existing
route-quality, calibration, and dogfood data. Do not add analyzer behavior,
bridge inference, runtime execution, public repair packets, gates, badges, or
support-tier promotion in that slice.

# ripr mutation calibration report

Status: advisory

This report joins static seam evidence to supplied cargo-mutants runtime data. Runtime outcome vocabulary in this report comes from that runtime data; static ripr reports continue to use audit vocabulary only.

## Provenance

ripr calibrate cargo-mutants: join matches by seam_id, span containment, or file/line; at least one of the repo-exposure / cargo-mutants artifacts does not carry a head SHA, so ensure both artifacts are from the same repository and adjacent commits.

## Summary

| Metric | Count |
| --- | ---: |
| static_seams_total | 8 |
| mutants_total | 7 |
| matched_total | 5 |
| ambiguous_file_line_total | 1 |
| ambiguous_span_overlap_total | 0 |
| unmatched_mutants_total | 1 |
| static_without_runtime_total | 1 |

## Static/runtime agreement

How to read this table: a **static gap** is a static seam whose grip class is outside the strong set `strongly_gripped` / `intentional` / `suppressed` (for example `ungripped` or `weakly_gripped`). A **runtime gap signal** is an imported cargo-mutants outcome the runtime failed to catch (`missed`, `survived`, `not_caught`, `uncaught`); a **runtime clean signal** is one it caught (`caught`, `timeout`).

- `static_gap_and_runtime_signal`: ripr flags a static gap (a seam outside the strongly_gripped / intentional / suppressed set) and a runtime mutant went uncaught at that seam: the two sources agree, which supports the static classification.
- `static_gap_without_runtime_signal`: ripr flags a static gap but this import produced no runtime gap signal (missed / survived / not_caught / uncaught) for it, so the bucket only says the static gap lacks runtime corroboration — not that it went unexercised: `contradicts_static_gap` rows caught the mutant (the runtime disagrees with the static gap), `no_runtime_data` rows matched only inconclusive or unobserved outcomes, and seams with no matched runtime record were not exercised by this import; see the Static gaps without runtime signals table.
- `runtime_signal_without_static_gap`: a runtime mutant went uncaught where ripr reports no static gap. Joined rows (`contradicts_static_clean`) genuinely disagree with a static-clean seam and are the first ones to investigate; unmatched rows (`runtime_only_signal`) had no static seam to contradict, so they are runtime-only evidence, not a disagreement; see the Runtime signals without static gaps table.
- `static_clean_and_runtime_clean`: ripr reports no static gap (the seam is strongly_gripped, intentional, or suppressed) and the runtime caught the mutant: the two sources agree.
- `runtime_inconclusive`: ambiguous joins (several seams on one line, or crossing spans) or unobserved runtime outcomes: counted separately as evidence neither for nor against the static verdict until a seam_id or unambiguous location is available.

| Agreement bucket | Count |
| --- | ---: |
| static_gap_and_runtime_signal | 2 |
| static_gap_without_runtime_signal | 2 |
| runtime_signal_without_static_gap | 2 |
| static_clean_and_runtime_clean | 1 |
| runtime_inconclusive | 1 |

Precision notes:

- runtime gap signals are imported runtime labels such as missed, survived, not_caught, or uncaught
- runtime clean signals are imported runtime labels such as caught or timeout
- static_gap_without_runtime_signal includes static gap seams with no matched runtime gap signal in this import
- runtime records with a complete span join by span_containment to the unique innermost static seam whose span contains the mutated range; with no containing span they fall back to file and line over seams without a span, and records without a complete span join by file and line
- ambiguous runtime gap signals (ambiguous_file_line: the file/line fallback found several seams on the line; ambiguous_span_overlap: equal or crossing innermost seam spans) are counted as runtime_inconclusive until a seam_id or unambiguous location is available
- agreement buckets mix counting units: `static_gap_and_runtime_signal`, `static_gap_without_runtime_signal`, `static_clean_and_runtime_clean`, and `runtime_inconclusive` count static seams (with ambiguous records also counted in `runtime_inconclusive`), while `runtime_signal_without_static_gap` counts runtime gap-signal records, so the bucket column need not sum to `matched_total` or `mutants_total`
- `runtime_signal_without_static_gap` includes 1 unmatched gap-signal record also counted in `unmatched_mutants_total`

### Runtime signals without static gaps

| Runtime mutant | Location | Runtime outcome | Static class | Confidence label | Reason |
| --- | --- | --- | --- | --- | --- |
| `m-v3-snapshot-field-discriminator` | seam:cal-v3-snapshot-field-discriminator | missed | `strongly_gripped` | `contradicts_static_clean` | runtime gap signal joined to a static-clean seam |
| `m-v3-runtime-only-signal` | src/runtime_only_v3.rs:99 | missed | `unmatched` | `runtime_only_signal` | runtime gap signal did not join to a static seam |

### Static gaps without runtime signals

| Seam | Class | Location | Confidence label | Reason |
| --- | --- | --- | --- | --- |
| `cal-v3-builder-override-outcome` | `weakly_gripped` | src/builder.rs:33 | `contradicts_static_gap` | static gap seam matched runtime data without a runtime gap signal |
| `cal-v3-cross-file-constant-boundary` | `weakly_gripped` | src/cross_file.rs:41 | `no_runtime_data` | static gap seam has no matched runtime record in this import |

## Runtime Outcome Counts

| Runtime outcome | Count |
| --- | ---: |
| caught | 2 |
| missed | 5 |

## Matched Mutants

| Seam | Class | Oracle | Mutation operator | Runtime outcome | Join | Confidence label |
| --- | --- | --- | --- | --- | --- | --- |
| `cal-v3-builder-override-outcome` | `weakly_gripped` | `exact_value`/`weak` | replace builder override threshold | caught | `seam_id` | `contradicts_static_gap` |
| `cal-v3-custom-helper-outcome` | `weakly_gripped` | `custom_assertion_helper`/`medium` | replace helper-covered threshold comparison | missed | `seam_id` | `supports_static_gap` |
| `cal-v3-mock-expectation-mismatch` | `weakly_gripped` | `mock_expectation`/`medium` | replace mock expectation amount argument | missed | `seam_id` | `supports_static_gap` |
| `cal-v3-snapshot-field-discriminator` | `strongly_gripped` | `snapshot_field`/`strong` | replace snapshot discount_code field | missed | `seam_id` | `contradicts_static_clean` |
| `cal-v3-table-boundary-outcome` | `strongly_gripped` | `exact_value`/`strong` | replace table threshold comparison | caught | `seam_id` | `supports_static_clean` |

## Ambiguous File/Line Matches

| Runtime mutant | Location | Runtime outcome | Confidence label | Candidate seams |
| --- | --- | --- | --- | --- |
| `m-v3-ambiguous-builder` | src/ambiguous_builder.rs:72 | missed | `ambiguous_runtime_join` | `cal-v3-ambiguous-builder-a`, `cal-v3-ambiguous-builder-b` |

## Ambiguous Span Overlaps

No runtime mutant span was contained by equal or crossing innermost seam spans.

## Unmatched Runtime Mutants

| Location | Mutation operator | Runtime outcome | Reason | Test command |
| --- | --- | --- | --- | --- |
| src/runtime_only_v3.rs:99 | replace runtime-only observed value | missed | `no_seam_on_line` | cargo test runtime_only_v3_observer |

## Static Seams Without Runtime Data

Sample only; see JSON `static_without_runtime_total` for the full count.

| Seam | Kind | Class | Location | Confidence label |
| --- | --- | --- | --- | --- |
| `cal-v3-cross-file-constant-boundary` | `predicate_boundary` | `weakly_gripped` | src/cross_file.rs:41 | `no_runtime_data` |

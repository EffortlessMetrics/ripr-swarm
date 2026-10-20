# ripr mutation calibration report

Status: advisory

This report joins static seam evidence to supplied cargo-mutants runtime data. Runtime outcome vocabulary in this report comes from that runtime data; static ripr reports continue to use audit vocabulary only.

## Provenance

Neither artifact carries a repository head SHA, so ensure both artifacts are from the same repository and adjacent commits. The join itself matches by seam_id, span containment, or file/line in every case.

## Summary

| Metric | Count |
| --- | ---: |
| static_seams_total | 6 |
| mutants_total | 6 |
| matched_total | 4 |
| ambiguous_file_line_total | 1 |
| ambiguous_span_overlap_total | 0 |
| unmatched_mutants_total | 1 |
| static_without_runtime_total | 0 |

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
| static_gap_without_runtime_signal | 1 |
| runtime_signal_without_static_gap | 1 |
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
| `m-v2-runtime-only-signal` | src/runtime_only.rs:99 | missed | `unmatched` | `runtime_only_signal` | runtime gap signal did not join to a static seam |

### Static gaps without runtime signals

| Seam | Class | Location | Confidence label | Reason |
| --- | --- | --- | --- | --- |
| `cal-v2-weak-snapshot-oracle` | `weakly_gripped` | src/summary.rs:36 | `contradicts_static_gap` | static gap seam matched runtime data without a runtime gap signal |

## Runtime Outcome Counts

| Runtime outcome | Count |
| --- | ---: |
| caught | 2 |
| missed | 4 |

## Matched Mutants

| Seam | Class | Oracle | Mutation operator | Runtime outcome | Join | Confidence label |
| --- | --- | --- | --- | --- | --- | --- |
| `cal-v2-mock-expectation` | `strongly_gripped` | `mock_expectation`/`strong` | delete gateway expectation call | caught | `seam_id` | `supports_static_clean` |
| `cal-v2-opaque-dispatch` | `reachable_unrevealed` | `opaque_dispatch`/`none` | replace dynamic dispatch target | missed | `seam_id` | `supports_static_gap` |
| `cal-v2-side-effect-observer` | `weakly_gripped` | `side_effect_observer`/`medium` | delete ledger side-effect call | missed | `seam_id` | `supports_static_gap` |
| `cal-v2-weak-snapshot-oracle` | `weakly_gripped` | `snapshot`/`weak` | replace serialized discount_reason field | caught | `seam_id` | `contradicts_static_gap` |

## Ambiguous File/Line Matches

| Runtime mutant | Location | Runtime outcome | Confidence label | Candidate seams |
| --- | --- | --- | --- | --- |
| `m-v2-ambiguous-opaque-dispatch` | src/routing.rs:58 | missed | `ambiguous_runtime_join` | `cal-v2-opaque-ambiguous-a`, `cal-v2-opaque-ambiguous-b` |

## Ambiguous Span Overlaps

No runtime mutant span was contained by equal or crossing innermost seam spans.

## Unmatched Runtime Mutants

| Location | Mutation operator | Runtime outcome | Reason | Test command |
| --- | --- | --- | --- | --- |
| src/runtime_only.rs:99 | replace observer value | missed | `no_seam_on_line` | cargo test runtime_only_observer |

## Static Seams Without Runtime Data

Every static seam matched at least one runtime mutant in the imported data.

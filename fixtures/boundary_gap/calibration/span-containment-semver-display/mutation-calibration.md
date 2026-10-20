# ripr mutation calibration report

Status: advisory

This report joins static seam evidence to supplied cargo-mutants runtime data. Runtime outcome vocabulary in this report comes from that runtime data; static ripr reports continue to use audit vocabulary only.

## Provenance

Neither artifact carries a repository head SHA, so ensure both artifacts are from the same repository and adjacent commits. The join itself matches by seam_id, span containment, or file/line in every case.

## Summary

| Metric | Count |
| --- | ---: |
| static_seams_total | 1 |
| mutants_total | 2 |
| matched_total | 0 |
| ambiguous_file_line_total | 0 |
| ambiguous_span_overlap_total | 0 |
| unmatched_mutants_total | 2 |
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
| static_gap_and_runtime_signal | 0 |
| static_gap_without_runtime_signal | 1 |
| runtime_signal_without_static_gap | 0 |
| static_clean_and_runtime_clean | 0 |
| runtime_inconclusive | 0 |

Precision notes:

- runtime gap signals are imported runtime labels such as missed, survived, not_caught, or uncaught
- runtime clean signals are imported runtime labels such as caught or timeout
- static_gap_without_runtime_signal includes static gap seams with no matched runtime gap signal in this import
- runtime records with a complete span join by span_containment to the unique innermost static seam whose span contains the mutated range; with no containing span they fall back to file and line over seams without a span, and records without a complete span join by file and line
- ambiguous runtime gap signals (ambiguous_file_line: the file/line fallback found several seams on the line; ambiguous_span_overlap: equal or crossing innermost seam spans) are counted as runtime_inconclusive until a seam_id or unambiguous location is available
- agreement buckets mix counting units: `static_gap_and_runtime_signal`, `static_gap_without_runtime_signal`, `static_clean_and_runtime_clean`, and `runtime_inconclusive` count static seams (with ambiguous records also counted in `runtime_inconclusive`), while `runtime_signal_without_static_gap` counts runtime gap-signal records, so the bucket column need not sum to `matched_total` or `mutants_total`

### Runtime signals without static gaps

No imported runtime gap signals lacked a matching static gap.

### Static gaps without runtime signals

| Seam | Class | Location | Confidence label | Reason |
| --- | --- | --- | --- | --- |
| `8828427428828b64` | `ungripped` | src/display.rs:20 | `no_runtime_data` | static gap seam has no matched runtime record in this import |

## Runtime Outcome Counts

| Runtime outcome | Count |
| --- | ---: |
| caught | 2 |

## Matched Mutants

No runtime mutants matched static seams.

## Ambiguous File/Line Matches

No line-only runtime mutants matched multiple static seams.

## Ambiguous Span Overlaps

No runtime mutant span was contained by equal or crossing innermost seam spans.

## Unmatched Runtime Mutants

| Location | Mutation operator | Runtime outcome | Reason | Test command |
| --- | --- | --- | --- | --- |
| src/display.rs:20 | BinaryOperator | caught | `no_containing_seam` | unknown |
| src/display.rs:20 | BinaryOperator | caught | `no_containing_seam` | unknown |

## Static Seams Without Runtime Data

Sample only; see JSON `static_without_runtime_total` for the full count.

| Seam | Kind | Class | Location | Confidence label |
| --- | --- | --- | --- | --- |
| `8828427428828b64` | `call_presence` | `ungripped` | src/display.rs:20 | `no_runtime_data` |

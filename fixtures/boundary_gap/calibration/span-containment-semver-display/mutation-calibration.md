# ripr mutation calibration report

Status: advisory

This report joins static seam evidence to supplied cargo-mutants runtime data. Runtime outcome vocabulary in this report comes from that runtime data; static ripr reports continue to use audit vocabulary only.

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
- ambiguous runtime gap signals (ambiguous_file_line: several line-only seams share the line; ambiguous_span_overlap: equal or crossing innermost seam spans) are counted as runtime_inconclusive until a seam_id or unambiguous location is available

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
| src/display.rs:20 | * | caught | `no_containing_seam` | unknown |
| src/display.rs:20 | - | caught | `no_containing_seam` | unknown |

## Static Seams Without Runtime Data

Sample only; see JSON `static_without_runtime_total` for the full count.

| Seam | Kind | Class | Location | Confidence label |
| --- | --- | --- | --- | --- |
| `8828427428828b64` | `call_presence` | `ungripped` | src/display.rs:20 | `no_runtime_data` |

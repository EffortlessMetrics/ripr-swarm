# ripr mutation calibration report

Status: advisory

This report joins static seam evidence to supplied cargo-mutants runtime data. Runtime outcome vocabulary in this report comes from that runtime data; static ripr reports continue to use audit vocabulary only.

## Summary

| Metric | Count |
| --- | ---: |
| static_seams_total | 26 |
| mutants_total | 27 |
| matched_total | 13 |
| ambiguous_file_line_total | 0 |
| ambiguous_span_overlap_total | 6 |
| unmatched_mutants_total | 8 |
| static_without_runtime_total | 12 |

## Static/runtime agreement

| Agreement bucket | Count |
| --- | ---: |
| static_gap_and_runtime_signal | 7 |
| static_gap_without_runtime_signal | 9 |
| runtime_signal_without_static_gap | 10 |
| static_clean_and_runtime_clean | 0 |
| runtime_inconclusive | 6 |

Precision notes:

- runtime gap signals are imported runtime labels such as missed, survived, not_caught, or uncaught
- runtime clean signals are imported runtime labels such as caught or timeout
- static_gap_without_runtime_signal includes static gap seams with no matched runtime gap signal in this import
- runtime records with a complete span join by span_containment to the unique innermost static seam whose span contains the mutated range; with no containing span they fall back to file and line over seams without a span, and records without a complete span join by file and line
- ambiguous runtime gap signals (ambiguous_file_line: the file/line fallback found several seams on the line; ambiguous_span_overlap: equal or crossing innermost seam spans) are counted as runtime_inconclusive until a seam_id or unambiguous location is available

### Runtime signals without static gaps

| Runtime mutant | Location | Runtime outcome | Static class | Confidence label | Reason |
| --- | --- | --- | --- | --- | --- |
| `crates/atuin-ai/src/context.rs:73:9: replace && with \|\| in capability_strings` | crates/atuin-ai/src/context.rs:73 | missed | `strongly_gripped` | `contradicts_static_clean` | runtime gap signal joined to a static-clean seam |
| `crates/atuin-ai/src/context.rs:78:79: delete ! in capability_strings` | crates/atuin-ai/src/context.rs:78 | missed | `strongly_gripped` | `contradicts_static_clean` | runtime gap signal joined to a static-clean seam |
| `crates/atuin/src/logs/otel/enabled.rs:62:12: delete ! in OtelCtx::try_enable` | crates/atuin/src/logs/otel/enabled.rs:62 | missed | `strongly_gripped` | `contradicts_static_clean` | runtime gap signal joined to a static-clean seam |
| `crates/atuin-ai/src/context.rs:56:5: replace capability_strings -> Vec<String> with vec!["xyzzy".into()]` | crates/atuin-ai/src/context.rs:56 | missed | `unmatched` | `runtime_only_signal` | runtime gap signal did not join to a static seam |
| `crates/atuin-ai/src/context.rs:56:5: replace capability_strings -> Vec<String> with vec![String::new()]` | crates/atuin-ai/src/context.rs:56 | missed | `unmatched` | `runtime_only_signal` | runtime gap signal did not join to a static seam |
| `crates/atuin-ai/src/context.rs:56:5: replace capability_strings -> Vec<String> with vec![]` | crates/atuin-ai/src/context.rs:56 | missed | `unmatched` | `runtime_only_signal` | runtime gap signal did not join to a static seam |
| `crates/atuin-ai/src/context.rs:117:9: replace ClientContext::to_json -> serde_json::Value with Default::default()` | crates/atuin-ai/src/context.rs:117 | missed | `unmatched` | `runtime_only_signal` | runtime gap signal did not join to a static seam |
| `crates/atuin/src/command/client/store/rebuild.rs:53:9: replace Rebuild::rebuild_history -> Result<()> with Ok(())` | crates/atuin/src/command/client/store/rebuild.rs:53 | missed | `unmatched` | `runtime_only_signal` | runtime gap signal did not join to a static seam |
| `crates/atuin/src/command/client/store/rebuild.rs:71:9: replace Rebuild::rebuild_scripts -> Result<()> with Ok(())` | crates/atuin/src/command/client/store/rebuild.rs:71 | missed | `unmatched` | `runtime_only_signal` | runtime gap signal did not join to a static seam |
| `crates/atuin/src/logs/otel/enabled.rs:99:9: replace <impl Drop for OtelCtx>::drop with ()` | crates/atuin/src/logs/otel/enabled.rs:99 | missed | `unmatched` | `runtime_only_signal` | runtime gap signal did not join to a static seam |

### Static gaps without runtime signals

| Seam | Class | Location | Confidence label | Reason |
| --- | --- | --- | --- | --- |
| `a2a1825a2a7a3178` | `ungripped` | crates/atuin-ai/src/context.rs:85 | `no_runtime_data` | static gap seam has no matched runtime record in this import |
| `cb384a36e083789a` | `ungripped` | crates/atuin-ai/src/context.rs:85 | `no_runtime_data` | static gap seam has no matched runtime record in this import |
| `1026575689c7153b` | `ungripped` | crates/atuin-ai/src/context.rs:99 | `no_runtime_data` | static gap seam has no matched runtime record in this import |
| `418947cf842e6ee5` | `activation_unknown` | crates/atuin/src/command/client/store/rebuild.rs:29 | `no_runtime_data` | static gap seam has no matched runtime record in this import |
| `ba5d32401a508d4d` | `ungripped` | crates/atuin/src/command/client/store/rebuild.rs:71 | `no_runtime_data` | static gap seam has no matched runtime record in this import |
| `69c30532868c4586` | `ungripped` | crates/atuin/src/logs/otel/enabled.rs:38 | `no_runtime_data` | static gap seam has no matched runtime record in this import |
| `6f6d7e4d5197a1de` | `ungripped` | crates/atuin/src/logs/otel/enabled.rs:58 | `no_runtime_data` | static gap seam has no matched runtime record in this import |
| `662e474d4c303fcb` | `ungripped` | crates/atuin/src/logs/otel/enabled.rs:62 | `no_runtime_data` | static gap seam has no matched runtime record in this import |
| `eef0358604b2e8fa` | `weakly_gripped` | crates/atuin/src/logs/otel/enabled.rs:99 | `no_runtime_data` | static gap seam has no matched runtime record in this import |

## Runtime Outcome Counts

| Runtime outcome | Count |
| --- | ---: |
| missed | 26 |
| unviable | 1 |

## Matched Mutants

| Seam | Class | Oracle | Mutation operator | Runtime outcome | Join | Confidence label |
| --- | --- | --- | --- | --- | --- | --- |
| `33f2fc9a4bfabc2e` | `strongly_gripped` | `unknown`/`unknown` | \|\| | missed | `span_containment` | `contradicts_static_clean` |
| `9b3a32b1fdffede4` | `strongly_gripped` | `unknown`/`unknown` | unknown | missed | `span_containment` | `contradicts_static_clean` |
| `f49c5d86e1d4a8fe` | `ungripped` | `unknown`/`unknown` | != | missed | `span_containment` | `supports_static_gap` |
| `2d733edde2d4a212` | `ungripped` | `unknown`/`unknown` | "xyzzy".into() | missed | `file_line` | `supports_static_gap` |
| `2d733edde2d4a212` | `ungripped` | `unknown`/`unknown` | String::new() | missed | `file_line` | `supports_static_gap` |
| `25a88f39b588c3f3` | `activation_unknown` | `unknown`/`unknown` | Ok(()) | missed | `file_line` | `supports_static_gap` |
| `3a2cc8888f55e1c1` | `ungripped` | `unknown`/`unknown` | Ok(None) | missed | `file_line` | `supports_static_gap` |
| `3a2cc8888f55e1c1` | `ungripped` | `unknown`/`unknown` | Ok(Some(Default::default())) | missed | `file_line` | `supports_static_gap` |
| `ae7b3b349a0b2660` | `ungripped` | `unknown`/`unknown` | != | missed | `span_containment` | `supports_static_gap` |
| `ae7ec0349a0e3e36` | `ungripped` | `unknown`/`unknown` | != | missed | `span_containment` | `supports_static_gap` |
| `ae7b3d349a0b29c6` | `ungripped` | `unknown`/`unknown` | && | missed | `span_containment` | `supports_static_gap` |
| `ae7b3d349a0b29c6` | `ungripped` | `unknown`/`unknown` | unknown | missed | `span_containment` | `supports_static_gap` |
| `a72597349643e206` | `strongly_gripped` | `unknown`/`unknown` | unknown | missed | `span_containment` | `contradicts_static_clean` |

## Ambiguous File/Line Matches

No line-only runtime mutants matched multiple static seams.

## Ambiguous Span Overlaps

| Runtime mutant | Location | Runtime outcome | Confidence label | Candidate seams |
| --- | --- | --- | --- | --- |
| `crates/atuin-ai/src/context.rs:40:5: replace history_output_capability_available -> bool with false` | crates/atuin-ai/src/context.rs:40 | missed | `ambiguous_runtime_join` | `000f706384b63df2`, `f17a02397c93826a` |
| `crates/atuin-ai/src/context.rs:40:5: replace history_output_capability_available -> bool with true` | crates/atuin-ai/src/context.rs:40 | missed | `ambiguous_runtime_join` | `000f706384b63df2`, `f17a02397c93826a` |
| `crates/atuin-ai/src/context.rs:40:30: replace && with \|\| in history_output_capability_available` | crates/atuin-ai/src/context.rs:40 | missed | `ambiguous_runtime_join` | `000f706384b63df2`, `f17a02397c93826a` |
| `crates/atuin-ai/src/context.rs:85:9: replace AppContext::capabilities_as_strings -> Vec<String> with vec!["xyzzy".into()]` | crates/atuin-ai/src/context.rs:85 | missed | `ambiguous_runtime_join` | `a2a1825a2a7a3178`, `cb384a36e083789a` |
| `crates/atuin-ai/src/context.rs:85:9: replace AppContext::capabilities_as_strings -> Vec<String> with vec![String::new()]` | crates/atuin-ai/src/context.rs:85 | missed | `ambiguous_runtime_join` | `a2a1825a2a7a3178`, `cb384a36e083789a` |
| `crates/atuin-ai/src/context.rs:85:9: replace AppContext::capabilities_as_strings -> Vec<String> with vec![]` | crates/atuin-ai/src/context.rs:85 | missed | `ambiguous_runtime_join` | `a2a1825a2a7a3178`, `cb384a36e083789a` |

## Unmatched Runtime Mutants

| Location | Mutation operator | Runtime outcome | Reason | Test command |
| --- | --- | --- | --- | --- |
| crates/atuin-ai/src/context.rs:56 | vec!["xyzzy".into()] | missed | `no_containing_seam` | unknown |
| crates/atuin-ai/src/context.rs:56 | vec![String::new()] | missed | `no_containing_seam` | unknown |
| crates/atuin-ai/src/context.rs:56 | vec![] | missed | `no_containing_seam` | unknown |
| crates/atuin-ai/src/context.rs:99 | Default::default() | unviable | `no_containing_seam` | unknown |
| crates/atuin-ai/src/context.rs:117 | Default::default() | missed | `no_containing_seam` | unknown |
| crates/atuin/src/command/client/store/rebuild.rs:53 | Ok(()) | missed | `no_containing_seam` | unknown |
| crates/atuin/src/command/client/store/rebuild.rs:71 | Ok(()) | missed | `no_containing_seam` | unknown |
| crates/atuin/src/logs/otel/enabled.rs:99 | () | missed | `no_containing_seam` | unknown |

## Static Seams Without Runtime Data

Sample only; see JSON `static_without_runtime_total` for the full count.

| Seam | Kind | Class | Location | Confidence label |
| --- | --- | --- | --- | --- |
| `ad0475b208359476` | `call_presence` | `strongly_gripped` | crates/atuin-ai/src/context.rs:73 | `no_runtime_data` |
| `9b3732b1fdfdb80d` | `call_presence` | `strongly_gripped` | crates/atuin-ai/src/context.rs:78 | `no_runtime_data` |
| `9b2cb7b1fdf49287` | `call_presence` | `strongly_gripped` | crates/atuin-ai/src/context.rs:78 | `no_runtime_data` |
| `9b22a3b1fdec1c06` | `call_presence` | `strongly_gripped` | crates/atuin-ai/src/context.rs:78 | `no_runtime_data` |
| `1026575689c7153b` | `call_presence` | `ungripped` | crates/atuin-ai/src/context.rs:99 | `no_runtime_data` |
| `418947cf842e6ee5` | `call_presence` | `activation_unknown` | crates/atuin/src/command/client/store/rebuild.rs:29 | `no_runtime_data` |
| `ba5d32401a508d4d` | `call_presence` | `ungripped` | crates/atuin/src/command/client/store/rebuild.rs:71 | `no_runtime_data` |
| `69c30532868c4586` | `call_presence` | `ungripped` | crates/atuin/src/logs/otel/enabled.rs:38 | `no_runtime_data` |
| `6f6d7e4d5197a1de` | `call_presence` | `ungripped` | crates/atuin/src/logs/otel/enabled.rs:58 | `no_runtime_data` |
| `662e474d4c303fcb` | `call_presence` | `ungripped` | crates/atuin/src/logs/otel/enabled.rs:62 | `no_runtime_data` |
| `8a8382794c05b62b` | `predicate_boundary` | `strongly_gripped` | crates/atuin/src/logs/otel/enabled.rs:99 | `no_runtime_data` |
| `eef0358604b2e8fa` | `call_presence` | `weakly_gripped` | crates/atuin/src/logs/otel/enabled.rs:99 | `no_runtime_data` |

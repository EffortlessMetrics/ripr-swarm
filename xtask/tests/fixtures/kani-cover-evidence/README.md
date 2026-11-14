# Native cover-evidence controls

These five standalone inputs and raw outputs retain bounded Kani 0.68.0
compatibility evidence for #14460. `provenance.json` records each actual command,
native exit code and SHA-256 of the source, stdout and stderr. The capture
versions are retained verbatim in `tool-version.txt`.

The `cover-fail` and `cover-unreachable` sources are copied from Kani's own expected tests at
[`0d2328a93f0e0ff66132d6bfa1a7d884877cf862`](https://github.com/model-checking/kani/tree/0d2328a93f0e0ff66132d6bfa1a7d884877cf862/tests/expected/cover).
`cover-pass-compact` is a local two-value domain with two reachable cover
witnesses and an unreachable ordinary assertion. `cover-undetermined-compact`
is a local three-iteration loop with unwind bound two; `no-cover` is a local
constant-assertion control. Sources have `.rs.txt` suffixes
because they are retained fixture data, not modules of either owning crate;
the original source bytes and native diagnostic paths are unchanged. Each
capture was freshly executed with a relative source filename from the fixed
generic directory `/tmp/ripr-kani-cover-controls`; the raw output was not
sanitized or rewritten.

| Capture | Native exit | Native evidence | Lane disposition |
| --- | ---: | --- | --- |
| `cover-pass-compact` | 0 | Successful, 2 of 2 covers satisfied; ordinary assertions include unreachable paths | `VERIFIED` |
| `cover-fail` | 0 | Successful, 0 of 1 covers satisfied; unsatisfiable cover | `FAILED` reachability policy |
| `cover-unreachable` | 0 | Successful, 1 of 3 covers satisfied; 2 unreachable covers | `FAILED` reachability policy |
| `cover-undetermined-compact` | 1 | Native verification failed, 0 of 1 covers satisfied and 1 undetermined | `FAILED` native verification |
| `no-cover` | 0 | Successful, no cover summary | `NOT-ESTABLISHED` |

`pinned_native_cover_captures_keep_distinct_lane_dispositions` replays these
unmodified outputs through the real block join and classification functions.
It does not execute Kani. Separate synthetic controls exercise success-looking
results with undetermined/error statuses, malformed or repeated summaries,
expected-panic success, and the process runner's report/exit behavior.

To repeat a native observation, restore a source's `.rs` filename in the
recorded working directory and run
`kani <source.rs> -Z unstable-options --harness-timeout 30s` with the recorded
tool and a separate 60-second outer deadline. Runtime timings, temporary paths and compiler diagnostics can differ;
the existing files retain the original observation. These controls establish
output-format compatibility, not a native proof of RIPR's production harnesses,
complete native inventory, authenticated execution, or full-lane qualification.

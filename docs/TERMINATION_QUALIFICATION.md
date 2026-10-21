# Termination qualification

`cargo xtask termination-qualification --binary <path>` runs one deterministic
adversarial corpus against one exact `ripr` binary and writes a JSON and a
Markdown packet from a single DTO. It is the harness behind #4505 and its
corpus children #4511, #4512 and #4513. It changes no product behavior, builds
or installs nothing, and does not decide qualification: #2769 consumes the
packet.

```bash
cargo xtask termination-qualification --binary <path-to-ripr> \
  --removal-control rust_nesting=<repairs-disabled-ripr> \
  --removal-control python_nesting=<repairs-disabled-ripr> \
  --removal-control path_glob_suppression=<repairs-disabled-ripr> \
  --removal-control path_glob_generated_file=<repairs-disabled-ripr>
```

Outputs: `target/ripr/reports/termination-qualification.{json,md}`.
Other flags: `--expected-sha256`, `--expected-version`, `--family`,
`--no-repeat`, `--deadline-seconds`, `--allow-incomplete`, `--work-dir`,
`--out`. The default row deadline is 60 seconds.

## What runs

Every row writes generated files into an owned, disposable workspace, launches
the binary by absolute path with a scrubbed `PATH`, and removes the workspace
afterwards. The shared process owner (`xtask/src/run.rs`) supplies process
group custody, deadline kill and bounded drains.

| Family | Route | Shapes |
|---|---|---|
| `rust_nesting` | `ripr check --diff` and `ripr pilot` | nested blocks, `if`, parentheses, closures, prefix operators, generic arguments, `else if` chains, long operator chains; a below-budget control; comment, string, char and raw-string lookalikes |
| `python_nesting` | `ripr check --diff` | long binary, ternary, `not`, unary minus and `await` runs; long `elif` chain; a deep-but-supported control; comment and string lookalikes |
| `path_glob_suppression` | `ripr check --suppression-policy` | `*`, `?`, `**`, zero-segment `**`, adjacent `**`, `.` and empty segments, Unicode, empty pattern, many-star and many-segment nonmatches |
| `path_glob_generated_file` | `ripr check` with `generated_file_patterns` | the same dialect through the second consumer, including its slashless-pattern rule |

Each Rust and Python row carries an unchanged hostile file outside the diff
and a changed file whose finding must stay visible. `pilot` is the only route
that reads every Rust file and discloses `rust_nesting_budget`; the disclosure
is on stderr, so the pilot row reads stderr plus the `repo-exposure.json` seam
list.

## Result taxonomy

Each row ends in exactly one #4505 state. Only the first two pass.

| State | Meaning |
|---|---|
| `passed_supported_analysis` | Full analysis, no limitation, expected evidence present |
| `passed_typed_limitation` | Declined with the named limitation; unaffected evidence kept |
| `semantic_failure` | Wrong limitation class, false limit on a supported input, unsuppressed or silently excluded path, non-JSON output |
| `process_abort` | Signal or abort exit code |
| `nonterminal_deadline` | The outer deadline fired |
| `missing_or_false_clean_limitation` | A required limitation or exclusion disclosure is absent, or the outcome is complete despite it |
| `wrong_subject` | The changed file's evidence was lost |
| `instrument_failure` | Launch, workspace or cleanup failure |
| `not_run` | The row did not execute |

## Controls

A packet that cannot fail proves nothing, so the aggregate needs these.

- **Instrument controls** (Unix) run stub binaries through the same path and
  must reach `process_abort`, `nonterminal_deadline`,
  `missing_or_false_clean_limitation`, `wrong_subject`, `semantic_failure` and
  `instrument_failure`. The hang stub's child must also be gone after the
  deadline kill.
- **Repeat control** reruns the corpus on the unchanged binary; every row's
  semantic content must match.
- **Removal controls** run a family's discriminator rows against a binary with
  the repair disabled. At least one row must abort, hang, or return a false
  clean result. A control byte-identical to the candidate does not count.
  Without `--removal-control` for a selected family the aggregate is
  `incomplete`, and the command exits nonzero unless `--allow-incomplete`.

A repairs-disabled binary is a normal build of the candidate source with the
three guards neutralised (`rust_nesting_refusal` returns `None`, the Python
tree-depth budget is `usize::MAX`, `path_glob_matches` is the recursive
matcher kept in `analysis/path_glob.rs` tests).

## Packet identity

The packet carries the binary's path, size, SHA-256 and reported version, host
identity (OS, architecture, family), the environment boundary, per-case argv, deadline,
termination, input digests, subject counts, stdout and stderr digests, cleanup
state, controls, non-claims and the aggregate. The `digest` covers everything
except elapsed time, the host-local binary path and itself. Owned workspace
paths are replaced by `<case-root>` before hashing.

## Claim boundary

A green packet says one exact binary terminated honestly on this retained
corpus. It does not bind the binary to a source SHA or candidate ref, exercise
LSP, prove absence of every crash or hang class, or establish latency. Python
f-string replacement-field shapes the tree-depth estimate does not model are
outside the corpus.

Known gaps against the child issues, each deliberate for this first harness:

- No toolchain identity, per-finding identities, parser or fallback state, or
  separate exit-status field in a row (#4505, #4511, #4512 evidence rows).
- `check` does not project the Rust `rust_nesting_budget` decline, so those
  rows assert termination and changed-file evidence only, and human/JSON
  parity is not checked (#9308).
- Python rows use `check --diff` only: no `pilot` route, gate or agent
  consumer, and no multi-line or continuation forms.
- Glob corpus omits mixed separators, root-relative boundaries and an empty
  path, and the generated-file family omits `.`/empty-segment and
  adjacent-`**` rows (parity of normalization across consumers is partial).
- #4510 execution controls are not built: planted ambient `ripr`, moved
  candidate after setup, foreign CWD with a decoy repository, preseeded stale
  artifact and simulated cleanup failure. `--expected-sha256` is optional; a
  run without it admits any runnable binary.
- A non-Unix host is refused rather than skipped.

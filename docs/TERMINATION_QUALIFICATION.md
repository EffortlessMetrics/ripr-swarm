# Termination qualification

`cargo xtask termination-qualification --binary <path>` runs one deterministic
adversarial corpus against one exact `ripr` binary and writes a JSON and a
Markdown packet from a single DTO (#4505, #4511, #4512; the path-glob family,
#4513, follows in a stacked PR). It changes no product behavior, builds or
installs nothing, and does not decide qualification: #2769 consumes the packet.

```bash
cargo xtask termination-qualification --binary <path-to-ripr> \
  --removal-control rust_nesting=<repairs-disabled-ripr> \
  --removal-control python_nesting=<repairs-disabled-ripr>
```

Outputs: `target/ripr/reports/termination-qualification.{json,md}`. Other
flags: `--expected-sha256`, `--expected-version`, `--family`, `--no-repeat`,
`--deadline-seconds` (default 60), `--allow-incomplete`, `--work-dir`, `--out`.

## What runs

Every row writes generated files into an owned, disposable workspace, launches
the binary by absolute path with a fixed `PATH` and `HOME`/`TMPDIR` inside the
workspace (other parent variables are inherited), and removes the workspace. `xtask/src/run.rs` supplies process-group custody, deadline kill
and bounded drains.

| Family | Route | Shapes |
|---|---|---|
| `rust_nesting` | `ripr check --diff` and `ripr pilot` | nested blocks, `if`, parentheses, closures, prefix operators, generic arguments, `else if` chains, long operator chains; a below-budget control; comment, string, char and raw-string lookalikes |
| `python_nesting` | `ripr check --diff` | long binary, ternary, `not`, unary minus and `await` runs; long `elif` chain; a deep-but-supported control; comment and string lookalikes |

Each Rust and Python row carries an unchanged hostile file outside the diff
and a changed file whose finding must stay visible. `pilot` is the only route
that reads every Rust file. A declined Rust file is a typed limitation on both
routes (#9308): `check` reports `analysis_complete=false` with a
`producer_failure` limitation naming the file and `rust_nesting_budget`; the
pilot row requires the summary's `declined_rust_files` entry, a
`limited_declined_files` `run_status` and a `rust_files_declined` limitation in
`repo-exposure.json`, plus the unaffected seam.

## Result taxonomy

Each row ends in exactly one #4505 state. Only `passed_supported_analysis`
(full analysis, expected evidence present) and `passed_typed_limitation`
(declined with the named limitation, unaffected evidence kept) pass. The
failing states are `semantic_failure` (wrong limitation class, false limit on a
supported input, non-JSON output), `process_abort`, `nonterminal_deadline`,
`missing_or_false_clean_limitation`, `wrong_subject` (the changed file's
evidence was lost), `instrument_failure` (launch, workspace or cleanup) and
`not_run`.

## Controls

A packet that cannot fail proves nothing, so the aggregate needs these.

- **Instrument controls** (Unix) run stub binaries through the same path and
  must reach each failing state, and the hang stub's child must be gone after
  the deadline kill.
- **Repeat control** reruns the corpus; every row's semantic content must match.
- **Removal controls** run a family's discriminator rows against a binary with
  the repair disabled. At least one row must abort, hang, or return a false
  clean result. A control byte-identical to the candidate does not count.
  A missing removal control, `--no-repeat` or `--family` exclusion makes the
  aggregate `incomplete`; the command exits nonzero unless `--allow-incomplete`.

A repairs-disabled binary is a build of the candidate source with the guards
neutralised (`rust_nesting_refusal` returns `None`, the Python tree-depth
budget is `usize::MAX`) and must report the candidate's `--version`.

## Packet identity

The packet carries the binary's path, size, SHA-256 and version, host identity,
environment boundary, per-case argv, deadline, termination, input digests,
subject counts, stdout and stderr digests, cleanup state, controls, non-claims
and the aggregate. The `digest` covers everything except elapsed time, the
host-local binary path and itself; owned workspace paths become `<case-root>`.

## Claim boundary

A green packet says one exact binary terminated honestly on this retained
corpus. It does not bind the binary to a source SHA or candidate ref, exercise
LSP, prove absence of every crash or hang class, or establish latency.

Known gaps against the child issues, each deliberate for this first harness:

- Python rows use `check --diff` only, with no multi-line or continuation forms.
- #4510 controls not built: planted ambient `ripr`, foreign CWD with a decoy
  repository, preseeded stale artifact, simulated cleanup failure.
  `--expected-sha256` is optional. A non-Unix host is refused, not skipped.
- A removal control is bound to the candidate by `--version` only, not by a
  source or build identity.

# Termination qualification

`cargo xtask termination-qualification --binary <path>` runs one deterministic
adversarial corpus against one exact `ripr` binary and writes a JSON and a
Markdown packet from a single DTO (#4505, #4511, #4512, #4513). It changes no
product behavior, builds or installs nothing, and does not decide
qualification: #2769 consumes the packet.

```bash
# Build the candidate source and one repairs-disabled copy per repair.
cargo xtask termination-removal-builds --out target/ripr/termination-qualification/removal-builds

# Qualify the exact binary, with controls the manifest vouches for.
cargo xtask termination-qualification --binary <path-to-ripr> \
  --expected-commit <candidate-sha> \
  --removal-manifest target/ripr/termination-qualification/removal-builds/manifest.json
```

On the cut, run the first command in a clean checkout of `CANDIDATE_SHA` with
`--profile release`, and replace `--expected-commit` with
`--candidate-manifest <#1609 manifest.json>`, which supplies `candidate.sha`.
That field is this harness's whole contract with the manifest; #1609's DTO owns
the rest.

A single control can still be named with `--removal-control <family>=<binary>`;
a family takes one control only, from either source.


Outputs: `target/ripr/reports/termination-qualification.{json,md}`. Other
flags: `--expected-sha256`, `--expected-version`, `--expected-commit`, `--family`, `--no-repeat`,
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
| `path_glob_suppression` | `ripr check --suppression-policy` | `*`, `?`, `**`, zero-segment `**`, adjacent `**`, `.` and empty segments, Unicode, empty pattern, many-star and many-segment nonmatches |
| `path_glob_generated_file` | `ripr check` with `generated_file_patterns` | the same dialect through the second consumer, including its slashless-pattern rule |

Corpus generation `ripr-termination-corpus-v2` retains the original row IDs and
expectations and adds generated-file leading `./`, inner `.`, and empty-segment
rows from the same literal table as suppression policy. A normalized nonmatch
is retained for both consumers. This expands the glob corpus from 29 to 34 rows;
the packet format remains `ripr-termination-qualification-v1`. Historical
version 1 packets retain their original corpus identity and coverage.

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
- **Execution controls** (Unix) run stubs through the launch path and check that
  the harness decided what ran: the child sees only the harness `PATH` and no
  ambient `ripr`, runs in the owned case root, discovers no repository above it
  (Git discovery stops at the case root's parent and `GIT_DIR`, `GIT_WORK_TREE`
  and `GIT_INDEX_FILE` are removed; the control plants a decoy repository above
  the root and the probe must not find it), cannot read an artifact that was
  in the workspace before launch (the harness removes it first), and runs the
  admitted bytes after the candidate path is rewritten. A passing row whose
  workspace cannot be removed becomes an `instrument_failure`; the denial is
  injected, because a root user ignores directory mode bits.
- **Repeat control** reruns the corpus; every row's semantic content must match.
- **Removal controls** run a family's discriminator rows against a binary with
  the repair disabled. At least one row must abort, hang, or return a false
  clean result. A control byte-identical to the candidate does not count.
  A missing removal control, `--no-repeat` or `--family` exclusion makes the
  aggregate `incomplete`; the command exits nonzero unless `--allow-incomplete`.

`cargo xtask termination-removal-builds` scripts the repairs-disabled builds in
a scratch worktree of `HEAD`: `rust_nesting_refusal` returns `None`, the Python
nesting and tree-depth budgets are `usize::MAX`, and both glob entry points use
a naive recursive matcher. An edit that no longer applies exactly once fails
the build. Its `manifest.json` records the source commit, the candidate and
each control's SHA-256 and version, and a digest of the applied edits. With
`--removal-manifest`, a control counts only if its bytes match the manifest and
the candidate reports the manifest's commit; a patched build reports that
commit with `-dirty`, which is compared away. `--expected-commit` takes the
`CANDIDATE_SHA` from the frozen-candidate manifest (#1609) and refuses a binary
built from another commit. Every control runs from an owned, re-hashed copy.

## Packet identity

The packet carries the binary's path, size, SHA-256 and version, host identity,
environment boundary, per-case argv, deadline, termination, input digests,
subject counts, stdout and stderr digests, cleanup state, controls, non-claims
and the aggregate. The `digest` covers everything except elapsed time, the
host-local binary path and itself; owned workspace paths become `<case-root>`.

## Claim boundary

A green packet says one exact binary terminated honestly on this retained
corpus. Unless `--expected-commit` is given, it does not bind the binary to a
source SHA or candidate ref, and even then the bind is the commit the binary
reports in `--version`, not the build that produced it. It does not exercise LSP, prove absence of every crash or hang class, or establish latency.

Known gaps against the child issues, each deliberate for this first harness:

- Python rows use `check --diff` only, with no multi-line or continuation forms.
- The decoy-repository control plants its repository above the case root; it
  does not run the harness from a decoy caller directory, because the child's
  working directory is always the owned root.
  `--expected-sha256` is optional. A non-Unix host is refused, not skipped.
- The #1609 manifest does not exist yet. The reader takes `candidate.sha` only
  and is tested against an inline fixture, not the real DTO.
- A control passed with `--removal-control` instead of a manifest is bound to
  the candidate by `--version` only; the packet says so in the control detail.
- The candidate in the packet is the shipped binary, not the manifest's own
  candidate build; the binding between them is the commit in `--version`, not
  a byte comparison.
- Glob corpus omits mixed separators, root-relative boundaries and an empty
  path, and the generated-file family omits adjacent-`**` rows.

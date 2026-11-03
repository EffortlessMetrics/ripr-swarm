# Exit Codes

ripr uses three ordinary command exit codes, plus standalone Unix check interruption:

| Code | Meaning |
|------|---------|
| `0`  | The command completed. |
| `2`  | The command could not complete: usage, parse, operational, or internal error. |
| `3`  | ripr declined: the command ran and reached a blocking result, or a request it will not run (the output says why). |
| `130` | Supported standalone Unix `check` analysis was interrupted. |

`ripr check` is advisory: it exits `0` whether or not it found gaps, including
`exposed` and `weakly_exposed` findings. A non-zero exit from `check` means the
analysis did not complete, not that it disapproved of your diff. The blocking
decision belongs to `ripr gate evaluate`, which exits `3` when the gate blocks
after writing its report. Do not build a CI gate on `check`'s exit code; read
its output, or run the gate.

## Why codes 2 and 3?

ripr uses exit code `2` for all could-not-complete conditions (not `1`) to
distinguish it from shell-level errors (which typically use `1`). Code `3` is
reserved for outcomes where the command itself ran to a deliberate, typed
answer — a blocking gate decision, or a typed refusal such as `ripr agent
verify-execute` declining a packet (the refusal JSON document is on stdout)
— so an orchestrator can branch on the exit status alone:

- `0`: the command completed (for `verify-execute`, the verification ran;
  read the disposition in the stdout JSON).
- `3`: the command completed by reaching a blocking decision or typed
  refusal; read the report or the stdout JSON document for the answer
  (standalone `ripr agent verify` is the exception: its refusal is named on
  stderr and stdout stays empty; see below. `ripr agent repair`,
  `ripr agent card` and `ripr agent stub` print their refusal document
  (`agent_card_refusal` / `rust_test_stub` `state: refused`) on stdout only
  with `--json`, with the prose on stderr; without it stdout stays empty and
  stderr names the cause. An unknown `--seam-id` is one typed refusal with
  one exit, `3`, on `agent start`, `agent card`, `agent stub`,
  `agent packet`, and `agent repair --phase before`. A known seam that
  agent policy withholds (#10712) is the same typed refusal (exit `3`) on
  `agent start`, `agent card`, `agent packet`, and
  `agent repair --phase before`; `agent stub` still produces the stub
  (exit `0`) with the omission disclosed in `warnings` and no repair
  journey offered. With `--json` the
  document is on stdout and the prose on stderr, except `agent start` and
  `agent repair --phase before`, whose
  refusals are prose on stderr with empty stdout).
- `2`: the invocation or operation failed; retrying differently is
  appropriate.

`ripr help --json` projects this ordinary completion mapping as a typed per-command `exit`
object (RIPR-SPEC-0190). Orchestrators that discover commands from that
document should branch on `exit`, not on free-text `stop_states`. External
standalone interruption (130 or native signal termination) is separate from
that completion metadata.

Error and panic diagnostics on stderr are best effort. If a stderr reader
closes after a JSON refusal has been written, the command retains that stdout
document and its original exit `2` or `3`. A failed panic diagnostic still
terminates with `2`; reporting it must not cause a recursive panic or abort.
Once `agent card` or `agent stub` reaches a named refusal, its stdout JSON
document is written first and the stderr prose is best effort: a closed
stderr reader keeps the stdout document and preserves exit `3`. An earlier analysis-stage failure can still exit `2`.
The refusal exit `3` also requires that the machine answer was delivered: for
`agent card`, `agent stub` and an unknown-seam `agent packet` under `--json`, a
refusal document that cannot be rendered or cannot be written to stdout (for
example a closed stdout reader) exits `2` instead, since no typed answer
reached the caller.

## When you see exit code 2

- **Analysis error**: the diff could not be read (a missing or unreadable
  `--diff` path, or a directory), the diff exceeded the hard
  `RIPR_MAX_DIFF_*` guards (`diff_scope_oversized` with no analysis run), the
  base ref could not be resolved, or the workspace root could not be
  determined. A read diff whose typed outcome is `unsupported_input`
  (for example malformed input or a truncated declared hunk) exits `2`
  after writing its original report, including retained advisory findings,
  counts, input identity, limitations and recovery routes. JSON stdout remains
  one ordinary check or diff document, with `analysis_complete: false`; it is not
  replaced by a refusal envelope. This changes the earlier unsupported-input
  exit-`0` contract (#7440 for check, #7793 for diff). Empty or whitespace-only input stays complete
  `no_scope` with exit `0`; it does not establish analyzed behavior.
  An `unsupported_input` run with `--write-artifact` neither creates nor
  replaces the requested reusable artifact; its report keeps direct-input
  drill-in commands rather than pointing to that unwritten artifact.
  A run that analyzed only
  part of its scope also exits `0`, with `partial_with_limitations`: a diff
  over the smaller partial budget (its limitation is also named
  `diff_scope_oversized`), a changed Rust file the parser refused and read
  lexically, or a changed file whose language adapter is unavailable. The
  parser follows stable Rust, so a changed file using nightly-only syntax it
  cannot parse (guard patterns, never patterns) also makes the run partial.
  With `--json`, every pre-report, post-argv-parse `check` failure — the analysis errors
  above, an unreadable config or suppression policy, a git timeout, or any
  other failure the command can produce — also writes a machine-readable
  refusal document to stdout naming the failure; the exit stays `2` and the
  human prose stays on stderr. The same envelope covers the other `--json`
  commands (#7331): `diff --json`, `rerun --json`, `context` (which emits
  JSON only), `agent brief --json`, and `agent packet --json` each write the
  shared refusal shape keyed with their own `schema_version` (rerun
  `ripr-targeted-rerun-v1`, context `1.1`, diff and agent brief `0.1`, agent
  packet `0.5`; an unknown or policy-withheld `agent packet --seam-id` is the one typed
  refusal among these and exits `3`, see above), and `mode` appears only on commands that accept `--mode`.
  A fresh `context` run preserves the check producer's typed failure and
  recovery route for both committed and working-tree analysis; an invalid
  base therefore names `base_unresolvable`, not the generic `analysis_failed`.
  For `check`, the machine-readable formats also carry their own refusal
  document on stdout (#7332): SARIF renders a schema-valid run whose only
  signal is a tool-configuration notification, GitHub renders one workflow
  error annotation, `badge-json` renders the badge family's degraded-state
  convention (`status: "warn"`, `color: "lightgrey"`, zeroed counts,
  `message: "refused: <identity>"`, repair route in the first warning,
  `analysis_outcome: null`; no `run_status` field), and
  `repo-exposure-json` and `agent-seam-packets-json` render their format's
  refused envelope (zeroed counts, `run_status: "refused"`). Machine formats without an
  honest refusal shape yet — `repo-sarif`, `badge-shields`, `badge-plus-*`,
  `repo-badge-*`, `repo-seams-*`, `repo-exposure-md`, and
  `repo-exposure-summary-json` — write
  nothing to stdout and put the prose diagnostic on stderr for now; that
  gap is a named limitation, not a clean result. Those formats keep stdout
  empty.
  Every `check --json` refusal document, scope-guard or typed, also carries
  the shared refusal block (#11584): `status: "refused"` and
  `error: {code, message, next}`, where `code` equals
  `run_limitations[0].category`, `message` is the stderr diagnostic, and
  `next` is the one command to run (`{purpose, command}`) or `null`. A
  completed run carries neither key, so a consumer can branch on `status`
  instead of inferring refusal from an empty `findings` list.
  Usage-class errors — argv errors (`unknown_argument`, `unknown_value`,
  `unknown_command`) and `context` selector misses or ambiguity
  (`selector_miss`) — write the standalone `cli_refusal` document
  (`schemas/ripr/cli-refusal.schema.json`) to stdout when argv requests JSON
  (`--json`, or a `--format` of `json` or a `*-json` family; `context` emits
  JSON only, so it always does). The exit stays `2` and the guidance and
  retry listing stay on stderr (RIPR-SPEC-0003). Without a JSON request,
  usage-class errors keep stdout empty. `explain` has no JSON output, so
  `explain --json` is an `unknown_argument` refusal.
  Fresh selector misses also retain producer-owned language limitations for
  that run, with the configured-off or unavailable-binary recovery. These
  describe analysis scope rather than prove why the selector missed;
  recorded `--from` lookups do not invent ambient language facts (#10720).
  [docs/OUTPUT_SCHEMA.md](OUTPUT_SCHEMA.md) documents the refusal
  shape and its `schema_version`.
- **User error**: unknown command, missing required argument, or invalid
  config.
- **Verification root failure**: `ripr agent verify-execute --json` with a
  missing root or a file root retains exit `2` and the root recovery diagnostic.
  It also emits the existing schema `1` `verification_wrong_root` response on
  stdout with `executed: false` and `result_committed: false`. No packet runs
  and no result is written. This is distinct from a packet path-policy refusal,
  which retains typed-refusal exit `3`.
- **Gate `--root` refusal**: `ripr gate evaluate --root <path>` that is
  missing or not a directory is an argv-level usage refusal: it exits `2`,
  names the root, and does not write `gate-decision.{json,md}`. Evaluation
  does not start. This is distinct from a completed `config_error` decision
  (also exit `2`), which still writes the report before returning.
- **Internal error**: a panic occurred (with a `ripr: internal error` message).
- **Closed output pipe**: the reader of stdout went away early (for example
  `ripr doctor | head`). ripr stops quietly with `2`, not `0`, because its
  output was cut short; it never turns a would-be `3` into a pass. Output
  small enough to be written in full before the reader closes never meets
  the closed pipe, so that run keeps its own exit code.

## When you see exit code 3

- **Gate failure**: `ripr gate evaluate` evaluated successfully and blocked
  the PR; the full report was still written to `--out`.
- **Doctor findings**: `ripr doctor` completed its diagnosis and at least
  one check failed; the full report was already written (human screen or
  `--json` document) and stderr names `doctor found issues` (#5102). A
  usage error or a failure before a report exists stays exit `2`.
- **Typed refusal**: `ripr agent verify-execute` refused the packet (for
  example `verification_rejected_policy` or `verification_wrong_root`) and
  rendered the typed refusal JSON on stdout; `ripr agent repair --phase
  after` refused with a named cause after selecting its attempt — a diverged
  HEAD, drifted analysis inputs, a no-movement verify refusal, a replaced
  trust-binding manifest, or a receipt refused on trusted-surface grounds
  (tracked content differs from the attempt's before-phase head outside the
  allowed test surface; the recovery narration names the commit-first route,
  #5262) — with the cause named on stderr, the refusal recorded on the
  attempt (the trusted-surface refusal is recorded beside the finished
  attempt's after verdict), and, with `--json`, one JSON document on stdout
  (the `repair_after_refusal` document naming the cause and recovery when the
  refusal came before the verify render, otherwise the bare agent verify
  document — the trusted-surface refusal always fires after the verify
  render). Without `--json` stdout stays empty on a refusal and stderr
  carries the cause and recovery. Operational errors after attempt selection (an unreadable
  retained packet or manifest, a failed artifact write) still exit `2`.
- **Typed verify refusal**: standalone `ripr agent verify` refused the pair
  for drifted analysis inputs (`analysis input identities differ`) or no
  repository movement between the artifacts — the same named refusals the
  repair after phase maps to `3`. This is the one exit-`3` path whose stdout
  stays empty: `agent verify`'s stdout is the verify artifact (the packet's
  `next` loop redirects it into `agent-verify.json`), and a rejected verify
  renders nothing to it (RIPR-SPEC-0134). The named cause is on stderr.
  Other verify rejections (unreadable or invalid artifacts, lineage or
  metadata mismatches) exit `2`.
- **Agent stub refusal**: `ripr agent stub` found the gap but will not write
  a stub for it (a side-effect or call-presence change, a changed field of a
  struct the owner does not return directly, an async, unsafe, or generic
  owner, an impl with type or const generics, an impl local to a function
  body or `const` block, an owner behind a cfg in its own file that a plain
  `cargo test` build may not enable, no return value, an out-of-line test module, or
  inline test modules that are all gated by more than `cfg(test)`), or the
  `--at` path names a language the Rust-only generator does not support
  (`unsupported_language`, exit `3`; a `.py` or `.ts` path prints `stub
  generation supports Rust files only`: follow any check-provided test
  guidance, edit and run the test by hand, then refresh the finding), or the selector names no
  reported gap (`seam_not_found`), or the location selector
  matches several files or tied disjoint seams (`selector_ambiguous`: choose
  a specific path or one of the suggested seam IDs), or a single related
  test already reaches the changed predicate boundary and only its assertion
  is weak (`existing_test_weak_oracle`, #10683: strengthen that named test's
  assertion instead of adding a second test). Several inline test modules are not a
  refusal: among those gated by plain `cfg(test)`, the stub goes into the
  one that already names the owner, else the nearest one after it, else the
  nearest one before it. The named reason is on stderr; under `--json` a
  `rust_test_stub` schema `0.4` `state: refused` envelope is the stdout
  document. A failed read, analysis, or `--write` stays exit `2`.
- **Typed agent card refusal**: `ripr agent card` reached a deliberate named
  refusal of the default handoff — the seam id names no seam
  (`seam_not_found`: re-list seams or correct the id), the seam's grip class
  is policy-omitted (`policy_omitted`: check the `agent brief` policy config;
  re-listing cannot fix it), the witness analysis produced no witness
  (`witness_unavailable`: rerun the analysis or pick another seam), no
  admitted evidence names a portable workspace identity
  (`identity_unnameable`: retrieve the full packet instead), or the card
  builder, route gate, or budget refused to mint the card
  (`budget_overflow`: fall back to the canonical packet). With `--json` the
  versioned `agent_card_refusal` envelope (`schema_version` `0.1`) renders
  on stdout with the typed `error.kind`, `seam_id`, verbatim `message`, and
  `remedy_route`, and the prose rendering stays on stderr. Without `--json`
  stdout stays empty and stderr carries the prose rendering only. The kinds are
  closed and pinned by `cargo xtask check-output-contracts`
  (RIPR-SPEC-0202). Operational failures of the command (an unreadable
  config, a failed git probe, a detail-source serialization failure) still
  exit `2`.
- **Task-first repair façade refusal**: `ripr repair` refused a start
  without writing a success document. The named causes are: an explicit
  item that names no seam, a `ripr check` finding ID, or a gap ID that
  names several seams; no repair-eligible seam under the selected root
  (honest setup or limitation prose, including when no seams are
  visible); several eligible seams, or a start the selector will not
  bind, so no item is selected (the canonical next-action block and a
  retry command); and a card render that fails after the before phase
  already published an attempt (#7032). A start whose seam has no test
  file ripr can edit safely, or several inline test modules and no way to
  choose one, is the same refusal: exit `3`, not `2` (#11584). In every case stdout stays
  empty and stderr names the cause; with `--json`, stdout instead carries
  one `cli_refusal` document (`code: repair_not_started`, `next` is
  `ripr pilot`) and an operational failure is `repair_failed` (exit `2`).
  The post-publication card path is a
  deliberate asymmetry with `ripr agent card`: even an operational card
  failure is remapped to exit `3` because the attempt exists, and
  recovery inspects or continues that same attempt
  (`ripr status --attempt` / `ripr continue --attempt`) rather than
  implying no start happened. A truncated seam inventory still exits
  `2`, because the command could not select from a complete list.
- **Task-first continue façade refusal**: `ripr continue` refused to
  claim completion. The named causes are: an attempt that ended without
  a receipt (`stale`, `failed`, or `incomparable`); an attempt whose
  finished state a compliant receipt does not confirm (unissued,
  unavailable, or compatibility-only); no current attempt under the
  selected root; and several current attempts, so none is selected (a
  bounded list and a retry command). Stdout stays empty; stderr carries
  the cause and, for an ended attempt, the status facts. With `--json`,
  stdout carries one `cli_refusal` document instead (`code:
  continue_declined`, `next` is `ripr status`; an operational failure is
  `continue_failed`, exit `2`). A continue that
  has selected a current attempt then follows the same
  `ripr agent repair --phase after` refusals above, whose documents
  (`repair_after_refusal`, `repair_after_failure`, each carrying
  `attempt_id`) are what `continue --json` prints.

- **Typed reason codes for `repair` and `continue` (#11597)**: beside the
  unchanged prose, the `ripr repair` / `ripr continue` refusals listed in the
  table below (and the matching `ripr agent repair` phases at the sites that
  assign a code) write one stderr line, after the plain-words prose (#12193),
  `ripr: refusal code=<code> disposition=<disposition>`, so a script
  switches on the state instead of parsing prose. `disposition` is
  `retryable` (the same attempt proceeds after the stated action),
  `terminal` (the attempt is over; start a fresh `ripr repair`),
  `needs_selection` (name one of several), or `not_started` (nothing
  started). The codes are closed. Operational errors (a store or argument error on a shared
  `agent repair` path) exit `2` with prose only and no code line. Some
  existing typed refusals still exit `3` without a code line (for example
  `agent repair --phase before` on an unknown or policy-withheld seam). The
  table is the covered set, not a claim over every error path:

  | code | exit | disposition | state |
  |---|---|---|---|
  | `item_names_no_seam` | 3 | `not_started` | `repair <item>` names no current seam |
  | `item_names_several_seams` | 3 | `needs_selection` | the item names several seams |
  | `seam_not_repair_eligible` | 3 | `not_started` | the one seam named is not repair-eligible |
  | `no_eligible_seam` | 3 | `not_started` | bare `repair`: no seam is eligible |
  | `several_eligible_seams` | 3 | `needs_selection` | bare `repair`: choose from the printed list |
  | `repair_needs_item` | 3 | `needs_selection` | bare `repair` binds no seam by itself |
  | `start_refused_uncommitted_target` | 3 | `not_started` | the focused test file has uncommitted changes |
  | `no_test_route` | 2 | `not_started` | no test file or module ripr can safely edit |
  | `awaiting_edit` | 3 | `retryable` | `continue`: edit the focused test, then rerun |
  | `retry_budget_exhausted` | 3 | `terminal` | `continue`: the retry budget is spent |
  | `no_current_attempt` | 3 | `not_started` | `continue`: no attempt to continue |
  | `ambiguous_attempts` | 3 | `needs_selection` | `continue`: pass `--attempt` |
  | `attempt_ended_without_receipt` | 3 | `terminal` | the attempt failed, went stale, or is incomparable |
  | `attempt_completion_unconfirmed` | 3 | `terminal` | finished, but no compliant receipt confirms it |
  | `edit_cage_violation` | 2 | `terminal` | the edit cage rejected the edit |
  | `attempt_stale` | 2 | `terminal` | the cage rejected the edit and HEAD had moved after the repair started |
  | `attempt_incomparable` | 2 | `terminal` | the cage could not compare the files |

  A repeated `ripr continue` after a confirmed-complete attempt reports
  that attempt's already-complete status and exits `0`: with `--attempt`,
  or without it when the newest attempt (the only one with the latest
  creation time) is the confirmed-complete one and no prepared or
  trust-bound attempt remains. A cage violation is exit `2` and terminal;
  a missing focused edit is exit `3` and retryable.

These are findings- and policy-driven exits, not operational failures; a
monitoring system should page on `2`, not on `3`.

Some refusals still exit `2` because the command could not do what was asked:
`ripr receipt check` when the receipt is orphaned or its gap does not match
the ledger (the verdict is in the `--json` document), when a named `--ledger`
cannot be read, or when `--gap` names a different gap than the receipt;
`ripr agent receipt` when the attempt is not receipt-ready; `ripr agent
repair --phase verify` without its explicit authorization or on a moved
tree; and a repair `--phase after` whose edit cage recorded a violation.

## `ripr doctor` exit codes

`ripr doctor` uses the same contract: `0` when all checks pass, `3` when
the diagnosis completed and any check failed (including missing language
runtimes that are enabled in the effective configuration), and `2` only
when the run could not complete — a usage error (no check ran) or an
operational failure before a report exists (#5102). Both the human and
`--json` surfaces use this mapping; the report is written before exit `3`.

The `Cargo.toml` check applies only when Rust is in scope for the root:
Rust is enabled and either a Cargo.toml is found at the selected root, an
ancestor Cargo project bounded by git, or nested in this tree; Rust markers
(`.rs` files) are detected at the selected root; or no other language is
detected or enabled. A Python-only or TypeScript-only root with no nested
or ancestor Cargo.toml reports `cargo_toml` as `skipped` with the reason.
The analysis profile also skips `cargo` and `rustc`; `--profile source-build`
still probes those tools and can exit `3` if they are missing.
A Cargo.toml at the selected root, an
ancestor Cargo project bounded by git, or a nested Cargo.toml in this tree
(for example `rust/Cargo.toml` at a polyglot git root that also has
`package.json`) passes `cargo_toml`.
A root with Rust sources but no Cargo.toml anywhere in that bounded search,
and an empty root under the Rust-only default, still fail on a missing
manifest.

The toolchain checks depend on the profile. Under the default `analysis`
profile, a missing `cargo` or `rustc`, or a workspace `rustc` older than
RIPR's build MSRV, is reported as `advisory` and does not change the exit
code: the installed binary can still analyze the workspace. A missing `cargo`
still withholds evidence that reads `cargo metadata`. Under
`--profile source-build`, the same conditions are failures and exit `3`. A
missing cargo or rustc names an install route on the failing check; an older
rustc still names `rustup update stable`. Enabled language runtimes stay
visible but do not decide that profile's exit.

## CI integration

The GitHub Actions workflow that `ripr init --ci github` generates runs its
whole analysis in one step, and the step does not swallow its exit code:

```yaml
      - name: Run RIPR
        run: ripr reports ci-packet --root .
```

`ripr reports ci-packet` runs the pilot, the pull request diff capture, the
PR guidance, the comment plan, the SARIF and badge renders, the gate, the
ledgers, start-here, the report index, and the changed-line annotations,
each as a log group named after the step it replaced ([docs/CI.md](CI.md)
keeps the full recipe). Advisory steps log a failure and continue. The diff
capture and the gate evaluation are required, so a failed diff capture,
gate evaluation, or blocking-mode producer makes the command exit nonzero
after the rest of the packet is written: the step fails instead of passing
on an incomplete analysis, and the later `always()` steps still upload the
artifacts and the step summary.

The exit code reaches the merge gate through `RIPR_GATE_MODE`, not through
a check step. The generated job carries
`continue-on-error: ${{ vars.RIPR_GATE_MODE == '' || vars.RIPR_GATE_MODE == 'visible-only' }}`:
empty (the default, advisory) or `visible-only` never fails the job;
`acknowledgeable`, `baseline-check`, or `calibrated-gate` fails the job on
a nonzero step exit. `ripr check` is advisory here as everywhere: it exits
`0` whether or not it found gaps. When `RIPR_GATE_MODE` is set, the packet
evaluates the gate, and `ripr gate evaluate` exits `3` when it blocks and
`2` on a `config_error`; those completed decisions still write
`gate-decision.{json,md}`. A `--root` that is missing or not a directory is
argv-level exit `2`, names the root, and writes no report. `ripr gate evaluate`
treats an incomplete, partial, or unsupported
analysis outcome as a `config_error` in every mode, so a gate never passes
on a partial denominator.

## Standalone Unix check interruption (#7618)

For ordinary single-output `ripr check` with live Git or a resolved regular
`--diff` file, first SIGINT requests existing cooperative cancellation during
analysis. The existing child owner settles/reaps the managed exporter before
returning where supported. Exit 130 and stderr explain how to rerun; cancellation
is not a partial successful report or fail-soft producer degradation. Confirmed
settlement discards only the current unpublished packet. Cleanup blockers or
unconfirmed settlement retain named staging; prior packets and user data survive.
The next invocation uses fresh staging and never consumes that retained partial.

Stdin (`--diff -`), FIFO/nonregular/unresolved inputs, `--output-dir`,
`--write-artifact`, and `--gap-ledger` retain native SIGINT. Their blocking I/O or
separate publication paths are outside cooperative single-output qualification.
After analysis/rendering, stdout publication switches to native interruption;
streamed bytes cannot be retracted. A second analysis SIGINT also permits native
termination if I/O or cleanup stalls. Forced termination does not establish
reaping or staging cleanup.

Existing library `ripr::cli::run` preserves caller-owned signal policy. The binary
opts into `run_with_interrupt_handling`, a once-per-process standalone entry whose
signal actions last until process exit. `CommandError::Interrupted(String)` adds
an exhaustive-match arm; use it or `message()`/`exit_code()`. This recorded but
unendorsed API migration is for the upcoming 0.11.0 minor, not a 0.11.x patch.
Native execution evidence is Linux x86_64 with explicitly trusted single-process
fixture exporters. macOS/Windows runtime, arbitrary descendants, hostile path
replacement and real perllsp compatibility remain unqualified.

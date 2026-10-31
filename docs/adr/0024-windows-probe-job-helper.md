# ADR 0024: Windows probe job helper is a small native binary built on the safe process-wrap Job Object

- Status: Proposed (safe design: no `unsafe` and no exception to `unsafe_code = "forbid"` is requested)
- Date: 2026-10-25
- Related: #9631, #9526, #5891, #9625, #9654, #3803, #9564

## Context

On Windows the VS Code extension does not start `ripr.exe` directly to probe it.
`spawnProbeProcess` (`editors/vscode/src/lspCompatibility.ts`) starts
`powershell.exe -EncodedCommand` with `WINDOWS_PROBE_JOB_WRAPPER`, which
`Add-Type`-compiles a C# helper (`RiprProbeJob`) that runs the probe inside a
Job object. The Job gives kill-on-close descendant cleanup and typed exit-code
and start-failure markers (#5891), so a probe that hangs or leaves children
cannot outlive the extension.

Every resolution pays for the wrapper. In the ordered trials on Windows
runners (aggregate timings, see Consequences), empty PowerShell starts ranged
from 1.7 to 7 s and the full wrapper reached 14 s, while `ripr --version`
itself takes 10 to 35 ms. The editor journey hung at the former 10 s budget (#9526). #9625 raised the Windows budget to 30 s and
#9654 added a timing diagnostic; both bound the damage, neither removes the
cost.

The workspace sets `unsafe_code = "forbid"`, and `docs/ENGINEERING.md` says to
keep it. The repository documents no exception process for it: no ADR, policy
file or `check-*` gate admits an `unsafe` island, and the root manifest's
unsafe-hygiene lints are kept only for a hypothetical future one. The
containment already shipped for the product's own bounded subprocesses (#3803,
`crates/ripr/src/process_owner.rs`) takes the other route: it uses
`process-wrap` (reviewed safe wrapper, already in `Cargo.lock` and
`crates/ripr/Cargo.toml`) for assign-before-execution Job Object ownership, and
its manifest comment records that `windows` and `windows-sys` are `unsafe fn`
surfaces excluded by the forbid. This ADR follows the same route, so the helper
needs no `unsafe` and no exception.

## Alternatives Considered

1. **Spawn `ripr.exe` directly and kill the tree with `taskkill /T /F`.** No new
   code. It cannot give kill-on-close if the extension host dies, and it loses
   the typed start-failure and exit-code markers the Job-contract tests pin.
2. **Precompile the C# helper once and `Add-Type -Path` it.** Removes the
   compile but retains PowerShell startup; that startup's contribution to the
   whole-wrapper cost remains unisolated.
3. **Small native helper binary using the Win32 Job API directly through
   `windows-sys`.** Startup cost is one process start and the Job contract is
   kept, but every call is `unsafe`. That needs the repository's first
   exception to the workspace forbid and, because Cargo cannot override an
   inherited `forbid` per crate, a helper that opts out of workspace lint
   inheritance and duplicates the lint tables. Rejected: option 5 gives the
   same behavior without it.
4. **A Node native addon.** Same `unsafe` need plus a per-ABI build matrix;
   rejected as heavier than option 5.
5. **Small native helper binary using `process-wrap` (safe API) for the Job
   Object.** Startup cost is one process start. Keeps the Job contract. The
   `unsafe` lives inside the reviewed dependency, as it does for `winsafe` and
   for `process_owner`, so the helper keeps `unsafe_code = "forbid"` and
   inherits the workspace lints unchanged.
6. **Same helper, but kill-on-close from `win32job` instead of the `tokio`
   `JobObject`.** `win32job` 2.0.3 is already a dependency (#9564) and
   `process_owner.rs` already uses it (`ExtendedLimitInfo`,
   `Job::create_with_limit_info`) to put the owner process in a kill-on-close
   Job, with its `unsafe` inside the crate. The helper could put itself in such
   a Job and run the probe with the plain std `process-wrap` Job, which would
   not need the `tokio1` feature or the `tokio` `process` feature. Open: it
   gives one Job for the helper rather than the probe's own Job, so whether
   descendant cleanup and the typed exit and start-failure markers still meet
   the Job-contract tests is unshown. It is the first thing to try in the
   helper slice, ahead of the `tokio` route in the Decision, because it adds no
   dependency feature.

## Decision

Proposed, not accepted. Adopt option 5, narrowly.

- A small helper that creates the Job Object through `process-wrap` and
  contains no `unsafe`. It is either an unpublished Windows-only crate outside
  the `ripr` package or a mode of the existing binary; that choice stays open
  (see "Not decided here"), and in both cases the published package, binary
  and library stay as ADR 0001 describes. Workspace lint inheritance, including
  `unsafe_code = "forbid"` and the unsafe-hygiene lints, is unchanged.
- Kill-on-close is part of the contract (#5891): a probe must not outlive an
  extension host that died. The `process_owner` configuration uses
  `process_wrap::std::JobObject`, whose Job is created without
  `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` (process-wrap 10.0.1,
  `src/std/job_object.rs` passes `kill_on_drop = false`; `process_owner` records
  the same). That is sufficient for its Drop-driven cleanup but not for a
  helper that can itself be killed. The helper therefore uses process-wrap's
  `tokio` `JobObject` together with `KillOnDrop`, which sets the flag through
  the same safe API. That adds the `tokio1` feature of `process-wrap` and a
  `tokio` `process` feature to the helper's dependency set. If that, or the
  flag's effect, cannot be shown under the Job-contract tests, the design
  returns to the architect: the fallback is option 3 with its `unsafe`
  exception, not a silent loss of kill-on-close. Alternative 6 (`win32job`,
  no new feature) is tried first; the `tokio` route is the next step if it
  cannot satisfy the Job-contract tests, and option 3 remains the last.
- Shipped only in the win32 VSIX. The PowerShell wrapper remains as the
  fallback when the helper is missing or fails to start, so a packaging mistake
  degrades to today's behavior rather than to no probe.
- The existing Job-contract tests in `lsp_compatibility.test.ts` gate the
  switch: the helper must satisfy them unchanged. The qualification lane must
  bind each probe invocation to the selected helper artifact, and a run that
  fell back to the wrapper is not native proof (see Consequences).

## Consequences

- The cold-probe cost is expected to drop from seconds to well under one
  second, but that is a hypothesis until measured. The numbers quoted in this
  ADR (empty PowerShell 1.7 to 7 s, wrapper up to 14 s, `--version` 10 to
  35 ms) are ordered aggregate observations from the current timing step in
  `.github/workflows/windows-editor-hang-hunt.yml`, not validated per-probe
  measurements: that step drains target output and stderr, ignores
  `close(code, signal)`, has no per-probe deadline, uses wall-clock duration,
  and runs the direct-target and empty-PowerShell trials before the wrapped
  ones, so an unsuccessful target can produce the same timing row as success
  and first-in-series is not an isolated cold start (the instrument finding on
  #9631).
- Instrument repair is therefore a prerequisite to any latency criterion. The
  "cold probe under 1 s" threshold applies only to a validated timing-only
  probe with monotonic timing, exact target and helper identities, verified
  target output and exit or start-failure markers, a per-probe deadline with an
  owned cleanup outcome, and honest cold, cache and order labels.
- Latency acceptance stays distinct from native Job-containment proof. Kill-on-
  close and descendant cleanup are proved by the existing Job-contract tests in
  `lsp_compatibility.test.ts`, which the helper must satisfy unchanged; reusing
  the current timing step supplies neither proof.
- Native containment proof is bound to the helper, and the fallback is
  qualified separately. The future lane must record the executed helper's path
  and content identity for every probe and must not credit a fallback run as
  native proof. Validation handoff:
  - Positive: the packaged helper handles `--version` and `lsp --stdio`, with
    its executed identity retained, including the Windows `.cmd` and
    `useShell=true` fixtures.
  - Negative: with the helper removed, and separately with it made unstartable,
    the product falls back to today's wrapper behavior, while native
    qualification fails or reports not established.
  - The target-start-failure and nonzero-target controls stay typed target
    outcomes, distinct from a helper launch failure.
- No `unsafe` exception. The dependency set changes only by enabling more
  features of `process-wrap` (already approved by #3803) and `tokio`; the
  dependency allowlist and `cargo-deny` checks are the review point for that
  slice. No architect acceptance of an exception is needed, though this ADR
  records a product decision (a second native binary or mode) and is
  merged as Proposed until the first slice lands.
- Per-platform VSIX packaging and a Windows-qualified build and test lane for
  the helper.
- The 30 s Windows probe budget stays until the measurement passes, then can be
  tightened in a separate change.

## Not decided here

Crate location and name (or a hidden mode of `ripr`, which would avoid VSIX
packaging of a second binary but adds a command-catalog entry), the exact
`process-wrap` and `tokio` feature list, and whether the helper also serves the
CLI. Those are implementation choices for the slices
after acceptance: timing-instrument repair, helper plus tests, VSIX packaging,
the `spawnProbeProcess` switch, then the validated timing check.

# ADR 0024: Windows probe job helper is a small native binary, not a PowerShell compile

- Status: Proposed (acceptance is the architect's call; no FFI crate or `unsafe` is added until it is accepted)
- Date: 2026-10-25
- Related: #9631, #9526, #5891, #9625, #9654

## Context

On Windows the VS Code extension does not start `ripr.exe` directly to probe it.
`spawnProbeProcess` (`editors/vscode/src/lspCompatibility.ts`) starts
`powershell.exe -EncodedCommand` with `WINDOWS_PROBE_JOB_WRAPPER`, which
`Add-Type`-compiles a C# helper (`RiprProbeJob`) that runs the probe inside a
Job object. The Job gives kill-on-close descendant cleanup and typed exit-code
and start-failure markers (#5891), so a probe that hangs or leaves children
cannot outlive the extension.

The cost is paid on every cold resolution. Measured on Windows runners, an empty
PowerShell start alone took 1.7 to 7 s cold and the full wrapper up to 14 s,
while `ripr --version` itself takes 10 to 35 ms. The editor journey hung
at the former 10 s budget (#9526). #9625 raised the Windows budget to 30 s and
#9654 added a timing diagnostic; both bound the damage, neither removes the
cost.

The workspace sets `unsafe_code = "forbid"`. A native Job-object helper needs
Win32 calls, so avoiding the PowerShell compile cannot be done without a
documented exception.

## Alternatives Considered

1. **Spawn `ripr.exe` directly and kill the tree with `taskkill /T /F`.** No new
   code. It cannot give kill-on-close if the extension host dies, and it loses
   the typed start-failure and exit-code markers the Job-contract tests pin.
2. **Precompile the C# helper once and `Add-Type -Path` it.** Removes the
   compile but not the PowerShell start, which is most of the measured cost.
3. **Small native helper binary using the Win32 Job API.** Cold cost is a
   process start. Keeps the Job contract. Needs `unsafe` for the FFI calls.
4. **A Node native addon.** Same `unsafe` need plus a per-ABI build matrix;
   rejected as heavier than option 3.

## Decision

Proposed, not accepted.

Adopt option 3 from the alternatives above, narrowly.

- An unpublished helper crate outside the `ripr` package, so the published
  package, binary and library stay as ADR 0001 describes. Windows-only target.
- One dependency, `windows-sys`, with only the Job-object and process features
  it needs. All `unsafe` lives in one small module with a safety comment per
  call; the helper crate carries a scoped exception to the workspace
  `unsafe_code` lint, and nothing else does.
- Shipped only in the win32 VSIX. The PowerShell wrapper remains as the
  fallback when the helper is missing or fails to start, so a packaging mistake
  degrades to today's behavior rather than to no probe.
- The existing Job-contract tests in `lsp_compatibility.test.ts` gate the
  switch: the helper must satisfy them unchanged.

## Consequences

- Cold extension probe should drop from seconds to well under one second.
  Acceptance is measured, not asserted: re-run the #9654 timing step on a
  Windows runner and require a cold probe under 1 s.
- A new dependency and the repository's first `unsafe` exception, which is the
  reason this is an ADR and not a PR.
- Per-platform VSIX packaging and a Windows-qualified build and test lane for
  the helper.
- The 30 s Windows probe budget stays until the measurement passes, then can be
  tightened in a separate change.

## Not decided here

Crate location and name, the exact `windows-sys` feature list, and whether the
helper also serves the CLI. Those are implementation choices for the slices
after acceptance: helper plus tests, VSIX packaging, the `spawnProbeProcess`
switch, then the timing check.

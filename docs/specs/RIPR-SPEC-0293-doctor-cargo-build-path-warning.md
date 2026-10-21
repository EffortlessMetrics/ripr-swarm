# RIPR-SPEC-0293: Doctor suppresses the PATH-mismatch warning for Cargo build outputs

Status: proposed

Owner: product / output

Created: 2026-10-21

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #9097 (PATH-mismatch warning fires for every contributor running a Cargo-built binary)

Linked PRs:

- None yet

Support-tier impact:

- No tier change. `docs/status/SUPPORT_TIERS.md` remains unchanged; this
  narrows one advisory doctor warning and adds no language support,
  platform, or packaging claim.

Policy impact:

- Register this spec in `policy/doc-artifacts.toml`.
- No new dependencies, crates, binaries, workflow permissions, or
  network/process surfaces.

## Problem

Running a Cargo-built binary (`target/debug/ripr.exe doctor`) with an
installed `ripr` on PATH printed the PATH-mismatch warning on every run:

```text
- warning: `ripr` on PATH is C:/Users/.../.cargo/bin/ripr.exe, not the running binary .../target/debug/ripr.exe; this report describes the running binary, and unless that PATH entry is a shim that launches it, commands that run `ripr` get a different binary
```

For contributors, the running binary being a build output is the normal,
intended state. The warning trains them to ignore doctor warnings.
`crates/ripr/src/output/doctor_binary.rs` fired the mismatch warning when
the canonical running binary differs from the PATH `ripr`; the same
function already computed `executable_is_cargo_build_output` (deps/ +
.fingerprint/ ancestry) for identity reporting, but did not use it to gate
the warning.

## Behavior

- When the running binary is a Cargo build output
  (`executable_is_cargo_build_output`, computed once in
  `evaluate_binary_identity` and reused for the JSON identity field),
  doctor does not emit the "`ripr` on PATH is ..., not the running
  binary" warning. Identity fields and JSON remain truthful about both
  paths: `executable`, `path_ripr`, and
  `path_ripr_is_running_executable` are unchanged.
- When the running binary is an installed binary that differs from PATH,
  the warning still prints (negative control:
  `different_installed_ripr_on_path_warns_without_build_output_claim`).
- The separate "PATH `ripr` is Cargo build output" warning is unchanged;
  a workspace build first on PATH is still named.
- No schema or exit-code change; doctor warnings remain advisory.

## Required Evidence

- Red-then-green regression coverage in `doctor_binary.rs` tests:
  `cargo_build_output_running_binary_suppresses_path_mismatch_warning`
  failed before the gate (warning present with
  `executable_is_cargo_build_output: true`) and passes after, asserting
  truthful identity (`path_ripr_is_running_executable == Some(false)`)
  with empty warnings.
- The pre-existing negative control
  `different_installed_ripr_on_path_warns_without_build_output_claim`
  passes unchanged.

## Required guards

- The suppression keys only off the running binary being a Cargo build
  output; identity reporting never claims the PATH binary is the running
  one when it is not.
- A genuinely different installed `ripr` on PATH must still warn.

## Acceptance Examples

- Accept: `evaluate_binary_identity` with a running executable under
  `target/<profile>/` (deps/ and .fingerprint/ present) and a different
  installed `ripr` on PATH yields `executable_is_cargo_build_output ==
  true`, `path_ripr_is_running_executable == Some(false)`, and no
  warnings.
- Accept: the same shape with a non-build-output running binary yields
  exactly the "not the running binary" warning.
- Reject: any schema, exit-code, or identity-field change; suppressing
  the warning for a non-Cargo running binary.

## Metrics

No new metric; the behavior is pinned by the two unit tests above.

## Test Mapping

`crates/ripr/src/output/doctor_binary.rs` tests
`cargo_build_output_running_binary_suppresses_path_mismatch_warning` and
`different_installed_ripr_on_path_warns_without_build_output_claim`.

## Non-Goals

- No change to the "PATH `ripr` is Cargo build output" warning, PATH
  search order, or shim detection.
- No schema, JSON field, or exit-code change.

## Implementation Mapping

- `crates/ripr/src/output/doctor_binary.rs` — `evaluate_binary_identity`
  computes `executable_is_cargo_build_output` once, gates the
  PATH-mismatch warning on it, and reuses it for the identity field.

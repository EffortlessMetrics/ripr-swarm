# ADR 0026: One user-facing package, lockstep internal support crates

- Status: Accepted
- Date: 2026-11-04
- Related: #12726, #8382, #12719
- Supersedes: the packaging clause of ADR 0001 (its internal module seams remain the default inside each crate)

## Context

ADR 0001 kept one published package and rejected crate splits "until a real
external contract exists", because no proven benefit justified the versioning
cost. That condition is now met by measurement, not preference.

On current `main` (`15579a490785e322f809082a964ef9e7002fa3fd`, Linux,
rustc 1.95.0, `cargo rustc -p ripr --lib --profile test`, jobs=1), the single
`ripr` libtest rustc reaches an attributed OS high-water of **13,525,372,928
bytes (13.525 GB)**. The #12726 target is a largest single rustc of at most
8,000,000,000 bytes, so 5.53 GB must leave the largest compiler unit. Phase
evidence is diffuse (5.0 GB after type-check, 11.1 GB after monomorphization
collection), so scheduling, codegen-unit, and hotspot changes cannot close the
gap; only moving production/test material into separate rustc units can.
Full receipt: #12726, 2026-11-04.

Cargo packaging constrains the shape: a normal dependency's local `path` is
ignored when packaging, so a `publish = false` support crate cannot sit under
the crates.io `ripr` package. A real production split therefore means either
publishing support crates, or not splitting production code at all.

## Decision

`ripr` remains the single **user-facing** package: the named CLI, library
facade, documentation surface, and the package users install
(`cargo install ripr` keeps working).

Internal support crates may be extracted when compiler-unit evidence requires
it, under these rules:

- Support crates are implementation packages, published to crates.io in
  lockstep with `ripr`: same version, exact (`=`) version requirements from
  their consumers.
- The public `ripr` API does not change. Support-crate types are re-exported
  at their established `ripr::` paths; support-crate APIs are not a public
  contract and may change in any release.
- Release automation publishes in dependency order and records per-package
  receipts. Workspace publishing is non-atomic (Cargo ≥1.90), so the release
  procedure must state the partial-publication recovery explicitly.
- Extraction order is measured, not architectural: contract types first, then
  the TypeScript adapter, then Python, then LSP/MCP, then central analysis
  only if the target is still missed (#8382 owns the ranked sequence). Stop
  when the largest-rustc target and aggregate-cost acceptance are met.
- Dev-only test crates remain `publish = false`; they are not support crates
  and carry no registry consequence.

## Consequences

- `cargo install ripr` from the registry resolves `ripr` plus its lockstep
  support packages; source-install users see no workflow change.
- The registry gains a small number of implementation packages whose names
  are not a public API commitment.
- A split that does not move the measured largest rustc is not justified by
  this ADR; each extraction slice requires the matched before/after
  attributed measurement from #12726.
- ADR 0001's module seams continue to govern organization inside `ripr` and
  inside each support crate.

## Alternatives Considered

- **Literally one crates.io package, generated recombined source package.**
  Rejected: it restores the monolithic rustc unit for source installers and
  creates a second compiled representation, so repository CI would improve
  while the cold source-install problem remains.
- **Test-only extraction (public-boundary integration tests).** Rejected as
  the remedy: integration tests are already separate rustc units, and the
  honest in-crate public cohort is ~10 tests against a 5.53 GB gap. It remains
  a relocation-mechanics pilot only.
- **Keep ADR 0001 unchanged and accept the 13.5 GB unit.** Rejected: the
  hosted 16 GB shards miss the #12719 3 GB headroom floor on every observed
  run, and the single rustc is the cause.

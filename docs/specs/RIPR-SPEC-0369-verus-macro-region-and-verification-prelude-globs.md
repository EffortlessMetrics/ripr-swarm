# RIPR-SPEC-0369: A changed line inside `verus!` is a named static limit, and verification preludes that export no assertion macro do not refuse assertion credit

Status: proposed

Owner: product / analysis

Created: 2026-10-31

Linked proposal:

- None yet

Linked ADRs:

- [ADR 0021](../adr/0021-static-and-executed-assurance-axes.md)

Linked plan:

- #10745 (program: change-to-guard workflows across formal verification and modern test frameworks)

Linked issues:

- #10747 (Verus dialect adapter; this spec is the fail-closed floor beneath it, not the adapter)

Linked PRs:

- #11703

Support-tier impact:

- No tier change. No new `static_limit_kind` or `stop_reasons` value: the
  existing `unsupported_syntax` and `proc_macro_opaque` are used. Claim
  boundaries stay governed by [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register in `policy/doc-artifacts.d/RIPR-SPEC-0369.toml`, add
  `.ripr/traceability.d/verus-macro-region-prelude-globs.toml`, add a
  `docs/spec-index.d/RIPR-SPEC-0369.md` row, and give
  `fixtures/rust-verdict-corpus/spec-scopes/RIPR-SPEC-0369.toml` its scope.

## Problem

Measured on ripr 0.11.0:

1. Code inside `verus! { .. }` is a macro token tree of Verus syntax. ripr
   still seeded expression probes from changed lines there but never resolved
   the enclosing function, so an exec-code change reported
   `propagation_unknown` ("no test is seen calling changed owner") with no
   limitation, and an edit to `requires`, `ensures`, `assert(..) by`,
   `proof { .. }` or a `spec fn` became a `weakly_exposed` or
   `infection_unknown` probe. Spec and proof text is erased at runtime; no
   `#[test]` can notice it, so those probes were false actionable signals. A
   real Verus commit whose change sat inside `verus!` reported only "no
   changed line is behavior ripr checks".
2. `use creusot_std::prelude::*;`, `use vstd::prelude::*;` and
   `use prusti_contracts::*;` are treated as foreign globs that might shadow
   `assert_eq!`, so assertion credit is refused for every file that follows
   each framework's recommended import.

## Behavior

### A `verus!` region is a named static limit

A changed line (added or removed) whose whole trimmed content lies inside the
token tree of a `verus!` invocation (`()`, `[]` or `{}` delimiters, comments
and strings masked, an unclosed tree running to the end of the file) is Verus
syntax, not Rust. The probe seeder emits one `static_unknown` probe per region
and diff side, keyed by region start line, and no expression-family probe; a
removed line replaced by an added one is paired into the added-side probe as
its `before` text, and only an unpaired removed line seeds a removed-side probe; a
removed-side probe resolves its region in the reconstructed old source at the
deleted line's old-side number, never at its new-file coordinate. A
line that also holds code outside the region keeps normal seeding. The
`verus` token must begin an identifier, so `my_verus!` and prose do not
match. A path-qualified invocation such as `vstd::prelude::verus! { .. }` is
the same region. Attribute-style Verus (`#[verus_spec(..)]`, `#[verus_verify]`) is plain
Rust syntax and is out of this rule.

The finding for such a probe is non-actionable and uncredited:

- class `static_unknown`; never `exposed`, `weakly_exposed`,
  `reachable_unrevealed` or `no_static_path`; no related test is credited;
- `static_limit_kind` is `unsupported_syntax` and the stop reason is
  `proc_macro_opaque`;
- evidence names the limitation `verus_macro_region_unsupported`, the region's
  line extent as the last established edge, the unresolved edge (Verus syntax,
  enclosing function, mode and test reach), the analyzer route
  `analysis/verus-macro-region` and a non-claim;
- the recommended next step says the change is judged by the Verus verifier
  for spec and proof edits and requests no test.

ripr does not run the verifier and does not claim a spec or proof edit is
adequate or inadequate. A diff whose changed lines all sit inside `verus!` is
no longer reported as "no changed line is behavior ripr checks".

### Verification-prelude globs do not refuse assertion credit

A glob import is not a foreign glob for the trusted assertion macros when it
is exactly one of:

| Glob path | Audited version |
| --- | --- |
| `creusot_std::prelude::*` | creusot-std 0.14.0 |
| `vstd::prelude::*` | vstd 0.0.0-2026-10-25-0158 |
| `prusti_contracts::*` | prusti-contracts 0.3.3 |

and the crate name resolves to its plain registry package for the importing
file's manifest (the existing drop-in manifest authority, RIPR-SPEC-0197: no
`package`, `path`, `git`, `registry`, `[patch]` or `.cargo` substitution) and
is not shadowed by a workspace package, module or `extern crate` rename.

Evidence for "exports no assertion macro", read from the published crate
sources (the clones are not vendored):

- `creusot_std::prelude` re-exports `base_prelude` (ghost, invariant, model
  and resolve types, the derive-shadowing `Clone`, `PartialEq`, `Default`,
  `vec!`, anonymous extension traits) and `macros`, a closed list of
  attribute and function-like proc macros (`requires`, `ensures`, `snapshot`,
  `ghost`, `proof_assert`, `logic`, `trusted`, ...). None is named
  `assert*` or `debug_assert*`.
- `vstd::prelude` re-exports `verus_builtin::*` (whose only exported macros
  are `with_triggers!` and `decreases_to!`), a closed list of
  `verus_builtin_macros` items, and vstd collection and view types. None is
  named `assert*`.
- `prusti_contracts` exports explicit proc macros plus the declarative
  `prusti_assert_eq!`, `prusti_assert_ne!`, `map!`, `seq!`, `set!`,
  `closure!` and `ghost!`. None is named `assert_eq`, `assert_ne` or any
  other trusted assertion.

The audit is of the listed versions only. The glob keeps assertion credit only
when the nearest `Cargo.lock` at or above the analysis root locks the crate
exactly once, from the crates.io registry, at an audited version (the table
above; the same lockfile reader as the audited proptest profile). An
unlocked crate, a missing or unreadable lockfile, a newer, older or unknown
version, a path, git or other-registry package, a workspace member of the same
name, or several locked versions of the crate refuse credit with the same named
macro-binding refusal as before this spec; the refusal text does not name the
version. A future release that exports a trusted assertion name is never
covered by this entry until it is re-read and added.

New-audit procedure: re-read the published crate's prelude export list at the
exact version (no macro named like a trusted assertion), add that exact version
string to `AUDITED_PRELUDE_VERSIONS` in `analysis/facts/drop_in.rs` and to the
table above, extend `verification_prelude_globs_credit_only_audited_locked_versions`,
and bump `CACHE_SCHEMA_VERSION`. Remove a version the same way if a re-read finds
an assertion-like export.

This grants no credit of its own: an assertion in a file
with such a glob is credited exactly as it would be without the glob, and a
local `macro_rules! assert_eq`, another foreign glob, a `#[macro_use]` or any
import of a trusted name in the same scope still refuses. `use
creusot_std::*;`, `use vstd::*;`, `use prusti_contracts::prelude::*;`, other
crate names (including the older `creusot_contracts`) and renamed or
substituted dependencies remain foreign globs.

## Non-Goals

- No Verus parser, `verus!` expansion or Kani/Verus fact producer (#10747).
- No verifier execution and no claim that a spec, proof or contract is
  adequate; no credit for a `requires`/`ensures` clause as a test oracle.
- No change to run-level `limitations[]` of the analysis outcome, which
  records incomplete-analysis state; the limitation here is finding-level.
- No new `static_limit_kind`, stop reason, schema field or verdict name.
- No claim about any crate version outside the audited table; the audit is of
  the listed versions only, not of the crate name or later releases.
- Creusot `#[logic]` items and `proof_assert!`/`ghost!` macros outside
  `verus!` are not covered by this spec.

## Required Evidence

- Unit tests for region extents (delimiters, comments, strings, mixed lines,
  look-alike names, unclosed tree) and for probe seeding on both diff sides.
- A finding-level test that the limited finding is `static_unknown`, carries
  the named limitation and the non-test next step, and is not credited.
- Parser-path tests that each allowlisted glob keeps assertion credit under a
  plain registry dependency and loses it under a `path`/`package`/patched
  dependency, a local `assert_eq` macro, a second foreign glob and the
  non-allowlisted spellings above.
- Version-matrix tests: an audited locked version credits; a newer, older or
  unknown version, a missing lockfile, a lockfile that omits the crate, several
  locked versions, a workspace member of the same name and a git source refuse.

## Acceptance Examples

1. An exec function body edited inside `verus! { .. }` beside an
   `assert_eq!`-based test reports one `static_unknown` finding naming
   `verus_macro_region_unsupported`, not `propagation_unknown`.
2. Editing `ensures r <= 100` to `ensures r <= 200` or a `spec fn` body
   inside `verus!` reports `static_unknown` with no recommended test; it is
   never `weakly_exposed` or `infection_unknown`.
3. A change to ordinary Rust beside the `verus!` block in the same file keeps
   its normal probe and verdict.
4. A runtime function edited in a file with `use prusti_contracts::*;` and a
   plain registry `prusti-contracts` dependency is credited `exposed` by an
   exact `assert_eq!`, as it would be without the glob.
5. The same file with `prusti-contracts = { path = ".." }`, or with a local
   `macro_rules! assert_eq`, still refuses assertion credit.
6. The file of example 4 with `Cargo.lock` locking prusti-contracts at a
   version outside the audited set (for example a newer release), or with no
   `Cargo.lock`, refuses assertion credit exactly as before this spec.

## Test Mapping

- `crates/ripr/src/analysis/probes/macro_definition.rs`: region extent tests.
- `crates/ripr/src/analysis/probes/diff.rs`: seeding tests.
- `crates/ripr/src/analysis/language/rust/probes.rs`: finding limit tests.
- `crates/ripr/src/analysis/syntax/owner_pin.rs` and the classify owner-pin
  tests: prelude-glob tests, including `verification_prelude_globs_credit_only_audited_locked_versions`.
- Corpus: `prusti-prelude-glob-boundary` in `fixtures/rust-verdict-corpus`
  (credited under the allowlisted glob, runtime-labeled). Verus and Creusot
  prelude cases need their toolchains to produce runtime truth and are not in
  the corpus; the unit tests above carry them.

## Implementation Mapping

- `crates/ripr/src/analysis/probes/macro_definition.rs`: `verus_regions`,
  folded into the `MacroMap` the probe seeder already consults for
  `macro_rules!` text.
- `crates/ripr/src/analysis/language/rust/probes.rs`:
  `apply_verus_macro_region_limit`, applied with the other probe/oracle limits.
- `crates/ripr/src/analysis/syntax/owner_pin.rs`:
  `ASSERTION_FREE_PRELUDE_GLOBS`, consulted in the macro-binding scan before a
  glob is recorded as foreign; `AUDITED_PRELUDE_VERSIONS` in
  `analysis/facts/drop_in.rs` with `locked_at_audited_version` in
  `analysis/workspace/assertion_dependency/registry.rs` (the lockfile reader
  shared with the proptest profile) supply the version gate.

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.

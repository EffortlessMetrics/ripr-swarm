# RIPR-SPEC-0442: An aliased owner import credits the owner only when the import names its module in the same compilation unit

Status: accepted

Owner: product / analysis

Created: 2026-11-13

Linked issues: #14508 (an aliased direct import of the owner is not credited,
and naive name matching over-credits a test crate's own module); #14016 (the
selective control for test-local rival imports).

Linked PRs: #14513

Support-tier impact: no tier change. The change decides which existing
`direct_owner_call` relation a test already-spelled call earns; it adds no
grip class, `static_limit_kind` value or exposure class. Claim boundaries
remain governed by [support tiers](../status/SUPPORT_TIERS.md).

Policy impact: register this spec in `policy/doc-artifacts.d/RIPR-SPEC-0442.toml`
and `.ripr/traceability.d/RIPR-SPEC-0442.toml`. No schema or JSON field
change; the judged repo-exposure fields stay `grip_class`, `evidence.reach`,
`related_tests[].relation_reason` and `observed_values`.

## Problem

A test that writes `use crate::pricing::discounted_total as dt;` and calls
`dt(100, 100)` reaches the owner, but no spec stated that the alias credits
it. The owner module path joined segments with `/` while the import used
`::`, so a nested owner stayed `activation_unknown`; and a test crate's own
`crate::pricing` resolved to `tests/pricing.rs` yet a bare module-name
comparison credited the library's `src/pricing.rs::owner`.

## Behavior

A direct import of the owner's function under an alias credits the owner as
`direct_owner_call` when both hold:

1. The import names the owner's module in the same compilation unit. A unit
   test may use `use crate::<module>::owner as dt;` (or `use crate::owner as
   dt;` for the crate root); an integration test uses the owner's package
   qualification, `use <package>::<module>::owner as dt;`, and the package
   must be the one the test's crate imports the owner's library by.
2. The call's spelling binds to that import at the call line: no nearer
   lexical `use`, local function, `let`, closure parameter,
   `for`/match-arm/`if let`/`while let` pattern or nested-`fn` parameter of
   the same spelling is in scope.

Both sides of the module comparison use one `/`-segment representation.
`src/pricing/mod.rs` names the `pricing` module; a crate root has the empty
path. A nested `mod x;` resolves as rustc does, following `#[path]`.

The alias does not credit the owner when:

- A `crate::` import appears in an integration test (`tests/...`), which
  names the test crate's own modules, or in a binary target, which names the
  binary.
- The import names another package, a sibling package, or another
  compilation unit than the owner's.
- In a binary target root, the alias chain's first segment is declared with a
  non-default `#[path]` or is not declared at all.
- A local function of the alias's name is declared in the import's own
  module or a nested module (at least as deep as the import) and in the
  call's module chain, or a lexical binding of the name is in scope at the
  call. A declaration in an enclosing module is shadowed by the import, and a
  sibling inline module is a different name scope: neither withholds the
  alias, and the sibling does not itself receive the alias credit.

A test that also spells a genuine local call of the alias name keeps the
generic direct-call credit for that call; only the alias-shaped calls are
routed to the import. The rule decides `related_tests[].relation_reason` in
the seam-grip and repo-exposure paths and adds no reach edge by itself.

## Required Evidence

- An aliased unit-test call credits the owner with `direct_owner_call` and
  the observed argument; an integration test's genuine package-qualified
  alias credits the owner, while the same test crate's own `crate::` module
  does not.
- A nested owner (`src/pricing/inner.rs`) reached through `pricing/mod.rs` or
  `pricing.rs`, and a `mod.rs` owner, credit their `crate::` imports.
- A same-named local function in the import's module or a nested module, or
  a lexical binding, withholds the owner; an enclosing-module declaration is
  shadowed by the import and keeps credit; a later genuine alias call keeps
  credit. A sibling inline module's same-named function neither shadows the
  alias nor receives the alias credit.
- Only a bare `alias(..)` spelling binds to the import: a same-line
  `rival::alias(..)` on a lowercase module names that module's function and
  lends the owner no argument.
- A free alias of the owner's name lends no argument to a same-named
  associated owner; the associated owner keeps only its own calls' values.

## Non-Goals

- Grouped (`use a::{b as c}`), chained (`use a::b as c; use c::d;`) and renamed module aliases (`use a as b;`).
- The diff-mode `ripr check` relation path, which is a separate gap.
- Trait or impl alias credit; the alias must name a free function.
- In a lib+bin package whose binary root declares the module with the default
  path, the binary's `crate::<module>` compiles the same source file as the
  library owner, and the alias still credits it. Tracked in #14558.

## Acceptance Examples

1. `src/pricing.rs::owner`, `tests/control.rs` with its own `mod pricing;`
   and `use crate::pricing::owner as dt; assert_eq!(dt(100), 100);`: no credit.
2. `src/pricing/inner.rs::owner`, a unit test with
   `use crate::pricing::inner::owner as dt; assert_eq!(dt(100), 90);`:
   `direct_owner_call`, observed `["100"]`.
3. `src/lib.rs::owner` with a genuine `use crate::owner as dt;` beside
   `mod unrelated { pub fn dt(..) { .. } }`: the real owner is credited;
   `src/lib.rs::unrelated::dt` is not credited as a direct owner call.

## Test Mapping

- `crates/ripr/src/analysis/test_grip_evidence/tests.rs`: the `given_direct_owner_alias_when_*` and `given_integration_test_when_owner_is_imported_under_an_alias_*` cases, including the library-child, bin-only-child and binary-child crate-root controls.
- `crates/ripr/src/analysis/extract/shadow.rs::fact_authority_tests`: the `scoped_shadow_*` cases.

## Implementation Mapping

- `crates/ripr/src/analysis/test_grip_evidence/related_tests.rs`:
  `match_direct_owner_call`, `direct_owner_alias_module_matches`, `crate_relative_alias_shares_compilation_unit`, `crate_root_file_for_test`.
- `crates/ripr/src/analysis/test_grip_evidence/related_tests/context.rs`:
  `direct_owner_import_aliases`, `normalize_direct_owner_module_path`, `local_function_shadows_call`, `alias_binding_shadows_call`, `alias_spelling_binds_other_owner`, `local_module_shadows_alias`.
- `crates/ripr/src/analysis/classify/const_flow.rs`:
  `ModuleScopes::declaration_shadows_import`.

## Metrics

- `unit_test_pass_rate`
- `corpus_case_pass_rate`

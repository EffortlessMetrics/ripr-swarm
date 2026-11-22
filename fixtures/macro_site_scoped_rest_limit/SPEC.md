# Fixture: macro_site_scoped_rest_limit

Spec: RIPR-SPEC-0240

Issue: #8265

## Given

An `alloc::vec::*` glob makes assertion bindings uncertain under the current
manifest contract: #8388 trusts std/core roots but keeps alloc foreign. The
fixture uses the built-in alloc crate, with an explicit extern declaration,
so the foreign witness compiles without a new Cargo dependency. An unrelated
inline module in the integration-test file defines `vec`, outside the real `vec!` call's
scope. The glob cannot rebind `assert_eq!` (rustc E0659, #16060), so the
test's `vec!` call carries the uncertainty, and the scoped definition must not
become the refusal.

## When

`cargo xtask fixtures macro_site_scoped_rest_limit` analyzes the changed bound.

## Then

The nonempty findings remain `static_unknown`, retain
`rust_assertion_context_unresolved` and `gap_evidence_unresolved`, and name the
unscoped foreign glob. They have no canonical gap IDs.

## Must Not

- Select the unrelated scoped definition as the workspace refusal.
- Promote analyzer uncertainty to an actionable gap or claim test adequacy.
- Accept an empty or all-exposed re-blessing.

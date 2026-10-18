# Fixture: macro_site_scoped_rest_limit

Spec: RIPR-SPEC-0240

Issue: #8265

## Given

A foreign glob makes assertion bindings uncertain. An unrelated inline module
in the integration-test file defines `assert_eq`, outside the real assertion's
scope. The likely-file pass saturates on the glob and defers that definition.

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

# RIPR-SPEC-0320: Guarded test-only assertion edit precondition

Status: proposed

Owner: product / lsp

Created: 2026-11-02

Linked issues:

- #1902 (this slice)
- #1571 (parent design decision)
- #1931 / #1932 (post-application lifecycle and identity authority)

Support-tier impact:

- None. Types and validation only; no command emits or applies an edit.

Policy impact:

- None beyond documenting the proposed edit-class exception to the read-only
  default in RIPR-SPEC-0069 (see ADR 0025).

## Problem

Before any LSP or editor slice advertises a guarded quick fix, the repository
needs a machine-checkable precondition contract and a closed reject vocabulary.
Copy/open actions remain the safe fallback whenever a precondition field is
missing, stale, ambiguous, or outside the allowed edit surface.

## Behavior

ADR 0025 proposes one additional edit class:

```text
one existing assertion expression/node in one recognized Rust test file
→ one exact producer-owned replacement assertion
```

`crates/ripr/src/domain/guarded_edit.rs` defines:

- `ExactTestEditPrecondition` (`exact_test_edit_precondition.v1`) with the
  required identity, range, digest, command-ref, edit-surface, capability,
  and non-claim fields;
- `GuardedEditUnavailable` with the closed reason vocabulary;
- `try_build_exact_test_edit_precondition`, which mints a precondition from
  producer-owned inputs and fails closed on ambiguous, stale, preview,
  production, import-requiring, multi-file, or validation-failed cases;
- apply-time helpers (`verify_source_text`, `verify_document_version`,
  `client_supports_guarded_edit`) that reuse the same reasons.

Forbidden in v1: create/delete/rename, imports, fixtures/setup, control flow,
production files, multi-file edits, generated tests, provider calls, and
automatic test execution.

## Required Evidence

- ADR 0025 (proposed) records the design and, once accepted, amends the read-only default
  documented in RIPR-SPEC-0069.
- Positive fixtures in
  `fixtures/boundary_gap/expected/guarded-edit-precondition/corpus.json` for a
  broad `is_err()` replacement and a boundary equality replacement.
- One negative corpus row per builder-path reason exercised in the slice.
- Library tests filtered by `fix_instruction` cover builder, corpus, and
  apply-time helpers.

## Non-Goals

- No `CodeAction.edit`, `WorkspaceEdit`, or source mutation in this slice.
- No proof that a replacement is semantically correct or that tests pass after
  application.
- No producer wiring from live analyzer fixtures (#1893) in this slice.

## Acceptance Examples

1. A broad-error input with one test file, one assertion range, matching
   original text, passing parse/format validation, and verify/receipt command
   refs mints an `ExactTestEditPrecondition` with
   `required_client_capability.workspace_edit = true` and
   `ripr_editor_guarded_test_edit = true`.
2. A boundary-value equality replacement mints a precondition with the same
   surface bounds and a different replacement string.
3. Ambiguous site, required import, stale snapshot, preview limitation,
   production target, parse failure, or multi-file surface each return
   `GuardedEditUnavailable` with the named reason and no precondition.

## Test Mapping

- `crates/ripr/src/domain/guarded_edit.rs::tests::fix_instruction_guarded_edit_*`
- `fixtures/boundary_gap/expected/guarded-edit-precondition/corpus.json`

## Implementation Mapping

| Surface | Responsibility |
| --- | --- |
| `crates/ripr/src/domain/guarded_edit.rs` | DTOs, builder, apply-time checks, unit tests |
| `docs/adr/0025-guarded-test-only-edit-boundary.md` | proposed edit-class decision |
| `docs/specs/RIPR-SPEC-0069-lsp-agent-feedback-use-case.md` | read-only default amended to reference the guarded exception |
| `fixtures/boundary_gap/expected/guarded-edit-precondition/corpus.json` | schema-backed positive and negative fixtures |

## Metrics

- `exact_test_edit_precondition_schema_version`
- `guarded_edit_unavailable_reasons`

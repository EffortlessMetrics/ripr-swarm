# ADR 0025: Accept guarded test-only assertion replacement boundary

Status: accepted

Date: 2026-11-02

Linked issues: #1571, #1902

## Context

RIPR-SPEC-0069 and the editor ADR chain (#0012–#0017) established a read-only
LSP surface: copy/open guidance, no autonomous source mutation. Dogfood showed
that when producer evidence already owns one exact assertion site and one exact
replacement, a bounded test-only `WorkspaceEdit` could reduce friction without
opening production repair, imports, or multi-file patches.

## Decision

Accept one additional edit class, gated behind explicit client capability and a
versioned source precondition:

```text
one existing assertion expression/node in one recognized Rust test file
→ one exact producer-owned replacement assertion
```

Everything else remains forbidden: create/delete/rename, imports, fixtures,
setup, control flow, production files, multi-file edits, generated tests,
provider calls, and automatic test execution.

The repository records the boundary in RIPR-SPEC-0320 and types it in
`domain::guarded_edit` (`ExactTestEditPrecondition`, `GuardedEditUnavailable`).
This ADR does not authorize emitting or applying edits; later slices (#1903+)
must consume the precondition fail-closed.

## Consequences

- RIPR-SPEC-0069’s default read-only posture stays true for all commands today;
  the amended exception is documented and test-backed, not silently added to
  code actions in #1902.
- Clients need both ordinary workspace-edit support and explicit
  `riprEditor.guardedTestEdit = true` opt-in (see #1902 policy comment).
- Passive surfaces may carry digest/range/document-version/edit-instruction
  identity without embedding full original source lines.
- Reject-list reasons are closed vocabulary and schema-validated before any
  future edit advertisement.

## Non-goals

- No `CodeAction.edit` emission in the ratification slice.
- No proof that a replacement is semantically correct or that tests pass after
  application.

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
- Claim boundaries for this surface are governed by the canonical ledger in [support tiers](../status/SUPPORT_TIERS.md); nothing here promotes a tier.

Policy impact:

- None beyond documenting the proposed edit-class exception to the read-only
  default in RIPR-SPEC-0069 (see ADR 0026).

## Problem

Before any LSP or editor slice advertises a guarded quick fix, the repository
needs a machine-checkable precondition contract and a closed reject vocabulary.
Copy/open actions remain the safe fallback whenever a precondition field is
missing, stale, ambiguous, or outside the allowed edit surface.

## Behavior

ADR 0026 proposes one additional edit class:

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

One invariant check, `ExactTestEditPrecondition::validate`, sits behind both the
builder (which runs it on its own output) and any decoded document, so a decoded
or mutated value is held to the builder's rules:

- **Identity.** `EditInstructionId` is `edit-instruction:` plus a 16-hex FNV-1a 64
  digest over `(key, 0x00, value, 0x00)` pairs, in this order: `canonical_gap`,
  `seam`, `instruction_semantic`, `instruction_instance`, `workspace`,
  `repository_head`, `input`, `snapshot`, `config`, `base_revision`,
  `document_version`, `test_file` (normalized), `test_selector`,
  `original_digest` (the SHA-256 of the original text), `replacement`, `range`
  (`sl:sc-el:ec`). An absent optional input hashes as `none`, a present one as
  `some:<value>`. `validate` recomputes it, so a changed input under a minted id
  is rejected. The corpus holds the expected ids as literals derived by an
  independent reference of this paragraph, not copied from the builder.
- **Validation evidence.** Parse and format validation must both be `passed`.
  `failed` names `parse_validation_failed` / `format_validation_failed`;
  `not_run`, including an omitted field, is `source_precondition_missing`.
- **Target.** `test_file` must be a contained, normalized Rust test source: a
  `.rs` file under a `tests` directory segment, with no absolute root, drive
  prefix, empty, `.` or `..` segment. A naming convention (`*_test.rs`) cannot
  establish a test role (RIPR-SPEC-0153), so such files are not recognized.
  The sole allowed edit surface must equal it.
- **Source.** The original text and/or its `sha256:` digest (64 lowercase hex)
  must be retained, and agree when both are. `verify_source_text` compares the
  current text with the retained text, or with its SHA-256 when only the digest
  is retained.
- **Capability.** Both required client capabilities must be true.

Visibility: `domain::guarded_edit` is a `#[doc(hidden)]` public module, like
`domain::context_packet`, so the documented path is importable by sibling
adapters. Its items are recorded in `policy/public_api.txt`; the contract stays
proposed with ADR 0026, and no consumer emits an edit from it yet.

Forbidden in v1: create/delete/rename, imports, fixtures/setup, control flow,
production files, multi-file edits, generated tests, provider calls, and
automatic test execution.

## Required Evidence

- ADR 0026 (proposed) records the design and, once accepted, amends the read-only default
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
4. A parse or format validation that did not run (or was omitted) mints nothing.
5. A test path that traverses (`tests/../src/lib.rs`), is not Rust
   (`tests/readme.md`) or leaves the root (`../other/tests/demo.rs`) mints
   nothing, and a decoded precondition mutated to any of those is rejected.
6. Changing only the document version, the instruction instance or any snapshot
   identity field changes the `EditInstructionId`.

## Test Mapping

- `crates/ripr/src/domain/guarded_edit.rs::tests::fix_instruction_guarded_edit_*`
- `fixtures/boundary_gap/expected/guarded-edit-precondition/corpus.json`

## Implementation Mapping

| Surface | Responsibility |
| --- | --- |
| `crates/ripr/src/domain/guarded_edit.rs` | DTOs, builder, apply-time checks, unit tests |
| `docs/adr/0026-guarded-test-only-edit-boundary.md` | proposed edit-class decision |
| `docs/specs/RIPR-SPEC-0069-lsp-agent-feedback-use-case.md` | read-only default amended to reference the guarded exception |
| `fixtures/boundary_gap/expected/guarded-edit-precondition/corpus.json` | schema-backed positive and negative fixtures |

## Metrics

- `exact_test_edit_precondition_schema_version`
- `guarded_edit_unavailable_reasons`

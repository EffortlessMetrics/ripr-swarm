# Fixture: asserted_u32_helper_discarded

Spec: RIPR-SPEC-0159

## Given

One primitive byte converter and one real ordinary test. The selected test discards the helper result and contains no assertion.

## When

`cargo xtask fixtures asserted_u32_helper_discarded` analyzes the owner's behavior-preserving `| 0` edit.

## Then

The single return finding reports `reachable_unrevealed` and retains the exact selected test relation.

## Must Not

Borrow an unrelated oracle, credit a discarded or lossy result, or replace the changed owner with a same-name or independent observer.

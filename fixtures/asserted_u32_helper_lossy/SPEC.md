# Fixture: asserted_u32_helper_lossy

Spec: RIPR-SPEC-0159

## Given

One primitive byte converter and one real ordinary test. The helper compares only the low byte and can miss changes to higher bytes.

## When

`cargo xtask fixtures asserted_u32_helper_lossy` analyzes the owner's behavior-preserving `| 0` edit.

## Then

The single return finding reports `propagation_unknown` and retains the exact selected test relation.

## Must Not

Borrow an unrelated oracle, credit a discarded or lossy result, or replace the changed owner with a same-name or independent observer.

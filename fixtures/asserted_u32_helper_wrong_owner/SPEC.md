# Fixture: asserted_u32_helper_wrong_owner

Spec: RIPR-SPEC-0159

## Given

One primitive byte converter and one real ordinary test. The selected helper compares an independent converter; it does not call the changed owner.

## When

`cargo xtask fixtures asserted_u32_helper_wrong_owner` analyzes the owner's behavior-preserving `| 0` edit.

## Then

The single return finding reports `no_static_path` and retains the exact selected test relation.

## Must Not

Borrow an unrelated oracle, credit a discarded or lossy result, or replace the changed owner with a same-name or independent observer.

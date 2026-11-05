# Fixture: asserted_u32_helper_positive

Spec: RIPR-SPEC-0159

## Given

One primitive byte converter and one real ordinary test. The whole builtin u32 return is compared with the unchanged sample and the Boolean result is positively asserted.

## When

`cargo xtask fixtures asserted_u32_helper_positive` analyzes the owner's behavior-preserving `| 0` edit.

## Then

The single return finding reports `exposed` and retains the exact selected test relation.

## Must Not

Borrow an unrelated oracle, credit a discarded or lossy result, or replace the changed owner with a same-name or independent observer.

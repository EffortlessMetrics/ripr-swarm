# Fixture: rust_macro_table_fused_colon_token

Spec: RIPR-SPEC-0117

## Given

A local `macro_rules!` test table whose matcher spells a literal `:` after a `tt` and whose rows are written `name :: => (..)` (#10298). rustc lexes `::` as one token, so the table arm never matches, the empty fallback arm expands, and the file declares no test. Native execution enumerates zero tests.

## When

`ripr check` analyzes the changed predicate (`n > 9` -> `n >= 10`).

## Then

No generated test is indexed: the finding does not reach the owner and is not `exposed`.

## Must Not

Split the one `::` token into two `:` puncts, bind the first half to the `tt` and credit tests rustc never generates.

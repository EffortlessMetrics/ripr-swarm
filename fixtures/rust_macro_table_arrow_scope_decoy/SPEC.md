# Fixture: rust_macro_table_arrow_scope_decoy

Spec: RIPR-SPEC-0117

## Given

A local `macro_rules!` test table with `name => (..)` rows whose transcriber declares `mod decoy { pub fn gate(..) }` and a test that asserts `decoy::gate(..)` (#10298, #9967). The only generated test never calls the changed `gate`.

## When

`ripr check` analyzes the changed predicate (`n > 9` -> `n >= 10`).

## Then

No generated test is indexed for an arrow table whose body declares its own scope: the finding does not reach the owner and is not `exposed`.

## Must Not

Credit the generated test as exercising the changed function when it calls a different function of the same name.

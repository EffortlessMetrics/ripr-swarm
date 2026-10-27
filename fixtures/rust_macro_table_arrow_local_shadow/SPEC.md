# Fixture: rust_macro_table_arrow_local_shadow

Spec: RIPR-SPEC-0117

## Given

A local `macro_rules!` test table with `name => (..)` rows whose transcriber binds `let gate = |_| true;` before asserting `gate(..)` (#10298, #9967). The only generated test never calls the changed `gate`.

## When

`ripr check` analyzes the changed predicate (`n > 9` -> `n >= 10`).

## Then

No generated test is indexed for an arrow table whose body binds a local callable named like the owner: the finding does not reach the owner and is not `exposed`.

## Must Not

Credit the generated test as exercising the changed function when it calls a local binding of the same name.

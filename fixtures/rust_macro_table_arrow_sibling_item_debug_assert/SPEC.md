# Fixture: rust_macro_table_arrow_sibling_item_debug_assert

Spec: RIPR-SPEC-0117

## Given

A local `macro_rules!` test table with `name => (..)` rows whose transcriber emits a sibling item invoking `crate::debug_assert_eq!()`, an assertion-named exported macro, at module scope before the generated `#[test] fn`; the macro lives in a plain out-of-line `mod support;` and defines a `fn gate` that is not the changed function (#10298, #9967). The only generated test calls `gate`.

## When

`ripr check` analyzes the changed boundary (`n > 10` -> `n >= 10`).

## Then

No generated test is indexed for an arrow row that is not exactly one top-level `#[test] fn`: the finding does not reach the owner and is not `exposed`.

## Must Not

Credit the generated test as exercising the changed function when a sibling item beside it can redefine the callee.

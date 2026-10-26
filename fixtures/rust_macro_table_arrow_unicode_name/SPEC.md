# Fixture: rust_macro_table_arrow_unicode_name

Spec: RIPR-SPEC-0117

## Given

A local `macro_rules!` test table with `name => (..)` rows whose transcriber invokes `crate::αassert_eq!()` beside the test, an exported macro defined in another module that expands to a block-local `fn gate` (#10298, #9967). The only generated test never calls the changed `gate`.

## When

`ripr check` analyzes the changed predicate (`n > 9` -> `n >= 10`).

## Then

No generated test is indexed for an arrow table whose body invokes a macro from another module: the finding does not reach the owner and is not `exposed`.

## Must Not

Credit the generated test as exercising the changed function when it calls a block-local function the macro defines.

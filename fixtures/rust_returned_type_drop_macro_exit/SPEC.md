# Fixture: rust_returned_type_drop_macro_exit

Spec: RIPR-SPEC-0197

## Given

The owner returns `Cfg { n: x * 3 }`. The diff changes the constructed field from `x * 2` to `x * 3`. The only test compares the whole returned value: `assert_eq!(make(4), Cfg { n: 12 })`.

`Cfg`'s `Drop` impl is written by a `macro_rules!` macro (`exiting_drop!(Cfg)`), and its body exits the process. When the assertion fails and unwinding drops the returned value, the test binary exits 0, so the test cannot fail. The impl lives inside the macro's token tree, so the shared `Drop` scan cannot read its body (#15107).

## When

`cargo xtask fixtures rust_returned_type_drop_macro_exit` runs the public analyzer.

## Then

Neither finding on the changed line reads `exposed`; both read `weakly_exposed`, as for the hand-written impl in `rust_returned_type_drop_exit`.

## Must Not

- Credit a whole-value owner pin when a macro in the workspace writes a `Drop` impl, for this type or for a type it receives.

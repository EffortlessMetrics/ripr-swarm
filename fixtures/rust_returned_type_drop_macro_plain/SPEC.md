# Fixture: rust_returned_type_drop_macro_plain

Spec: RIPR-SPEC-0197

## Given

The control for `rust_returned_type_drop_macro_exit`. The same owner, diff and whole-value test, and a `macro_rules!` macro invoked for `Cfg`, but the macro writes an inherent method, not a `Drop` impl. `Cfg` has no `Drop` impl.

## When

`cargo xtask fixtures rust_returned_type_drop_macro_plain` runs the public analyzer.

## Then

Both findings on the changed line read `exposed`. A macro that writes no `Drop` impl does not withhold the pin (#15107).

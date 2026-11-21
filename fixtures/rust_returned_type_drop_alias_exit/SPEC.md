# Fixture: rust_returned_type_drop_alias_exit

Spec: RIPR-SPEC-0197

## Given

`src/guard.rs` declares `Cfg` with a `Drop` impl whose body exits the process. `src/lib.rs` re-exports it as `pub use guard::Cfg as Config;`. The owner returns `Config { n: x * 3 }`, and the diff changes the field from `x * 2` to `x * 3`. The only test compares the whole returned value: `assert_eq!(make(4), Config { n: 12 })`.

The returned value is a `Cfg`, so dropping it during unwinding exits 0 and the test cannot fail. The impl is written against the original name in another file (#15107).

## When

`cargo xtask fixtures rust_returned_type_drop_alias_exit` runs the public analyzer.

## Then

Neither finding on the changed line reads `exposed`; both read `weakly_exposed`.

## Must Not

- Credit a whole-value owner pin because the returned type is spelled through a rename or type alias of a type with a non-plain `Drop` impl.

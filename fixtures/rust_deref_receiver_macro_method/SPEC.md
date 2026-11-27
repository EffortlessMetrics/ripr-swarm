# Fixture: rust_deref_receiver_macro_method

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

The inherent `Inner::limit` changes. `Guard` derefs to `Inner`, and a workspace `macro_rules! method` expands `method!(Guard, limit);` to `impl Guard { pub fn limit(&self) -> u32 { 100 } }`, a `fn` whose name ripr cannot read. The test asserts `g.limit() == 100`. It passes on the old code and on the new code.

## When

`cargo xtask fixtures rust_deref_receiver_macro_method` runs the public diff analyzer.

## Then

The `return_value` finding does not read `exposed`. Any `macro_rules!` in the workspace refuses the deref reading. Two gates refuse this fixture at once: the `macro_rules!` gate, and the wrapper gate that refuses `Guard` as an argument of a macro other than the std assertion and formatting macros (`method!(Guard, limit)`). With the `macro_rules!` gate disabled, the second still refuses it.

## Must Not

- Credit `Inner::limit` because no `fn limit` is spelled on `Guard`.

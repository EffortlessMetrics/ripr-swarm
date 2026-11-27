# Fixture: rust_deref_receiver_partial_member

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

The inherent `Inner::limit` changes. `Guard` derefs to `Inner`. The workspace member `extlib` declares `trait Ext { fn limit(&self) -> u32 { 7 } }` with `impl<T> Ext for T {}`, and `app` imports `extlib::Ext`. The fixture runs in `fast` mode, which does not index `extlib`. The test asserts `Guard(Inner { cap: 3 }).limit() == 7`. `cargo test` passes on the old code and on the new code.

## When

`cargo xtask fixtures rust_deref_receiver_partial_member` runs the public diff analyzer.

## Then

The `return_value` finding does not read `exposed`. A Rust source under the workspace root that the index did not read refuses the deref reading, so the class does not depend on how much the mode indexed (in `deep` mode the second `fn limit` refuses it).

## Must Not

- Credit `Inner::limit` because the member crate's files were not indexed.

# Fixture: rust_deref_receiver_path_module

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

The inherent `Inner::limit` changes. `Guard` derefs to `Inner`. `#[path = "../shared/ext.rs"] mod ext;` loads a file outside `src` that declares `trait Ext` and `impl Ext for crate::Guard { fn limit(&self) -> u32 { 7 } }`, and the crate imports `Ext`. The test asserts `Guard(Inner { cap: 3 }).limit() == 7`. `cargo test` passes on the old code and on the new code.

## When

`cargo xtask fixtures rust_deref_receiver_path_module` runs the public diff analyzer.

## Then

The `return_value` finding does not read `exposed`. A `#[path]` attribute refuses the deref reading, and so does an out-of-line module whose target is not the default; when the mode leaves the loaded file unindexed, the workspace-closure gate refuses too.

## Must Not

- Credit `Inner::limit` because the indexed files spell no other `limit`.

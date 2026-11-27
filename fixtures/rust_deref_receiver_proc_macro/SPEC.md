# Fixture: rust_deref_receiver_proc_macro

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

The inherent `Inner::limit` changes. `Guard` derefs to `Inner`. The workspace member `mac` is a proc-macro library, and `#[derive(Ext)]` on the unrelated `Marker` emits `trait Ext` and `impl Ext for crate::Guard { fn limit(&self) -> u32 { 7 } }`, which no source file spells. The test asserts `Guard(Inner { cap: 3 }).limit() == 7`. `cargo test` passes on the old code and on the new code, because `Ext::limit` on `&Guard` is found before the deref.

## When

`cargo xtask fixtures rust_deref_receiver_proc_macro` runs the public diff analyzer.

## Then

The `return_value` finding does not read `exposed`. A non-std derive anywhere in the workspace refuses the deref reading. The proc-macro member manifest refuses it as well, and so does the `#[proc_macro_derive]` attribute when `mac` is indexed.

## Must Not

- Credit `Inner::limit` because no indexed source spells a `limit` on `Guard`.

# Fixture: rust_dyn_box_bare_object_other_file

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

The `rust_dyn_box_bare_object_impl` crate (edition 2018), with the
`Iterator` and `ExactSizeIterator` impls for the bare trait object `Shape`
moved into an out-of-line module `src/objects.rs` that imports
`crate::Shape`. That file spells neither `Box` nor `dyn`. The test binds
`let s: Box<dyn Shape> = Box::new(Square(3));` and asserts `s.len() == 4`.
It passes on the old code and on the new code, because the call runs
`ExactSizeIterator::len`.

## When

`cargo xtask fixtures rust_dyn_box_bare_object_other_file` runs the public
diff analyzer.

## Then

The `return_value` finding does not read `exposed`. The workspace gate
reads every file that names the trait, not only files that spell `Box` or
`dyn`.

## Must Not

- Credit `Square`'s `len` because the file holding the bare object impls
  never spells `dyn`.

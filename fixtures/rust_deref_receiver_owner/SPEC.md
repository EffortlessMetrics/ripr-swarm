# Fixture: rust_deref_receiver_owner

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

The inherent `Inner::limit` changes from `self.cap * 2` to `self.cap * 2 + 1`. `Guard(pub Inner)` has one `impl Deref for Guard` with `type Target = Inner;` and `fn deref(&self) -> &Inner { &self.0 }`, and nothing else in the workspace answers `limit`. The test binds `let g = Guard(Inner { cap: 3 });` and asserts `g.limit() == 7`. `cargo test` fails on the old code and passes on the new code.

## When

`cargo xtask fixtures rust_deref_receiver_owner` runs the public diff analyzer.

## Then

The `return_value` finding reads `exposed` with a `direct_owner_call` relation, the same as `Inner { cap: 3 }.limit()`. Method lookup finds nothing on `Guard`, `&Guard` or `&mut Guard`, derefs to `Inner` and runs the changed inherent method.

## Must Not

- Read the call as name-only because the receiver's type is the wrapper, not `Inner`.

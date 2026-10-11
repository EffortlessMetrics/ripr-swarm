# Fixture: rust_trait_parent_import

Spec: RIPR-SPEC-0108
Related spec: RIPR-SPEC-0197
Issue: #7175 (same production-path corpus gap as #7117)

## Given

The production `Render` trait has one implementation for the declared unit struct `Sample`. The `#[cfg(test)] mod helpers;` parent owns a separate `render_tests.rs` file whose test calls `Sample.render()` and pins the independently specified empty `String`.

The out-of-line parent module imports the production trait. The assertion executes the changed production method.

## When

```bash
cargo xtask fixtures rust_trait_parent_import
```

Independent compilation and runtime controls:

```bash
cargo test -p ripr --test owner_pin_execution trait_shadow_corpus_matches_compiled_owner_dispatch -- --exact --nocapture
```

## Then

Exactly one `return_value` finding reads `exposed`. Exactly one
related test retains its real file/line, an `exact_value`/`strong` assertion,
and relation `direct_owner_call`. The corpus gate independently pins the same
classification and relation; `goldens check` compares against the real analyzer.

The authored crate compiles and executes exactly one passing test. Changing
only the production expression from `String::from("")` to `String::from("wrong")` (actual `"wrong"`, expected `""`)
fails that test at its equality assertion.
The matched shadow fixture uses the same literal assertion but the opposite trait binding.

## Must Not

- Infer production dispatch from the method name or assertion tokens alone.
- Accept a missing finding or test as a non-promotion pass.
- Infer runtime behavior from an uncompiled, manufactured `RustIndex`.
- Generalize these static readings to mutation adequacy or path-prefix
  precision (`super::`/`crate::` under a shadow is the separate #7174).

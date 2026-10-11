# Fixture: rust_trait_default_import

Spec: RIPR-SPEC-0108
Related spec: RIPR-SPEC-0197
Issue: #7175 (same production-path corpus gap as #7117)

## Given

The production `Counter::advance` default computes `4 * self.step()` for `Unit`, whose step is the literal `2`. The test pins `self::Counter::advance(&Unit)` to the independent literal `8`.

The test module imports the production trait. The assertion executes the changed production method.

## When

```bash
cargo xtask fixtures rust_trait_default_import
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
only the production expression from `4 * self.step()` to `4 + self.step()` (actual `6`, expected `8`)
fails that test at its equality assertion.
The matched shadow fixture uses the same literal assertion but the opposite trait binding.

## Must Not

- Infer production dispatch from the method name or assertion tokens alone.
- Accept a missing finding or test as a non-promotion pass.
- Infer runtime behavior from an uncompiled, manufactured `RustIndex`.
- Generalize these static readings to mutation adequacy or path-prefix
  precision (`super::`/`crate::` under a shadow is the separate #7174).

# Fixture: rust_trait_default_overridden

Spec: RIPR-SPEC-0108
Related spec: RIPR-SPEC-0197
Issue: #15273

## Given

The production `Counter::advance` default changes from `4 * self.step()` to
`4 + self.step()`. `impl Counter for Unit` overrides `advance` to return `8`.
The test pins `Unit.advance()` to the literal `8`.

## When

```bash
cargo xtask fixtures rust_trait_default_overridden
```

## Then

Exactly one `return_value` finding reads `reachable_unrevealed`. The related
test keeps its `exact_value`/`strong` assertion, and its relation is
`weak_token_substring`, not `direct_owner_call`: every call in the test
resolves to `Unit`, whose impl overrides the default, so the changed default
never runs.

The authored crate compiles and its one test passes on both sides of the
diff, because `Unit.advance()` runs the override.
`rust_trait_default_import` is the control: there `Unit` keeps the default,
and the same literal pin reads `exposed`.

## Must Not

- Claim the test reaches the changed default.
- Advise adding a separating input to a test that cannot run the default.
- Infer runtime behavior beyond the static dispatch reading.

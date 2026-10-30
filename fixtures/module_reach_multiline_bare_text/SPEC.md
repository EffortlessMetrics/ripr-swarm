# Fixture: module_reach_multiline_bare_text

Spec: RIPR-SPEC-0159

## Given

The imported bare entry spelling appears only inside a multiline raw string; the only real call is a foreign `foreign::order_discount`. The changed line is the private helper `is_bulk` in `src/inner.rs`.

## When

```bash
cargo xtask fixtures module_reach_multiline_bare_text
```

## Then

Not related: the bare spelling is inside a string, not a call.

## Must Not

- Promote the finding on a counterexample, or lose the relation on the controls.

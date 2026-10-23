# Fixture: module_reach_multiline_qualified_text

Spec: RIPR-SPEC-0159

Owner: analysis-fixtures

Issue: #8999

## Given

The qualified entry spelling appears only inside a multiline raw string whose closing line holds a foreign call of the same name. The changed line is the private helper `is_bulk` in `src/inner.rs`.

## When

```bash
cargo xtask fixtures module_reach_multiline_qualified_text
```

## Then

Not related: the call is read from the masked file at its own line.

## Must Not

- Promote the finding on a counterexample, or lose the relation on the controls.

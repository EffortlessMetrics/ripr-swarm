# Fixture: module_reach_cfg_comment_hidden

Spec: RIPR-SPEC-0159

## Given

The module is declared `#[cfg(any())]` with a `// {` comment between the attribute and `pub mod inner;`. The changed line is the private helper `is_bulk` in `src/inner.rs`.

## When

```bash
cargo xtask fixtures module_reach_cfg_comment_hidden
```

## Then

Not related: the cfg is read from the parsed attribute, not a raw text scan.

## Must Not

- Promote the finding on a counterexample, or lose the relation on the controls.

# Fixture: typescript_module_value_unused

Spec: RIPR-SPEC-0243

## Given

The same changed exported initializer as `typescript_module_value_use`
(`export const stop = ...`). The test imports `stop` but never references it
in the test body; its only mention is in a comment.

## When

```bash
cargo xtask fixtures typescript_module_value_unused
```

## Then

The finding stays `no_static_path`: an unused import (or a comment mention)
is not a value use, so the heuristic `module_value_use` relation does not
link the test.

## Must Not

- Relate a test through an import that the test body never uses.

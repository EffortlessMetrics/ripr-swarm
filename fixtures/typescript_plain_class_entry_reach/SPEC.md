# Fixture: typescript_plain_class_entry_reach

Spec: RIPR-SPEC-0243

## Given

A TypeScript module with two module-private classes (#15928 rows B1, B2):

- the exported `serialize` constructs `Serializer` and calls a method in
  one expression: `new Serializer().write(input)`;
- `write` calls its sibling `this.order(...)`, and `order` passes the
  module function `compareKeys` to `Array.prototype.sort`;
- `Serializer.describe` is called by nothing;
- the exported `count` takes a parameter named `Counter` and constructs
  that, so the module's own `class Counter` is not what it runs.

The diff changes a return in `compareKeys`, in `write`, in `order`, in
`describe` and in `Counter.total`.

A Vitest test calls `serialize(...)` and pins its string result. Another
calls `count(...)` with a class of its own.

## When

```bash
cargo xtask fixtures typescript_plain_class_entry_reach
```

## Then

- The changes in `write`, `order` and `compareKeys` are related to the
  `serialize` test through the bounded module-entry relation and read
  `static_unknown`, not `no_static_path`.
- The change in `describe` stays `no_static_path` with no related test:
  the entry's call path does not run it.
- The change in `Counter.total` stays `no_static_path` with no related
  test: `count` constructs its parameter.

## Must Not

- Reach `exposed` through the entry relation, or start a repair from it.
- Relate the `count` test to `Counter.total`.
- Use mutation-runtime outcome vocabulary reserved for real mutation execution.

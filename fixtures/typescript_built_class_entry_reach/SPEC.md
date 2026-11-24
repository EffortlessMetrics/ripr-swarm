# Fixture: typescript_built_class_entry_reach

Spec: RIPR-SPEC-0243

## Given

A TypeScript module in the shape of unjs/ohash `serialize` (#15928):

- the exported `serialize` constructs a module-private class and calls it:
  `new Serializer().serialize(input)`;
- the class is declared inside an immediately invoked function assigned to
  `const Serializer`;
- a class method passes the module function `compareStrings` by reference
  to `Array.prototype.sort`.

The diff changes a return in `compareStrings`, the `sort` call inside the
class, and a return in `compareLengths`. `compareLengths` is the control:
its only mention is `setTimeout(compareLengths, 0, ...)` in the exported `later`,
a deferred reference.

A Vitest test calls `serialize(...)` and pins its string result. Another
calls `later(...)`.

## When

```bash
cargo xtask fixtures typescript_built_class_entry_reach
```

## Then

- The change inside the class (owner `Serializer`) and the change in
  `compareStrings` are related to the `serialize` test through the bounded
  module-entry relation and read `static_unknown`, not `no_static_path`.
- The change in `compareLengths` stays `no_static_path` with no related
  test: a function handed to `setTimeout` is not run by the call.

## Must Not

- Reach `exposed` through the entry relation, or start a repair from it.
- Relate the `later` test to `compareLengths`.
- Use mutation-runtime outcome vocabulary reserved for real mutation execution.

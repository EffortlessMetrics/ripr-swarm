# Fixture: typescript_module_value_use

Spec: RIPR-SPEC-0243

## Given

A TypeScript module changes the initializer of an exported module-level
value:

```ts
export const stop = skip;          // before
export const stop = Symbol('stop'); // after
```

A Jest-style test imports `stop` from the module and passes it as a value
inside a call argument (`collect([1, skip, 2, stop, 3])`). The binding is not
the direct subject of `expect(...)`, so it is not a module-initializer
observer.

## When

```bash
cargo xtask fixtures typescript_module_value_use
```

## Then

- The test is related to the changed initializer through the heuristic
  `module_value_use` relation, so the finding no longer reads
  `no_static_path` (#8568).
- The relation is uncertain: it carries no oracle credit, so the finding is
  `weakly_exposed` at most and the repair packet is not ready.

## Must Not

- Reach `exposed` from a value use alone.
- Claim a test observes the initializer.

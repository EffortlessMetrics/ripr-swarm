# Fixture: typescript_reexport_class_receiver_no_false_credit

Spec: RIPR-SPEC-0095

## Given

A TypeScript preview workspace changes the `origin` getter of class `Url` in
`src/url.ts`. A DIFFERENT class named `Url` lives in `src/other.ts`, and the
barrel `src/index.ts` forwards that one:

```typescript
export * from './other';
```

A test imports `Url` from the barrel, constructs it and asserts on `origin`.

## When

```bash
cargo xtask fixtures typescript_reexport_class_receiver_no_false_credit
```

## Then

The TypeScript preview adapter resolves the barrel to `src/other.ts`, which is
not the changed owner's module, so the test is not related: the owner stays
`no_static_path` and no `re_export_chain_followed` relation is emitted.

## Purpose

No-false-credit control for the class receiver path (#13592 B3): the barrel
chain must land on the owner's own class, not merely on a class of the same
name.

## Must Not

- Credit the test because the class name matches.
- Emit `re_export_chain_followed` when the chain leads to a different module.

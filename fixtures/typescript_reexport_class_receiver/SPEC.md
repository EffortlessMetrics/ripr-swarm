# Fixture: typescript_reexport_class_receiver

Spec: RIPR-SPEC-0095

## Given

A TypeScript preview workspace changes the `origin` getter of class `Url` in
`src/url.ts`. The barrel `src/index.ts` forwards the class:

```typescript
export * from './url';
```

A test in `test/url.test.ts` imports the class from the barrel, constructs it
and asserts on the getter with `toBe`:

```typescript
import { Url } from '../src/index';
const url = new Url('https://x');
expect(url.origin).toBe('https://host');
```

## When

```bash
cargo xtask fixtures typescript_reexport_class_receiver
```

## Then

The TypeScript preview adapter:

- resolves the barrel hop (`test -> index.ts -> url.ts`) to the class `Url`
  declared in the owner's module, exactly as it does for a function owner;
- relates the test through the constructed receiver and discloses it as
  `relation_reason: re_export_chain_followed` (`medium` confidence);
- classifies the finding as `exposed` (strong `toBe` oracle), the same class
  the test reads when it imports `../src/url` directly.

## Purpose

Positive twin of `typescript_reexport_class_receiver_no_false_credit`
(#13592 B3): the receiver and constructor relations follow the same bounded
barrel chain that function owners already follow.

## Must Not

- Leave the owner at `no_static_path` merely because the class arrived through
  a barrel.
- Present the relation as a direct import (the barrel hop is disclosed).

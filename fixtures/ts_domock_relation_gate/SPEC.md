# Fixture: ts_domock_relation_gate

Spec: RIPR-SPEC-0026

## Given

A TypeScript owner `applyDiscount(amount, threshold)` changes its predicate
boundary (`amount > threshold` becomes `amount >= threshold`). The related
test registers `jest.doMock("../src/pricing")` — the hoisted mock variant
both Jest and Vitest support (issue #4103 shape 2). Without collecting
`doMock` registrations, the extractor would miss the owner-module mock and
the exact-value assertion on the mocked call would be credited as real
boundary evidence.

The fixture workspace enables the TypeScript preview adapter via
`ripr.toml`:

```toml
[languages]
enabled = ["rust", "typescript"]
```

## When

```bash
ripr check \
  --root fixtures/ts_domock_relation_gate/input \
  --diff fixtures/ts_domock_relation_gate/diff.patch \
  --mode fast
```

## Then

The TypeScript preview adapter:

- collects the `jest.doMock("../src/pricing")` registration into
  `mocks_in_file` like a `jest.mock(...)` call,
- refuses the trusted owner-call relation (the owner module is mocked, so
  the call executes the mock, not the changed code),
- surfaces the `mocked_module` static limit with the named limitation
  `typescript_mock_only_observer`,
- keeps the finding `weakly_exposed` and advisory.

## Must Not

- Promote the finding to `exposed` on an assertion that observes a mocked
  substitution.
- Treat `jest.doMock(...)` / `vi.doMock(...)` as invisible to owner-module
  mock detection.

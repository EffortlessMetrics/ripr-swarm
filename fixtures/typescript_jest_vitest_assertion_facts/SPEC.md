# Fixture: typescript_jest_vitest_assertion_facts

Spec: RIPR-SPEC-0027

## Given

A TypeScript preview workspace changes several function-level behaviors and
opts in to the TypeScript-family adapter:

```toml
[languages]
enabled = ["rust", "typescript"]
```

The related Vitest/Jest-style tests cover nested `describe(...)`,
`test.each(...)`, `it.each(...)`, exact-value assertions, async
`resolves`, mock interaction assertions, bounded mock payload assertions,
snapshot assertions, and smoke-only truthiness assertions.

## When

```bash
cargo xtask fixtures typescript_jest_vitest_assertion_facts
```

or:

```bash
ripr check \
  --root fixtures/typescript_jest_vitest_assertion_facts/input \
  --diff fixtures/typescript_jest_vitest_assertion_facts/diff.patch \
  --mode fast
```

## Then

The TypeScript preview adapter:

- discovers tests nested below `describe(...)` blocks;
- discovers array-form `test.each(...)` and `it.each(...)` calls;
- maps `toStrictEqual`, `toBe`, and `resolves.toBe` to exact-value oracles;
- maps `toHaveBeenCalledWith` to a mock-interaction oracle;
- names literal mock payload evidence when the callee and expected argument are
  syntax-bounded;
- reads a literal `toHaveBeenCalledWith` payload on a `vi.fn()` the test
  binds under the owner parameter's own name as strong (RIPR-SPEC-0247
  rule 1), so `notifyStatus` and `notifyLiteral` read `exposed`;
- keeps `notifyAudit` at `mock_expectation` / medium and `weakly_exposed`:
  its test asserts on a `record` mock passed inside a `sink` local, not on
  the parameter's own name, so RIPR-SPEC-0247 rule 1 refuses it, and the
  object is not an inline literal argument, so RIPR-SPEC-0256 rule 2
  refuses it too (this is the
  `mock_interaction_without_payload_proof` audit case's source finding);
- keeps snapshot and smoke-only assertions weak instead of treating them as
  strong discriminators;
- preserves `language = "typescript"`, `language_status = "preview"`, and
  `owner_kind = "function"` on the findings.

## Must Not

- Invoke `tsc`, `tsserver`, Vitest, Jest, a package graph, or mutation runtime.
- Promote TypeScript/JavaScript beyond opt-in preview.
- Treat snapshot or smoke-only assertions as strong repair proof.
- Change default gates, badges, baselines, branch protection, or RIPR Zero.

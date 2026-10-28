# Fixture: typescript_exported_expect_lookalike

Spec: RIPR-SPEC-0243

## Given

A changed `price` return value is import-anchored by four ordinary tests.
Each test module declares its own no-op `expect`, using an exported const,
an exported destructuring declaration, an exported function, or a named
default-exported function. Exporting a declaration preserves its local binding.
The JavaScript const case and TypeScript cases share the matcher admission path.

## When

`cargo xtask fixtures typescript_exported_expect_lookalike` analyzes the
return-value diff through the production TypeScript preview adapter.

## Then

The single finding stays `weakly_exposed`, with four related tests and
`unknown` oracle strength/kind. Each lookalike accepts the wrong return value;
its `toBe` property cannot establish a runner matcher. The honesty corpus
independently pins the finding count, class and oracle, so blessing an
`exposed` golden cannot satisfy that contract.

## Must Not

- Credit a local exported `expect` as the unbound runner global.
- Lose the changed owner or its four import-anchored related tests.
- Change genuine global/imported matcher or imported Node assertion credit.
- Infer runtime execution, infection or propagation from static evidence.
- Change callable registration, test activation or table-row semantics.

# Fixture: javascript_jest_cjs_unexported_class

Spec: RIPR-SPEC-0243

Additional contract: #16493.

## Given

`src/cart.js` declares `class Cart` and exports it only as
`exports.Basket = Cart`. The edit changes `n > 50` to `n >= 50` in
`Cart.total`. A Jest test destructures `const { Cart } = require('../src/cart')`
and pins the exact boundary, `expect(new Cart().total(50)).toBe(45)`.

## When

Analyze the committed change in `diff.patch` with the TypeScript adapter.

## Then

The module exports nothing named `Cart`, so the test's `Cart` is `undefined`
and `new Cart()` throws on every run. The test cannot tell the edit apart and
is not related to `Cart.total`. The finding reads `no_static_path`.

The honesty corpus keeps this case from promoting. The adapter tests in
`tests/unexported_class_import_tests.rs` pin the controls: `exports.Cart`,
`module.exports.Cart`, `module.exports = { Cart }` and a whole-module class
export stay related, and a module whose exports cannot be read in full keeps
the relation.

## Must Not

- Relate a named import of a class name the module provably does not export.
- Drop a relation when the module's export facts are unknown or forwarded.
- Infer a runtime mutation result from static evidence.

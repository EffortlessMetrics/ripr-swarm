# Fixture: ts_cross_module_reach_chain

Spec: RIPR-SPEC-0243

Issue: #13592

## Given

`src/a.ts` exports `decodeThing`, whose replacement string the diff changes
from `"%2F"` to `""`. `src/b.ts` imports `decodeThing` and exports
`normalize`, which bare-calls it. The only test imports `normalize` and
asserts `expect(normalize("a%252Fb")).toBe("/ab")`; no test calls
`decodeThing` itself.

## When

`ripr check --root input --diff ../diff.patch --format json --mode fast`
exercises the production CLI.

## Then

`decodeThing` is related to the test through the exported caller in another
module (one call edge across one import), so reach is `yes` and the finding is
no longer `no_static_path`. The reach is indirect: the finding is
`static_unknown` and says the tests reach it "only through cross-module
callers (`normalize`)".

## Must Not

- Report `no_static_path` for a helper a test runs through an importing
  exported function.
- Report `exposed`, or start a repair, from reach that never calls the owner
  itself.

<!-- section: Fixed -->
- TypeScript committed-snapshot: a Jest or Vitest config whose root object
  spreads an unread base (`...require("./jest.base")`, `...baseConfig`) or
  uses a variable-computed key no longer keeps the strong snapshot credit,
  because the base may carry a snapshot override (#9940). Array and
  nested-object spreads are unaffected.

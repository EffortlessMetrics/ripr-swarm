<!-- section: Fixed -->
- The TypeScript preview adapter now reads a `node:assert` call inside a
  `.then(` callback of the owner's promise, like an `expect(` there, as
  assertion-like code it could not resolve: the finding is `static_unknown`
  instead of an actionable `weakly_exposed` gap, since whether a late failure
  fails the run depends on the runner (#9686).

<!-- section: Fixed -->
- TypeScript: a node:assert call (`assert(...)`, `assert.equal(...)`) inside a floating `.then` callback tied to the changed owner now reads `static_unknown` instead of an actionable `weakly_exposed` gap, because whether a late rejection fails the run depends on the runner; nothing is credited (#9686).

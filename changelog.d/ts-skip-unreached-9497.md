<!-- section: Internal / CI -->
- TypeScript analyzer: a `node:test` body that calls `t.skip()` / `t.todo()` registers no discriminator, and a changed `return` expression that old and new versions evaluate identically on every literal input the strong assertions use reads `weakly_exposed` instead of `exposed` (RIPR-SPEC-0243 rules 16 and 17; #9497).

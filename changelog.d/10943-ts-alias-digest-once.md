<!-- section: Fixed -->
- A TypeScript check in a workspace with a `package.json`-owned package no longer renders the whole package table once per indexed file. The alias-map digest is computed once per run and the table is shared instead of deep-copied for each parse, so a valibot-sized monorepo diff drops from about 65 s to a few seconds (#10943).

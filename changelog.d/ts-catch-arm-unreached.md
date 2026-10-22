<!-- section: Fixed -->
- TypeScript: a changed line inside a `catch` arm (or `.catch(` handler) that no related test can make run, because no related test file holds a throwing or rejecting signal, now reads `weakly_exposed` with a named missing line instead of `exposed` (RIPR-SPEC-0246 rule 6, #9437).

<!-- section: Fixed -->
- TypeScript: an edit to the comparison in a ternary condition (`n === 1` to `n <= 1`) no longer reads `exposed` when every related test passes a numeric literal that both comparisons treat alike; it reads `weakly_exposed` and names a value where they differ (RIPR-SPEC-0246 rule 9, #9437).

<!-- section: Fixed -->
- Dependent-scope narrowing no longer rebuilds a widened reach index per owner on diffs with many distinct caller closures: once the widened builds pass the cache capacity, the run builds one full-selection reach index, shares it and the lookups derived from it across the remaining owners, and keeps every finding unchanged (#9278).

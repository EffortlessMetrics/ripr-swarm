<!-- section: Fixed -->
- An expectation computed by the owner itself behind a `const`, an alias chain, a `const fn` or an associated constant is now pinned in a production-path test as never `exposed`, with a literal `const` as the control that keeps its credit (#7260).

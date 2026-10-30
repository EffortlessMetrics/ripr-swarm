<!-- section: Internal / CI -->
- Verdict corpus: the TypeScript ledger test no longer pins the exact SPEC-0243 example count, and the spec-coverage floor need not be raised by every PR that adds an example, so concurrent corpus PRs stop conflicting on those lines (#9859).

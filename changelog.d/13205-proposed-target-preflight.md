<!-- section: Added -->
- A repair attempt that follows a producer-owned new-test proposal now preflights the proposed target read-only (a plain in-repository path, an unchanged stale-proposal rule, an owning Cargo package) and says in the attempt's limitations that no test exists yet for it, instead of a missing result reading as an empty one; attempts on existing tests are unchanged (#13205).

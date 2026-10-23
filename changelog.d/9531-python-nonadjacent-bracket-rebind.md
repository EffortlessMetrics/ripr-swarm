<!-- section: Fixed -->
- Python protocol-entry receivers rebound through non-adjacent bracket targets such as `(x, (r)) = ...` and `[a, [r]] = ...` including outer/chained assignments and inline loop/with targets no longer receive `exposed` credit (#9531).

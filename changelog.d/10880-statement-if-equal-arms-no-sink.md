<!-- section: Fixed -->
- A statement `if`/`else` whose arms assign or do the same thing (`if x >= 5 { r = 1; } else { r = 1; }`, one-line or rustfmt layout) no longer reads its condition as exposed through a tail such as `r + 0`: the choice between identical arms is unobservable, so the condition has no returned-value sink (#10880).

<!-- section: Fixed -->
- A private generic helper whose whole body is `if cond { a } else { b }` over two different parameters (for example a `capped(value, cap)` helper) now has the condition as its return sink, so a boundary change tested through an exact forwarding wrapper reads `exposed` instead of `propagation_unknown` (#6694).

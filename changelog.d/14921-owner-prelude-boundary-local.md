<!-- section: Changed -->
- A boundary local preceded only by `let flag = !flag;` rebindings or function-local integer `const` items now resolves to the boundary the comparison actually reads, so a test that misses it is reported as a gap instead of `infection_unknown`. Any other prelude shape keeps the earlier refusal (#12771).

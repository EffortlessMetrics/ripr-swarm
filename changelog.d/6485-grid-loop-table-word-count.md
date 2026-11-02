<!-- section: Fixed -->
- Loop accumulators whose update is `count += line.split_whitespace().count()` over a `&[&str]` walk now map to `return_value`, so constant-row table pins such as `grid-loop-table` credit the compound-assignment rewrite instead of staying `static_unknown` (#6485).

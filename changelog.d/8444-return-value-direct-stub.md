<!-- section: Fixed -->
- `ripr agent stub --at … --kind return_value` no longer substitutes the nearest in-function return-value gap when the requested line holds none; it emits a direct stub for that line's return probe or an explicit refusal (#8444).

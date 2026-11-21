<!-- section: Fixed -->
- `ripr explain` and the human finding block: when a condition and its block body both changed, such as `if total >= 100 { total - 10 }` to `if total > 200 { total - 25 }`, the `after` row now says it is the condition only and names the line to read, instead of reading as if the block were deleted. `before` stays the whole old line, which classifiers rely on (#15420).

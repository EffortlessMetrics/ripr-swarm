<!-- section: Fixed -->
- `ripr check` no longer reports a replaced line twice. For example, when a tail return such as `x * 5` changes to `x * 10`, the changed return value is reported once instead of also as a deleted `static_unknown` behavior. A line deleted with no replacement keeps its finding (#7807).

<!-- section: Fixed -->
- `ripr explain` and the context witness: when a condition's operator or literal and the block body after it all changed, such as `if total >= 100 { .. }` to `if total > 200 { .. }`, the old side now shows the old condition (`total >= 100`) next to the new one instead of the whole old line (#15420).

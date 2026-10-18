<!-- section: Fixed -->
- A test file that is also a module parent is now read with the same macro
  list whichever probe reaches it first. Before, a nested test running first
  could make the parent file's own `json!` assertion refuse (it is now
  admitted, as it was when the parent ran first), and with parallel
  classification the order was scheduling (#8797).

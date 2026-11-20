<!-- section: Fixed -->
- Rust: an item moved within its file with no other change no longer gets
  findings. A function moved to another place in the same module, or a
  method moved inside the same `impl`, used to be reported as changed behavior
  on both sides of the move. A move with an edit, a move into another `impl`
  or module, and a reordered statement, field or variant keep their findings
  (#15110).

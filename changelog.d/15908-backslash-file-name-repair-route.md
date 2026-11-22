<!-- section: Fixed -->
- On Unix, a Rust file whose name holds a literal backslash (`src/a\b.rs`)
  now keeps its own path in findings and gap IDs (`src/a%5Cb.rs`) instead of
  borrowing `src/a/b.rs`, and `ripr check` offers its repair start. The repair
  attempt's edit check now also sees edits to tracked files with such names;
  before, it looked for a `/`-separated path and missed them. Gap IDs and spot
  IDs change for such files only (#15908).

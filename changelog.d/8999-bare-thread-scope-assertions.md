<!-- section: Fixed -->
- An exact assertion inside `scope(|s| ..)` (or a joined `spawn(..)`) now
  earns exact credit when the file imports it with exactly
  `use std::thread::scope;` in the test's own module. Before, only the
  qualified `std::thread::scope` / `thread::scope` spellings were credited.
  A local `scope`, another import, a rename or an import list stays
  uncredited (#8999).

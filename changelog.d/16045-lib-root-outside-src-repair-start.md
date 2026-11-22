<!-- section: Fixed -->
- `ripr check` now offers a `ripr repair` start for a weak finding in a
  library root outside `src/` (`[lib] path = "source/entry.rs"`) and in a
  symlinked `src` directory or `src/lib.rs`, the same start the default
  layout gets. Before, the finding read `weakly_exposed` with no start
  because the repo spot inventory left those files out (#16045).
- With a symlinked `src` that also holds `main.rs`, a module only the library
  root declares now names its owner, as in the plain layout. Before, every
  module there read as unplaced (#16068).

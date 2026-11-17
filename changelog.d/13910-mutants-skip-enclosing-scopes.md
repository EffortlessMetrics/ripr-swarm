<!-- section: Added -->
- `ripr check` honors cargo-mutants' `#[mutants::skip]` (and
  `#[cfg_attr(<predicate>, mutants::skip)]`) on a scope that encloses the
  changed owner, as cargo-mutants does: an `impl` or `trait` block, an inline
  module at any depth, or the whole file through an inner
  `#![mutants::skip]`. Such findings are withheld and disclosed exactly like a
  function-level skip; JSON adds `findings[].withheld_scope` (`fn`, `impl`,
  `trait`, `module` or `file`) and the human block names the scope. Enclosing
  scopes come from the parsed syntax tree and a file that does not parse
  cleanly withholds nothing. A `#[mutants::skip] mod name;` declaration is not
  followed into the module's own file. `ripr explain` now states on its first
  line when the selected finding is withheld by an owner opt-out (#13910).

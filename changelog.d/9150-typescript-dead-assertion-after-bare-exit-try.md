<!-- section: Fixed -->
- TypeScript: an assertion written after `try { return; } catch (e) {}` (a
  `try` block whose first statement is a bare `return;`, a literal `return`,
  `break` or `continue`) is dead code, because nothing can throw into the
  `catch`, and no longer earns oracle credit. A `try` that does work before
  exiting (`try { work(); return; } catch (e) {}`) keeps the later credit
  (#9150).

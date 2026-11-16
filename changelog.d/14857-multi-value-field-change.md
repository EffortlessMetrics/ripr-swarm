<!-- section: Fixed -->
- A TypeScript object-literal change that moves two field values on one line
  no longer reads `exposed` when the test asserts only a third, untouched
  field: an unresolvable change is not evidence the test observed it, so the
  finding is `weakly_exposed` (#14857). A read that names either changed field
  keeps `exposed`, including through a nested path (`f().retries.n`), and so
  does a whole-object assertion whose subject is a compound expression
  (`f() ?? defaults.config`), because the assertion receives the whole object.

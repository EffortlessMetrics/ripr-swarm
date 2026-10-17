<!-- section: Fixed -->
- Python preview no longer credits `exposed` from an assertion that calls a
  locally rebound stand-in of the imported owner (`Mock`, an aliased `Mock`,
  a lambda, or a local `def`). The same assertion on the unshadowed import,
  a genuine import alias, a function-local import, or an outer call beside a
  helper-local shadow still observes the production return (#8247).

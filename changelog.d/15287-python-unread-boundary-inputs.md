<!-- section: Fixed -->
- Python: a `<` to `<=` style boundary flip no longer reads `exposed` when the
  related tests call the owner with inputs ripr cannot read (`clamp(*args)`
  parametrize rows, a parametrize table whose rows name constants, a
  reassigned test local). Those findings now read `weakly_exposed` and name no
  repair target. A first-statement `value = float(value)` or
  `value = int(value)` coercion is read through, so an off-boundary literal
  argument such as `from_timestamp(-10)` reads weak and names `value == 0`
  (#15287).

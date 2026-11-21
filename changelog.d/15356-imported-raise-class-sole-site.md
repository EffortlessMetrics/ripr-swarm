<!-- section: Fixed -->
- A Python raise changed from a builtin to an imported exception class
  (`raise ValueError(...)` to `raise InvalidVersion(...)`) now reads `exposed`
  when a test runs the owner under a plain `pytest.raises(InvalidVersion)`
  imported from the defining module and the changed line is the class's only
  construction site in the production tree. A context naming the base class,
  the same spelling from another module, or a class constructed elsewhere
  stays `weakly_exposed` (#15356).

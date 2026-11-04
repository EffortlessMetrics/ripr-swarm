<!-- section: Fixed -->
- Python Hypothesis `@example(...)` rows are admitted as concrete input facts
  only when static `@settings(phases=...)` on the test still includes
  `Phase.explicit`; omitted or unresolvable phase lists withhold the example
  rows instead of treating them as parametrized witnesses (#12544).

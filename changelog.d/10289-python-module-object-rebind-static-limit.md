<!-- section: Fixed -->
- Python: a module-object field write whose related test reloads the module or replaces its names (`importlib.reload`, `setattr`, `__setattr__`, `delattr`, or attribute assignment) now reads `static_unknown` with a `mocked_module` limit instead of a weak finding. The old weak finding carried a repair card that asserted on the stale imported object (#10289).

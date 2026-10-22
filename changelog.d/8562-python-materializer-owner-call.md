<!-- section: Fixed -->
- Python tests that read a generator or iterator owner through a sole-argument
  materializer (`list`, `tuple`, `set`, `frozenset`, `sorted`) now align the
  owner call with the asserted sink, for both the direct operand and a local
  assigned from it. Other wrappers stay opaque (#8562).

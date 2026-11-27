<!-- section: Fixed -->
- `ripr check`: for a Python edit inside a dunder method that tests exercise through syntax (such as `FIFOCache.__setitem__` reached by `cache[k] = v`), the `reach` stage now reads `unknown` instead of `no` when the `dynamic_dispatch` limit applies, so the stage no longer claims what the limit disclaims (#15339).

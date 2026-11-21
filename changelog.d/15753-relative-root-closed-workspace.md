<!-- section: Fixed -->
- `ripr check --root .` now gives the same results as an absolute root for
  analyses that need to know the workspace has no outside dependencies, such as
  one-hop field projection. Before, a relative root made that check fail, so
  those findings came out as `propagation_unknown` instead of `exposed`
  (#15753).

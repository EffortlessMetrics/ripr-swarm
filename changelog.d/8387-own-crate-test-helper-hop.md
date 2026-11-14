<!-- section: Fixed -->
- `ripr check` relates an integration test that reaches the changed function
  through its own file's helper when the helper spells the call through the
  package's library name (`fn t(p: &str) -> Hir { pkg::hir::translate(p) }`),
  so the test a diff adds through such a helper is listed as reaching the
  change instead of as a name-token neighbour; the link is reach only and
  cannot make a finding `exposed` (#8387).

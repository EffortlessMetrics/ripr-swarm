<!-- section: Fixed -->
- Missing-input reads now name the path and the recovery route in the sibling
  commands' vocabulary: `receipt check --path` and a missing `--ledger` point
  at `ripr receipt write` / `ripr reports gap-ledger`, `swarm queue
  --gap-ledger` and `agent packet --gap-ledger` point at `ripr reports
  gap-ledger`, and `baseline create --from` / `baseline update` point at `ripr
  gate evaluate`; `cache export` on a root without analysis output points at
  repo exposure / repo badge analysis; a missing file reads `not found`
  instead of leaking the raw `(os error N)` text (#9851).

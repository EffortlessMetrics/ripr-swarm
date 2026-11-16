<!-- section: Changed -->
- `ripr.collectRepairPacket` with no packet on disk now points at
  `ripr check --root .` and the repair start it prints (`ripr repair <id>`
  for Rust) for the current change, instead of the whole-repo `ripr pilot`
  and gap-ledger recipes (#13636).

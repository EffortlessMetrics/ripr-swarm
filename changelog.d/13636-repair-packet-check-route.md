<!-- section: Changed -->
- `ripr.collectRepairPacket` with no packet on disk now points at
  `ripr check --root .` then `ripr repair <id> --root .` for the current
  change, instead of the whole-repo `ripr pilot` and gap-ledger recipes (#13636).

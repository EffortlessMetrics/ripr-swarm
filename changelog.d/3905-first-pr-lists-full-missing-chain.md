<!-- section: Fixed -->
- `ripr first-pr` on a Rust root with no evidence now lists the whole regeneration chain in one output (repo exposure report, gap decision ledger, then PR repair cards, with the cards marked as needed only when the ledger names no repair start) instead of revealing one missing artifact per run. It still only reads artifact presence and runs no analysis (#3905).

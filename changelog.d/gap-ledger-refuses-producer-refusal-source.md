<!-- section: Fixed -->
- `ripr reports gap-ledger` exits 2 when its source is a producer refusal (`downstream_consumable: false`, nothing analyzed), naming the source flag and the recorded `repair_route` (or, for a saved ledger that records none, saying to re-run the producing check), instead of exiting 0 with a `blocked` ledger; the `blocked` ledger is still written first (#14434).

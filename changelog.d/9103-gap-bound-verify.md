<!-- section: Added -->
- `ripr agent verify --gap-id <canonical gap id>` binds a verify over two `check-findings-json` snapshots to one gap: it succeeds only when the gap is open before and `exposed` after, reports it under `bound_gap`, and refuses an absent, already exposed or still open gap with exit `3`; absence from the after snapshot is never closure (#9103).

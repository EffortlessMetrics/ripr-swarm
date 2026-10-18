<!-- section: Fixed -->
- `ripr mcp --stdio` resource reads now accept percent-encoded template
  expansions such as `ripr://gap/gap%3Arust%3Asrc%2Flib.rs%3A...`, which
  RFC 6570 clients send for the advertised `{canonical_id}` and
  `{snapshot_id}` templates; they previously answered `item_not_found` or
  `stale_snapshot` for ids the server itself links unencoded.

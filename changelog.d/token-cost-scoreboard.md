<!-- section: Added -->
- `cargo xtask token-cost` now measures exact stdout bytes, finding/result counts, and bytes-per-finding for `ripr check` in json, json-compact, sarif, human, and human-full formats on the dogfood fixtures, writing an advisory `target/ripr/reports/token-cost.{json,md}` scoreboard so output token cost is visible in review instead of only in agent context windows.

<!-- section: Fixed -->
- Upgrading from 0.10: `ripr receipt check --gap` now finds the receipt 0.10
  wrote for an ordinary Rust gap id. The id embeds the file path
  (`gap:rust:src/lib.rs:...`), so 0.10 stored the receipt in nested
  directories under `target/ripr/receipts/`, and the lookup only handled an
  id without `/`. The check now reports that the receipt predates HEAD
  binding and prints the `ripr receipt write` command that replaces it,
  instead of saying no receipt exists. The lookup stays inside
  `target/ripr/receipts/` and does not follow a link out of it. When no
  receipt is found, the message prints the file name ripr looked for (it
  printed `%` escaped twice) and names `--path` (#15899).

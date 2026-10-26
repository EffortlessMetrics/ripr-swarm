<!-- section: Fixed -->
- `ripr agent` value flags now refuse a repeat (`agent <command> takes <flag>
  once`) instead of silently keeping the last value: `--root` on every
  subcommand, verify `--before/--after/--gap-id`, `--seam-id` on
  start/card/packet/receipt/repair, repair `--gap-id/--phase/--base/--attempt/
  --store/--edit-authority/--verify-authority/--python-repair-trust-*`,
  `start --out`, `brief --max-seams`, verify-execute
  `--packet/--result-json/--cancel-after-ms`, receipt
  `--attempt/--test/--verify-json/--out`, status `--out/--store/--attempt`,
  and packet `--gap-ledger/--gap-id`. `stub --kind` now routes through the
  same helper; `receipt --command` stays repeatable by design.
  `agent verify --gap-id ""` is refused like `repair` and `packet`,
  `agent status --out` accepts lexically equivalent spellings of the default
  directory (such as `target/ripr/other/../workflow`) while `../` escapes stay
  refused, and the `--max-seams`, `--cancel-after-ms` and `--at` errors no
  longer leak std integer-parse text (#10542).

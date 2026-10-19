<!-- section: Fixed -->
- `ripr agent stub` now names the effective cargo test target in its run
  command: a `[[test]]` entry that renames `tests/discount.rs` to `pricing`
  prints `--test pricing`, not the file stem. When the target name cannot be
  established (for example `autotests = false` without an explicit entry, an edition-2015 package whose explicit `[[test]]` entries disable auto-discovery, or `harness = false`),
  `run_command` is `null` instead of a command cargo would reject (#8794).

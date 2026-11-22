<!-- section: Fixed -->
- The `[languages] enabled` list `ripr check` prints when a preview language is
  off starts from the list `ripr.toml` already sets, as the `ripr doctor` tip
  does. Before, it was rebuilt from the files in the diff: a list that left
  `rust` out got `rust` added back whenever the root had a `Cargo.toml`, and a
  preview language the list enabled was dropped when the diff had no file of
  that language, so pasting the snippet changed what was analyzed (#15989).

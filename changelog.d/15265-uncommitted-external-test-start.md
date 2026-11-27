<!-- section: Fixed -->
- `ripr repair` now refuses to start when the test file it would add to has
  uncommitted changes, with the same `start_refused_uncommitted_target`
  refusal (exit 3) as an uncommitted inline test module. It names the file to
  commit or stash. Before, the start failed later with an untyped error and
  empty `--json` output (#15265).
- An uncommitted-target start refusal from `ripr agent repair --phase before`
  now names that advanced command, with its `--store`, as the command to rerun,
  instead of the `ripr repair` façade that would use the default store (#15265).

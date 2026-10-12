<!-- section: Changed -->
- `command_spec_sha256` no longer hashes the command's human display.
  Since #3948 the display names the concrete checkout root, so
  equivalent checkouts got different command identities. The digest now
  covers every typed field except the display (it is serialized with an
  empty `human_display`), so changing argv, cwd, policies or expected
  writes still changes it. Digest values therefore differ from earlier
  builds. A display whose `--root` was swapped for another absolute
  directory still recovers no typed spec (#3999).

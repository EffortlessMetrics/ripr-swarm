<!-- section: Fixed -->
- `ripr gate check` flag typos now suggest the nearest `gate check` flag, and
  `ripr gate evaluate` no longer suggests `--base` or `--head`, which only
  `gate check` accepts. `ripr help --json` lists flags for `gate check` and
  `gate evaluate`, and reports the gate `--mode` and `--root` as taking a
  value (#14086).

<!-- section: Fixed -->
- `ripr context` now refuses a missing, non-directory or empty `--root` with
  the shared `repository_root_unusable` refusal (exit 2), the same identity and
  repair route that `check`, `diff`, `rerun` and the agent commands use. It
  previously reported `analysis_failed`, or, with `--from`, an artifact
  failure (#3920).

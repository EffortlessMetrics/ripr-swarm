<!-- section: Added -->
- `ripr context` refusals now have a published schema,
  `schemas/ripr/context-refusal.schema.json`: the shared refusal envelope keyed
  with the context packet's `schema_version` `1.1`, carrying the run's `mode`
  and the caller's `--base`. It is tested against the real command's output,
  including `--format json-compact` (#3920).

<!-- section: Fixed -->
- `ripr context` with a missing or non-directory `--root` now refuses as
  `repository_root_unusable`, as `check`, `diff`, `rerun` and the agent
  commands do, instead of `analysis_failed` from the later config load (#3920).

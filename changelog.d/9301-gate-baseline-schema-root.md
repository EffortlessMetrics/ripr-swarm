<!-- section: Fixed -->
- `ripr gate` now refuses a baseline that records an unsupported `schema_version` (for example `99.0`) or a top-level or per-entry `root` naming another repository, matching `ripr baseline diff`, instead of silently accepting it. Baselines that omit these fields stay accepted (#9301).

<!-- section: Fixed -->
- `ripr gate` now refuses a baseline that records an unsupported `schema_version` (for example `99.0`), the same refusal `ripr baseline diff` applies. An absent `schema_version` stays accepted (#9301).
- `ripr gate` also refuses, as a gate-only contract, a baseline `root` (top-level or on any indexed item) that is a non-string value or an absolute path differing from the gate's absolute root. Absent, `null`, blank and relative roots stay accepted as unknown (#9301).

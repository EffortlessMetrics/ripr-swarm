<!-- section: Fixed -->
- `ripr check --format json` `summary.changed_files_by_language` now lists the changed files of every routed language, including TypeScript, JavaScript and Python files whose adapter is not enabled, so it agrees with `analysis_outcome` counts instead of reporting only a zero Rust entry (#10719).

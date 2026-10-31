<!-- section: Changed -->
- A human `ripr check` in a non-Cargo project (Python, TypeScript) whose git ignore rules do not cover `target/ripr` now prints one stderr line on each run naming the exact `.gitignore` entry to add; it stays silent once the rule exists, for Cargo projects, under `--quiet` or `RIPR_QUIET` and for machine formats, and never edits `.gitignore` (#11879).

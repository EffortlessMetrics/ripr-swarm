<!-- section: Fixed -->
- `ripr doctor` run from a subdirectory without `--root` now diagnoses the
  root `ripr check` would analyze from there: the enclosing workspace,
  package or Git top level. It prints the same "resolved workspace root"
  disclosure as check. Before, a run from `py/` inside a Rust workspace
  detected Python alone, printed `Root: .`, and suggested
  `enabled = ["python"]`, which drops Rust. `--root .` still diagnoses the
  current directory alone (#15304).
- The `enabled` tip keeps `rust` when `ripr.toml` lists it in
  `[languages].enabled`, even where no Rust marker is found. Only the
  built-in default `rust` is left out for a repository with no Rust (#15304).

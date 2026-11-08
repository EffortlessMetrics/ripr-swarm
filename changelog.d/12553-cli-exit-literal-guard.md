<!-- section: Internal / CI -->
- `cargo xtask check-cli-exit-contract` pins numeric `process::exit` literals in the CLI control paths to counted entries in `policy/cli_exit_literal_allowlist.toml`, and `check-process-policy` now runs it, so an undocumented exit code fails the enforced gate (#12505, #12553).

<!-- section: Fixed -->
- `cargo xtask dx-scoreboard` still loads a mutation spot-check receipt
  against a `--config` written before the generated recall rows existed: the
  three #12900 rows are projected only when the configuration declares them,
  and a malformed receipt section now fails with a message naming the section
  (#12900).

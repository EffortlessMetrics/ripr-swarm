<!-- section: Fixed -->
- `ripr check --format github`, `sarif`, `sarif-compact`, `repo-sarif` and
  `repo-sarif-compact` now name each file relative to the Git checkout top
  level, the base GitHub annotations and code scanning resolve paths against.
  A `--root` below the top level (an absolute `crates/app` path, `--root .`
  inside the member, or a `..` spelling) used to emit `src/lib.rs`, so the
  annotation or alert landed on a missing file. Runs rooted at the top level
  are unchanged. For runs rooted below the top level the SARIF
  `riprFingerprintV1` value embeds the corrected path, so those code-scanning
  alerts close and reopen once on the first upload after upgrading (#15367).

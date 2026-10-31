<!-- section: Changed -->
- The generated `ripr init --ci github` workflow's `Install ripr` step now
  installs the release's Windows x64 zip on Windows runners instead of building
  with `cargo install`: it verifies the published SHA-256 before extracting,
  extracts into `$RUNNER_TEMP/ripr-bin` with the `tar.exe` Windows ships, runs
  `ripr.exe` by absolute path and requires it to report the pinned version.
  A missing or mismatched checksum, a malformed or unsafe archive or a wrong
  payload now fails the step on every platform instead of falling back to
  `cargo install`, and Windows ARM64 reports an explicit unsupported error.
  Regenerate the workflow to pick this up (#6245).

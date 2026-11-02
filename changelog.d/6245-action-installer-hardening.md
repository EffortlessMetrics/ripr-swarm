<!-- section: Changed -->
- The `.github/actions/ripr-check` composite Action's `Install ripr` step now
  follows the same installer contract as the generated workflow: Windows x64
  installs the release's zip (verified SHA-256, flat-root listing check, the
  `tar.exe` Windows ships, `ripr.exe` run by absolute path), Windows ARM64
  reports an explicit unsupported error, a missing or mismatched checksum, bad
  archive or wrong payload exits 1 with no `cargo install` fallback, and the
  installed `ripr` must report exactly the requested version (a look-alike such
  as `0.10.00` or `0.10.0-rc.1` is refused). The already-on-PATH shortcut and
  `cargo install` for an unmapped runner or an undownloadable archive are
  unchanged (#6245).

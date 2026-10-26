<!-- section: Fixed -->
- `ripr help --all` now rejects trailing arguments with a usage error (exit 2) instead of silently discarding them, and `--help --all` on commands other than `check` exits 2 naming the supported routes (`ripr help --all`, or `ripr check --help --all` for check) instead of printing ordinary help while ignoring the flag (#9867).

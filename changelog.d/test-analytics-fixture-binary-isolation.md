<!-- section: Internal / CI -->
- Test Analytics now sets `RIPR_FIXTURE_BINARY_ISOLATE=1`, so the fixture corpus builds its default-feature `ripr` in `target/fixture-variants/default` instead of relinking the `--all-features` `target/debug/ripr` the nextest suite is executing. The required lane leaves the variable unset and is unchanged (#11602).

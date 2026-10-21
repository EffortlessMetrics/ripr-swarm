<!-- section: Fixed -->
- `ripr doctor` now marks advisories (artifact roots not covered by git ignore rules or tracked by git, unwritable cache) with `~` instead of the blocking `!`, so only blocking failures (failed core checks and required runtime-probe failures) use `!` and fail the run; `ripr doctor --help` documents the two marks (#9033).

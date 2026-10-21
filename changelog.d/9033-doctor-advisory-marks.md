<!-- section: Fixed -->
- `ripr doctor` now marks advisories (artifact roots not covered by git ignore rules or tracked by git, unwritable cache) with `~` instead of the blocking `!`, so only failing core checks use `!` and fail the run; `ripr doctor --help` documents the two marks (#9033).

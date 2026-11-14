<!-- section: Fixed -->
- The first-`check` `.gitignore` note now probes a `RIPR_CACHE_DIR` base relocated inside the repository (the roots `ripr doctor` probes), instead of only the default `target/ripr`, so an uncovered relocated cache is named and a covered one stays silent (#11879, #11937).

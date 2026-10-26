<!-- section: Fixed -->
- `ripr --help` (and `ripr help`) now names the exit-code vocabulary — 0 completed, 2 could not complete, 3 blocking decision or typed refusal, 130 interrupted standalone check — with a pointer to the full contract in `docs/EXIT_CODES.md`, and documents the global `-v`/`--verbose` flag that previously appeared only in the `help --all` preamble (#9865).

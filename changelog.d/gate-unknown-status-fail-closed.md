<!-- section: Fixed -->
- `ripr gate evaluate` now fails closed on an unknown gate status (exit 2, could-not-complete) instead of silently exiting 0; only `pass`, `advisory`, and `acknowledged` pass, while `blocked` keeps exit 3.

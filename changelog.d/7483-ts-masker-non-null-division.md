<!-- section: Fixed -->
- TypeScript: the shared literal masker no longer reads the `/` in `n! / 2` (a non-null assertion followed by division) as a regex start, so a later binding on the same line is no longer hidden from name scans (#7483).

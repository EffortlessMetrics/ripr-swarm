<!-- section: Added -->
- `ripr agent repair --phase after` finishes a gap-bound TypeScript attempt: it re-runs the check against the commit the base resolved to, reports the gap closed only when it was open before and reads `exposed` after (absence is never closure), enforces the edit cage, and the attempt's `next_command` is now the executable `--phase after` command (#9103).

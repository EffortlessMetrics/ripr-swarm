<!-- section: Changed -->
- `ripr check` no longer re-runs the whole-file nesting estimate each time it reads a file whose parse it already holds: a held parse remembers that its text passed the estimate. On ripr's own self-check the estimate was about 6% of analysis time, almost all repeats on large files (#15396).

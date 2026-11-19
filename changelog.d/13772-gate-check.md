<!-- section: Added -->
- `ripr gate check --base REV` is a one-command CI gate. It runs `review-comments`, then `gate evaluate` (acknowledgeable by default) in one process, and exits 0 for pass, 3 for blocked and 2 when it could not complete. A failed producer run is a config_error, never a pass over stale guidance (#13772)

<!-- section: Fixed -->
- `ripr assistant-loop health` now prints a stderr warning for each `--proof` it could not use, naming the path and the cause (the file is not found, is not JSON, or lacks seam and movement context) and the producer command. Before, a mistyped path printed only `Proofs: 1` and the report blamed missing seam evidence (#14371).
- The `ripr assistant-loop health` Markdown repair line now repeats that cause instead of always asking for seam and movement context (#14371).
- `ripr pr-review front-panel` now prints a stderr warning for each explicitly passed input that is missing, unreadable, or not JSON, naming the input and path. A missing file reads "not found" instead of an OS error code (#14371).

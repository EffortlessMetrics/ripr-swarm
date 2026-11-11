<!-- section: Fixed -->
- `ripr assistant-loop health` now prints a stderr warning for each `--proof` it could not use, naming the path and the cause (the file is not found, is not JSON, or lacks seam and movement context) and the producer command. Before, a mistyped path printed only `Proofs: 1` and the report blamed missing seam evidence (#14371).

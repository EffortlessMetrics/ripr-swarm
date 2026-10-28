<!-- section: Added -->
- `ripr pilot -v` prints a stage table on stderr after a complete run: elapsed milliseconds and an item count for loading the change, the seam inventory, change classification, the seam budget, the language scan and the artifact writes, plus the total. Stdout and the pilot artifacts are unchanged, so they stay byte-identical across runs (#10727).

<!-- section: Fixed -->
- LSP diagnostic `data.confidence` (and the witness confidence value) is rounded to the two-decimal contract precision that `ripr check` prints, so the same finding reads `0.4` in the editor instead of `0.4000000059604645`. The rounding lives in one shared helper used by every LSP producer of the value (#10717).

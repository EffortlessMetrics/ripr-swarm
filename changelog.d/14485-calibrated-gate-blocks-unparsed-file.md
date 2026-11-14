<!-- section: Changed -->
- `ripr gate evaluate --mode calibrated-gate` now blocks (status `blocked`,
  exit `3`) when a changed file could not be parsed, because no seam was
  judged for it. Other modes keep the `advisory` warning added in #14082
  (#14485).

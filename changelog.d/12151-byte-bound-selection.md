<!-- section: Fixed -->
- The byte-bounded pilot `repo-exposure.json` now skips and counts a single oversized seam instead of
  dropping every later seam (#12151), keeps seams past the current change by pilot rank rather than
  inventory order (#12152), and names the population of its `metrics` with the true total (#12153).

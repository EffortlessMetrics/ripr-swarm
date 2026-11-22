<!-- section: Fixed -->
- Python preview also reports unknown infection for the loop selector seeded
  with `best = -math.inf` (or `math.inf` with `<`) when the module imports
  `math` once and never rebinds it, and the related tests pin literal results
  only (#12954).

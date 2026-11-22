<!-- section: Fixed -->
- Python preview also reports unknown infection for the loop-selection case
  written as `best = xs[0]` followed by `for x in xs:`, not only `for x in xs[1:]:`,
  when the related tests pin integer results only (#12954).

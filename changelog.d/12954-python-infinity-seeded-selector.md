<!-- section: Fixed -->
- Python preview also reports unknown infection for the infinity-seeded loop
  selector (`best = float("-inf")`, `for x in xs:`, `if x > best:`, or `"inf"`
  with `<`) when the related tests pin integer results only (#12954).

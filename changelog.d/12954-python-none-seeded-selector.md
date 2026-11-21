<!-- section: Fixed -->
- Python preview also reports unknown infection for the `None`-seeded loop
  selector (`best = None`, `for x in xs:`, `if best is None or x > best:`) when
  the related tests pin integer results only (#12954).

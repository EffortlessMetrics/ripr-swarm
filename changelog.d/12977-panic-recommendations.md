<!-- section: Fixed -->
- `ripr pilot` and `ripr agent brief` withhold seams whose `#[should_panic]`
  outcome remains unresolved, matching repair eligibility. Pilot counts the
  withheld limitations, and evidence records preserve their named reason and
  analyzer repair route without changing the seam class or stage states (#12977).

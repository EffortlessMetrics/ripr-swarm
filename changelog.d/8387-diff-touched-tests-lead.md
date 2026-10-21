<!-- section: Changed -->
- Related tests that the diff itself adds or changes now lead their confidence
  tier in `ripr check` output, ahead of name order, because the author wrote them
  for this change. The ordering never admits a test or lifts one into a higher
  confidence tier, so classifications do not change (#8387).

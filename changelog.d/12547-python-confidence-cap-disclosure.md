<!-- section: Changed -->
- Python preview findings now carry a per-finding `confidence_basis` and
  `confidence_why_capped` explanation, derived in the classifier at parity with
  TypeScript, so human and JSON output explain the capped score; classifications
  and confidence values are unchanged (#12547).

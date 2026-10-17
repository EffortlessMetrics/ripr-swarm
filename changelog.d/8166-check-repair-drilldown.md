<!-- section: Fixed -->
- `ripr check` (bounded human output) now prints a `ripr repair <seam-id>`
  next step in the top-finding drill-down when that finding's seam passes
  the same repair-eligibility gate `ripr repair` applies, naming the seam
  identity the repair resolver accepts instead of dead-ending at
  `ripr explain` / `ripr context`; ineligible, ambiguous, and
  truncated-inventory findings print no repair line (#8166).

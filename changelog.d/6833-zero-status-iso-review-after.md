<!-- section: Fixed -->
- `ripr zero status` now compares `YYYY-MM-DD` (and RFC3339) baseline
  `review_after` values with the UTC run date, and classifies an incomparable
  deadline as `unknown` instead of silently `current` (#6833).

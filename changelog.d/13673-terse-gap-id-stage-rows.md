<!-- section: Changed -->
- `ripr check --format json-terse` also omits each finding's `canonical_gap_id` (same as `canonical_gap.id`) and basis-free `evidence_stages` rows (restatable from `ripr`), listing them in `dropped_aliases`; the 5-gap fixture shrinks about 6% more and full `json` is unchanged (#13673).

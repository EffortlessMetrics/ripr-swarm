<!-- section: Fixed -->
- `ripr repair <seam-id|gap-id>` now refuses an explicit seam that is not
  repair-eligible (or is configured off) with a decision exit and the named
  reason, before creating any repair attempt or before-phase lock, matching
  the gate the implicit selector already applied (#8166).

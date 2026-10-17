<!-- section: Fixed -->
- `ripr check --format repo-exposure-json` no longer rebuilds the owner-pin
  syntax cache for every seam, so the repository seam walk pays its
  full-index macro-site pass once per worker thread instead of once per seam
  and can finish on large repositories (#8282).

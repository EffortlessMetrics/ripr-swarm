<!-- section: Fixed -->
- The shared seam-finding join no longer credits every twin seam of one
  function with one finding's witness: the producer-structured branch now
  requires the finding's probe line to fall on the seam (inside its recorded
  span, else exactly its display line), so only the seam at the changed line
  binds and the unchanged twin binds nothing (#7177).

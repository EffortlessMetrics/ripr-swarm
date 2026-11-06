<!-- section: Fixed -->
- A seam-evidence walk whose internal shared-table build failed is no longer
  stored in the classified-seam cache or the diff-scoped result cache; the next
  run recomputes real evidence instead of replaying the fail-closed result.
  Fail-closed entries already written by earlier builds are rejected on load and
  recomputed, so they no longer replay as complete (#13340).

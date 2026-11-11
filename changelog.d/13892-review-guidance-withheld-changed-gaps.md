<!-- section: Fixed -->
- `ripr review-comments`: a gap seam on a changed line or in a changed owner
  that is withheld as a static limitation (for example an owner no test calls
  whose boundary operand is computed, which `ripr check` lists as
  `no_static_path`) is now itemized in `suppressed` with reason
  `static_limitation`, its static class and the named limitation, instead of
  only a warning string. `ripr gate evaluate --pr-guidance` now evaluates it
  and lists it with its file and line under `## Suppressed` in
  `gate-decision.md`. It stays non-blocking: the gate still exits 0, but its
  status reads `advisory` rather than `pass` when such a gap is the only
  finding. The PR review front panel keeps an actionable card as the top
  issue over a suppressed or withheld entry (#13892).

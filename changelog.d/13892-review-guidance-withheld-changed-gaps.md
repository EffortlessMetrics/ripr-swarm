<!-- section: Fixed -->
- `ripr review-comments`: a gap seam on a changed line or in a changed owner
  that is withheld as a static limitation (for example an owner no test calls
  whose boundary operand is computed, which `ripr check` lists as
  `no_static_path`) is now itemized in `suppressed` with reason
  `static_limitation` and the named limitation, instead of only a warning
  string. `ripr gate evaluate --pr-guidance` now evaluates it and names it in
  `gate-decision.md`; it stays non-blocking (#13892).

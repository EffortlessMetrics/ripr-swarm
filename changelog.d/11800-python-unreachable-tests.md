<!-- section: Fixed -->
- Python tests that can never run no longer credit `exposed`: a test def
  rebound later in the same scope, and assertions after a definite
  `return`/`raise`/skip/fail/exit or in a loop over an empty literal (#11800).

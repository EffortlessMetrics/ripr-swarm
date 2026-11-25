<!-- section: Fixed -->
- Python: a removed line no longer disappears when the diff pairs it with an
  added comment that sits beside replacement code. When every code line of
  the added run already has its own counterpart, the comment replaced
  nothing, so the removed line keeps its `base_deleted` record naming the
  removed code (#16011).

<!-- section: Fixed -->
- Interactive stderr progress finishes the held-open TTY line before a
  mid-run notice, so the notice is not glued onto the progress row and is
  not left as a stale suffix after a later overwrite (#7439).

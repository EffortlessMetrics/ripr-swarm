<!-- section: Fixed -->
- After `ripr continue` refuses an attempt because production code was
  edited, the recovery no longer says a new attempt is not offered. The start
  was held only by that same uncommitted edit, so the line now says to undo it
  and names the `ripr repair` command that asks again (#14271).

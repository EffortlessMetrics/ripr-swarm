<!-- section: Fixed -->
- A pure-deletion hunk (`+T,0`, which every `--unified=0` deletion block produces) now anchors each removed line at the next new-side line instead of one line above it (or at line 0), matching hunks that carry context, so pilot current-change, diff-touched and task-first locate a deletion where the removed code was (#13098).

<!-- section: Fixed -->
- `ripr repair` with visible seams but none repair-eligible no longer ends on `ripr pilot` alone: its next step also names `ripr check`, which points at a changed-code gap to test by hand, so the first-run loop in `ripr --help` has no dead end (#8158).

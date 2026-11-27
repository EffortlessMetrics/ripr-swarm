<!-- section: Fixed -->
- On a clean checkout of the default branch, `ripr repair`, `ripr agent stub`
  and `ripr pilot` now read the last commit, the same range bare `ripr check`
  analyzes there, instead of an empty range. A change committed on `main` is no
  longer called "already checked" by repair, stub or pilot while `ripr check`
  reports a gap in it. An explicit `--base` still names its own range, and a
  root commit still has nothing to compare against. Pilot's "could not
  confirm" sentence now names the same `ripr check --root <root>` command as
  its closing line (#16082).

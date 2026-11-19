<!-- section: Fixed -->
- `ripr check --root <subdirectory>` no longer prints a `ripr agent repair` / `ripr repair` start that exits 2 with
  "not the exact Git top-level": under a root below the Git top-level every format (human, agent, JSON) now states that a repair
  starts only at the repository's top-level, and `ripr agent repair` refuses such a root before creating anything under it (#15363).

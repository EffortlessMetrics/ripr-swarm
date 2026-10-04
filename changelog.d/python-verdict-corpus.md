<!-- section: Added -->
- Python verdict corpus: `fixtures/python-verdict-corpus` holds 61 authored
  cases across pytest, pytest fixtures, unittest and Hypothesis, including
  one case for each runnable RIPR-SPEC-0233 acceptance example, each labeled
  by running its mutants against the project's own tests.
  `cargo xtask verdict-corpus check --language python` scores them: today
  15/33 false actionable, 6/28 false exposed, 0/28 false silent. The DX
  scoreboard gates those three rates against the committed baseline.

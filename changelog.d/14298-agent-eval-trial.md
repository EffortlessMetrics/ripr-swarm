<!-- section: Internal / CI -->
- `cargo xtask agent-eval trial-setup <dir>` writes the gap scenarios as fresh repositories for a real agent, and `trial-score <dir>` judges the tests it added by reverting each keyed line; a catch needs a compiled, failing run, and unmeasurable runs leave the score incomplete and exit nonzero (#14298).

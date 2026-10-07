<!-- section: Changed -->
- The verdict corpus no longer commits `expected/summary.json`. Each
  expected row records its own contradiction counts, and the summary is
  derived from the rows wherever it is read: `verdict-corpus check`, the
  dx scoreboard's trust rates (new `verdict-corpus:` source) and the
  public proof receipt. Corpus case PRs now add only their own case and
  row files and leave `docs/PUBLIC_PROOF.md` to a separate refresh, so
  parallel case PRs no longer conflict on shared generated lines (#6660).

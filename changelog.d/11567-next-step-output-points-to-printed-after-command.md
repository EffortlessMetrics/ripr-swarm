<!-- section: Fixed -->
- Next-step output that follows a repair start now points to the after-phase
  command the `ripr agent repair --phase before` summary actually prints, instead
  of spelling out an `--attempt ... --phase after` command. That summary prints
  `ripr continue --attempt <id>` for an ordinary attempt and keeps the advanced
  form with `--test-result FILE` for an attempt that follows a new-test proposal,
  so the generic step no longer names either spelling. This covers the after-phase
  step in `start-here.md`, `first-useful-action.md`, `gate-decision.md`, the PR
  review front panel, `pr-summary` and the CI job summary, the "no receipt yet" line
  of the agent review summary, the after-test-edit note of `ripr agent status`, the
  receipt recovery sentence, and the PR inline comment (#11567).

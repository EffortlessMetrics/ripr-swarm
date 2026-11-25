<!-- section: Fixed -->
- Next-step output that follows a repair start now names the command the plain
  `ripr agent repair --phase before` summary prints (`continue --attempt`),
  instead of an `--attempt ... --phase after` command that summary never shows. This
  covers the after-phase step in `start-here.md`, `first-useful-action.md`,
  `gate-decision.md`, the PR review front panel, `pr-summary` and the CI job summary,
  the "no receipt yet" line of the agent review summary, the after-test-edit note of
  `ripr agent status`, the receipt recovery sentence, and the PR inline comment (#11567).

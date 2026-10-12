<!-- section: Fixed -->
- The generated-PR CI dogfood scenario checks Start-here, repair commands,
  gate authority, and language grouping in `ripr reports ci-summary` output,
  and keeps job-level `continue-on-error`, summary-step `if: always()` /
  `continue-on-error: true` with the `ci-summary` invoke in that step, and
  artifact-upload wiring on the workflow. The family receipt names both
  `init --ci github --dry-run` and `reports ci-summary --base-ref main`.
  The advisory-summary invoke must be an executable `run` line, not a
  quoted echo, comment, or a line after a whole-line `exit` / `exit <status>`.
  The generated install-failure `echo …; exit 1` stays a different line. The
  Dogfood Report example in `docs/OUTPUT_SCHEMA.md` names both producers
  (#6958).

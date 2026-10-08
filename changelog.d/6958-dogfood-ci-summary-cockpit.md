<!-- section: Fixed -->
- The generated-PR CI dogfood scenario checks Start-here, repair commands,
  gate authority, and language grouping in `ripr reports ci-summary` output,
  and keeps job-level `continue-on-error`, summary-step `if: always()` /
  `continue-on-error: true` with the `ci-summary` invoke in that step, and
  artifact-upload wiring on the workflow
  (#6958).

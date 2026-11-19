<!-- section: Fixed -->
- The workflow docs (`FIRST_PR_WORKFLOW`, `TARGETED_TEST_WORKFLOW`, `REPAIR_ATTEMPT`,
  `AGENT_DISPATCH_WORKFLOW`, `CI`, `INSTALLATION_VERIFICATION`, `OUTPUT_SCHEMA`) now name
  the command the plain `ripr agent repair --phase before` summary prints,
  `ripr continue --root <root> --attempt <repair-attempt-id>`, instead of a
  `--phase after` command that only the `--json` `repair_attempt.next_command`
  carries; a test keeps that wording from drifting back (#11567).

<!-- section: Fixed -->
- Editor: with the full diagnostic profile, after a save that ran the quick
  analysis (repair spots skipped),
  "Start repair: copy repair command" is listed disabled on a gap finding with
  the next step, "run ripr: Refresh Full Analysis", instead of being missing.
  "ripr: Start Current Repair" shows that reason from a finding diagnostic
  instead of "No current ripr repair gap". When a saved test makes a gap
  `exposed`, the analysis status now names it ("now exposed: a test appears to
  observe the changed behavior"), and VS Code shows that once rather than the
  diagnostic just vanishing. An `exposed` Python or TypeScript finding is no
  longer reported as a preview finding "not shown in the editor" (#13636).

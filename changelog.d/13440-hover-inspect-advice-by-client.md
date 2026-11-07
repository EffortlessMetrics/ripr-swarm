<!-- section: Fixed -->
- LSP: the line hover for findings the `actionable` profile does not publish
  no longer promises the "Inspect finding" quick fix to every client. The
  promise is derived from the same negotiated client-command decision that
  gates the code action, so a generic client without the `ripr.copyContext`
  command is told only that no executable Inspect finding quick fix is
  offered; related-test navigation is unaffected and no other action claim is
  made. No actionability or repair data is added (#13440).

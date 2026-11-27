<!-- section: Fixed -->
- The VS Code extension now resets an open crash-recovery circuit on every
  intentional restart (configuration change, active-editor root switch,
  workspace trust grant), re-resolves the server when removing a folder leaves
  an ambiguous multi-root window with one root, and `ripr: Show Top Limitation`
  reports that the server is not running (with the restart route) instead of
  implying an analysis snapshot is pending, with human-readable wording (#15303).

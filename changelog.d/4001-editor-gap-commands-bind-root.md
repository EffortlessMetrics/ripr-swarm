<!-- section: Fixed -->
- Editor gap verify and receipt commands now name the selected workspace.
  Gap artifacts record portable `--root .` routes, and the language server
  used to pass them through to diagnostics data, hover, code actions and the
  workspace status packet, so a command copied from the editor analyzed
  whatever directory it was pasted into. These surfaces now bind `--root`
  to the selected workspace and anchor relative redirect targets under it,
  while equivalent checkouts keep one diagnostic identity. The editor cockpit
  and first-run fixtures render `--root <root>`, and the fixture contract
  rejects `--root .` in editor projections (#4001).

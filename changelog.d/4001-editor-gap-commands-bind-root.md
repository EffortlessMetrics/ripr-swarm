<!-- section: Fixed -->
- Editor gap verify and receipt commands now name the selected workspace.
  Gap artifacts record portable `--root .` routes, and the language server
  used to pass them through to diagnostics data, hover, code actions and the
  workspace status packet, so a command copied from the editor analyzed
  whatever directory it was pasted into. Those four surfaces now bind
  `--root` to the selected workspace and anchor relative redirect targets
  under it. A route the server cannot bind safely is withheld instead of
  shown portable. Equivalent checkouts keep one diagnostic identity, whether
  or not their paths need shell quoting. The VS Code client accepts the bound
  gap commands for its copy actions. The editor gap cockpit and first-run
  fixtures render `--root <root>`, and their fixture contract rejects
  `--root .` (#4001).

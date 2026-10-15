<!-- section: Internal / CI -->
- VS Code test host failures now keep a size-capped, redacted log snapshot
  from the pinned `target/ripr/vscode-test-user-data/logs` folder, and the
  vscode CI job uploads it only on failure with 5-day retention (#7987).

<!-- section: Fixed -->
- The VS Code status bar now says `ripr: server failed` when a resolved
  language server dies at `client.start()`, and keeps `ripr: server missing`
  only for resolution failures where no binary was found (#8587).

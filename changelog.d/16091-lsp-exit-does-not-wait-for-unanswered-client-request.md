<!-- section: Fixed -->
- `ripr lsp` no longer waits for an answer that cannot arrive when the editor
  closes while a request from the server to the editor (such as the settings
  pull) is still open. That wait kept the process alive for about 28 seconds
  after `exit` or a closed input (#16091).

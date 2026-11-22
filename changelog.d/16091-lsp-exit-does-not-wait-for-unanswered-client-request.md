<!-- section: Fixed -->
- `ripr lsp` now exits at once when the editor closes while a request from
  the server to the editor (such as the settings pull) is still unanswered.
  It used to stay alive for about 28 seconds after `exit` or a closed input
  (#16091).

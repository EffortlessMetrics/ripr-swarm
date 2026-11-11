<!-- section: Fixed -->
- LSP: in Neovim and other generic pull-diagnostic clients, a save or
  `ripr.refresh` now updates the open file's diagnostics. ripr advertised
  workspace diagnostics to every pull client, and Neovim 0.12 answers
  `workspace/diagnostic/refresh` with a workspace pull only and ignores that
  report for buffers it pulls by document, so the open file kept its first
  (often "not analyzed yet") state. Generic pull clients are now offered
  document pull only and re-pull each open buffer on refresh; the ripr VS Code
  extension keeps workspace pull (#1630).

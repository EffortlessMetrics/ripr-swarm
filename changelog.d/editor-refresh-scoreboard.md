<!-- section: Changed -->
- The speed scoreboard times an edited-save refresh through `ripr lsp --stdio`
  per corpus repository, once with the client answering progress creation and
  once never answering it, so a refresh that waits on the client shows up as a
  regression instead of as slow analysis (#5317, #9149).

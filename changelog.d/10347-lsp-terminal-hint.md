<!-- section: Fixed -->

- `ripr lsp --stdio` started from an interactive terminal now prints one stderr
  hint that it is waiting for an editor instead of looking hung, and the
  `ripr --help` editor line says the editor starts it. Nothing is printed when
  stdin is a pipe or file, and stdout stays LSP framing only (#10347).

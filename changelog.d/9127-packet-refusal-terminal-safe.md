<!-- section: Fixed -->
- `ripr agent packet --json` refusal documents now go through the same
  terminal-safe stdout writer as `agent card` and `agent stub`: control and
  bidi characters are spelled, a trailing newline is kept, and the write is
  chunked. Exit 3 still means the document was delivered; exit 2 still means
  render or write failed (#9127).

<!-- section: Fixed -->
- Python boundary call-row excerpts now use the exact call extent (including
  whitespace before the parenthesis), keep a multibyte-aligned 238-240 byte call
  whole, and no longer emit a leading `...` when only indentation was cut (#8788).

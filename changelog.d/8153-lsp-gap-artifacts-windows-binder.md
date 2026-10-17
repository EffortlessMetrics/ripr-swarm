<!-- section: Fixed -->
- The experimental LSP gap-artifact command binder now quotes and reads
  producer commands with the host's shell family: Windows console form
  (double quotes, literal backslashes, `""` interior doubling) reuses the
  shared recovery parser, so a spaced root or `items.len()` identity no
  longer falls back to an unbound `--root .`. Bash single-quote behavior
  is unchanged (#8153).

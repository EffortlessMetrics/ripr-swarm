<!-- section: Fixed -->

- The MCP `tools/list` payload no longer repeats every tool's `title` inside
  `annotations.title` (eight duplicate copies, ~232 wire bytes per session
  start), and a pinned byte-budget guard now caps the payload so the
  per-session context sink cannot grow back unnoticed. Measured live:
  46,048 -> 45,816 bytes (#9852).
- Evaluated and rejected with recorded arithmetic: factoring repeated
  `outputSchema` subschemas through `$defs`/`$ref` cannot pay within one
  schema document (best case ~75 bytes across all nine tools), and the large
  cross-tool duplication (the shared typed-failure branch, ~7.3 KB) may not
  be factored because each advertised `outputSchema` must validate
  identically as a standalone schema document (#9852).

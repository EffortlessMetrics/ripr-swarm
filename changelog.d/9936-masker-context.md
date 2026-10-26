<!-- section: Fixed -->
- TypeScript runner admission keeps real writes after JSX closing tags and
  backtick-bearing regex literals visible without mistaking division or
  comparisons for regex or JSX syntax (#9936).
  Regex and JSX spans come from the OXC parser; call sites that pass no source
  kind (for example the CommonJS default gate and the mock-payload guards)
  intentionally keep the lexical fallback.

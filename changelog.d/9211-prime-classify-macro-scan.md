<!-- section: Changed -->
- `ripr check` on a diff that reaches assertion macros no longer leaves the
  classify workers idle while the first one parses the whole workspace for
  macro-binding sites. The scan now runs once, fanned out across the classify
  pool, before the probes are classified, so the other workers no longer sleep
  on the shared memo lock. Findings are unchanged (#9211).

<!-- section: Fixed -->
- `ripr check` no longer reports each TypeScript/JavaScript finding twice when `[languages].enabled` lists both `typescript` and `javascript`: the shared adapter result is merged once per adapter family, so retained findings, limitations and per-language file counts match the single-id run (#10013).

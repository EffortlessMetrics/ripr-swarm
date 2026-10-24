<!-- section: Fixed -->
- Repo-scope `ripr check` runs (no diff) no longer report each TypeScript/JavaScript finding twice when `[languages].enabled` lists both `typescript` and `javascript`: the shared adapter result is merged once per adapter family in either id order, so retained findings and per-language file counts match the single-id run. Completes the diff-mode fix (#10013).

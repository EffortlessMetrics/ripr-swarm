<!-- section: Fixed -->
- TypeScript: dropping or adding one literal field after a spread, such as `return { ...cfg, ok: true }` becoming `return { ...cfg }`, no longer reads `exposed` when every observed test input already carries `ok: true` or cannot be read. RIPR-SPEC-0246 rule 3 keeps the credit only when an observed input lacks the field or holds another value there (#7963).

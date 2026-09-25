# Fixture: typescript_deep_nesting_parse_budget

Spec: RIPR-SPEC-0027

## Given

A workspace enables the TypeScript preview adapter via `ripr.toml`:

```toml
[languages]
enabled = ["rust", "typescript"]
```

One undiscovered-by-the-diff production file (`src/deep.ts`) contains an
expression nested 500 parentheses deep — past any plausible hand-written or
generated nesting budget, and past the oxc recursive-descent stack budget on
a default main-thread stack. The file is NOT in the diff.

A second production file (`src/app.ts`) carries a benign one-line addition
that IS the diff.

## When

```bash
cargo xtask fixtures typescript_deep_nesting_parse_budget
```

or:

```bash
ripr check \
  --root fixtures/typescript_deep_nesting_parse_budget/input \
  --diff fixtures/typescript_deep_nesting_parse_budget/diff.patch \
  --mode fast
```

## Then

The run completes and reports normally (no `has overflowed its stack`
abort, no empty output):

- the deep file is skipped via the nesting budget with a typed
  `typescript parse budget exceeded` disclosure in the limitations;
- the benign `src/app.ts` change is analyzed exactly as without the deep
  file present (same probes, same classifications);
- `check.json` is valid with a `findings` field and `human.txt` is non-empty.

## Must Not

- Abort the process (stack overflow is not a panic the startup guard can
  catch; exit 127 / `0xC00000FD` with empty output is the bug).
- Parse the deep file anyway (no owner/test extraction from a file past
  the budget).
- Change editor routing, VS Code selectors, policy, gates, or defaults.
- Claim Rust-level maturity for the TypeScript preview evidence.

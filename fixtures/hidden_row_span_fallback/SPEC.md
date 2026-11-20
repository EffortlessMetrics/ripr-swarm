# Fixture: hidden_row_span_fallback

Spec: RIPR-SPEC-0122

## Given

A TypeScript preview workspace (non-Rust files never offer a per-finding test
stub, so omitted findings group into one `Hidden:` row). One function `check`
has twelve `if (field === "...") { throw new RangeError(...); }` blocks; the
diff changes each thrown error class (`Error` becomes `RangeError`) on twelve
non-adjacent lines. The only test is a smoke test that does not pin any error.

## When

```bash
cargo xtask fixtures hidden_row_span_fallback
```

or:

```bash
ripr check --root fixtures/hidden_row_span_fallback/input \
           --diff fixtures/hidden_row_span_fallback/diff.patch \
           --mode draft
```

## Then

- One finding is the start-here gap; the other eleven share file, owner,
  class, currentness and reason, so `Hidden:` prints them as one grouped row.
- The eleven lines form more than three separate runs, so the row falls back
  to the covering span with the distinct line count:
  `src/limits.ts:6-36 (11 distinct lines)`. The span covers 31 lines but only
  11 carry a member, and the row says 11.

## Must Not

- Print the span length (31) as the line count.
- List the lines as a bare `6-36` range that implies every line in between is
  a member.
- Use mutation-runtime outcome vocabulary reserved for real mutation execution.

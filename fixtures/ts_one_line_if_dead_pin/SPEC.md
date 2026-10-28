# Fixture: ts_one_line_if_dead_pin

Spec: RIPR-SPEC-0027

## Given

A TypeScript owner `pick(s, admin)` changes the one-line `if (s > 404) { return 1; }`
to `>=`. At the boundary input `pick(404, false)` the unchanged code returns `2` and the
changed code returns `1`. The only related test pins `pick(404, false)` to `3`, a value
neither version returns, so it fails on both and proves nothing about the edit.

The fixture workspace enables the TypeScript preview adapter via `ripr.toml`.

## When

```bash
ripr check \
  --root fixtures/ts_one_line_if_dead_pin/input \
  --diff fixtures/ts_one_line_if_dead_pin/diff.patch \
  --mode fast
```

## Then

The boundary witness is withheld: the one-line `if` has no branch pair the scan can read,
so the dead pin cannot be shown live, and the finding stays below `exposed`.

## Must Not

- Promote the finding to `exposed` because the pin reaches the boundary input with an exact
  assertion (the scan used to read the enclosing block's returns, 2 and 3, and call `3` live).

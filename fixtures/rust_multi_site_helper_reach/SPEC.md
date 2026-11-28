# Fixture: rust_multi_site_helper_reach

Spec: RIPR-SPEC-0159

## Given

The private helper `number` changes its leading-zero predicate
(`s.len() > 2` → `s.len() > 1`). Its only caller, the public `parse`, calls it
at three direct sites (`number(parts[0])?`, `number(parts[1])?`,
`number(parts[2])?`). The integration test `parses_a_plain_version` calls
`parse("1.2.3")` and asserts only `is_ok()`.

## When

```bash
ripr check \
  --root fixtures/rust_multi_site_helper_reach/input \
  --diff fixtures/rust_multi_site_helper_reach/diff.patch
```

## Then

- Reach is `yes`: the helper chain `parses_a_plain_version -> parse -> number`
  resolves through a reach-only multi-site hop (#14918), and the test is
  related to the changed owner.
- The finding is `infection_unknown`, as for a single call site with computed
  arguments: no site's arguments stand for the others, so no input row binds.

## Must Not

- Report `no_static_path` with a transitive-reach limit for a helper that the
  reached entry calls directly.
- Bind one call site's arguments for the others, or forward the helper's
  result through the multi-site hop, which would claim an infection or
  propagation decision the static evidence does not support.

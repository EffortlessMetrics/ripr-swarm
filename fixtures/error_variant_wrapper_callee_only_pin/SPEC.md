# Fixture: error_variant_wrapper_callee_only_pin

Spec: RIPR-SPEC-0106

Issue: #3700

## Given

A boxed-error wrapper seam (`parse_summary` converts `try_parse_summary`
through `.map_err(..)`). The only related test calls the inner callee and
pins the exact variant against the callee's own result; no test invokes the
wrapper owner.

## When

```bash
cargo xtask fixtures error_variant_wrapper_callee_only_pin
```

or:

```bash
ripr check --root fixtures/error_variant_wrapper_callee_only_pin/input --diff fixtures/error_variant_wrapper_callee_only_pin/diff.patch --mode fast
```

## Then

The `error_path` probe for `try_parse_summary(raw).map_err(Into::into)`
reports `no_static_path` (NOT `exposed`). The callee-only pin supplies
variant identity but observes nothing about the wrapper: the test relates
only through its name, which the shared reach-bearing relation rule
(#5335) does not credit as reach, and nothing calls `parse_summary`, so
the finding discloses the missing static path instead of a reach claim.
The discriminate stage reads `no`: no test reaches the changed owner, so
no test can discriminate this change.

## Must Not

- Classify the wrapper seam as `exposed` when no wrapper-invoking test
  confirms the established variant binding (the BUG-1 false promotion).
- Treat a callee-only exact-variant pin as wrapper observation.

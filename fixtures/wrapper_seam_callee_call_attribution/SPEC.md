# Fixture: wrapper_seam_callee_call_attribution

Spec: RIPR-SPEC-0106

Issue: #3714

## Given

A wrapper error seam (`parse_summary(raw).map_err(|error| error.to_string().into())`
over the typed callee `try_parse_summary`), with two integration tests:

- a generically-named test (`observes_callee_outcome`) whose body calls only
  the seam's converted callee (`try_parse_summary("abc")`) — no owner call,
  no owner or probe-token affinity in its name;
- an unrelated-named test (`unrelated_scope_has_no_call`) that calls nothing
  related to the seam.

The diff changes only the wrapper conversion line (stringified conversion
to a concrete-error-preserving conversion), so the wrapper seam probe fires
on the changed line.

## When

```bash
cargo xtask fixtures wrapper_seam_callee_call_attribution
```

or:

```bash
ripr check --root fixtures/wrapper_seam_callee_call_attribution/input \
           --diff fixtures/wrapper_seam_callee_call_attribution/diff.patch \
           --mode fast
```

## Then

The generically-named callee-calling test is related with
`relation_reason: seam_callee_call` (captured `CallFact` name match on the
converted callee, confidence medium) and the seam classifies
`weakly_exposed` — the relation is weak: no exact-variant credit is claimed
for the wrapper conversion (`wrapper_error_binding_unresolved` stays the
honest outcome for the conversion's variant identity per #3700).

The unrelated-named test produces no related-test entry: a name that shares
nothing with the probe and calls nothing the seam touches establishes no
relation.

## Must Not

- Leave a direct callee-calling test unrelated (`no_static_path`) because
  its name carries no affinity — name affinity ranks, it does not gate.
- Credit `seam_callee_call` relations with exact-variant discrimination:
  the relation is weak; variant identity for wrapper conversions remains
  the typed `wrapper_error_binding_unresolved` limitation (#3700).
- Relate tests whose bodies define or bind a same-named local (`fn <callee>`
  / `let <callee> =` shadow defeat).
- Use mutation-runtime outcome vocabulary reserved for real mutation execution.

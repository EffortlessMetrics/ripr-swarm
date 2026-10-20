# Fixture: typescript_local_registration

Spec: RIPR-SPEC-0243

## Given

The changed predicate includes age 18. A local no-op `describe` discards
one callback containing a boundary test. A genuine imported `describe`
contains a local no-op `test` that discards another boundary assertion.
Neither callback is registered with the runner (#8959).

## When

`cargo xtask fixtures typescript_local_registration` analyzes the diff.

## Then

Exactly one finding remains `no_static_path`, with no related tests and
oracle `unknown` / `none`. The honesty corpus pins this independently of
the golden output. Analyzer tests retain imported and ambient-global
registrations, lexical scope isolation and nonempty registration loops.

## Must Not

- Credit callbacks discarded by a locally bound registration lookalike.
- Withhold an independent genuine registration in another lexical scope.
- Claim runtime execution or general higher-order-function analysis.

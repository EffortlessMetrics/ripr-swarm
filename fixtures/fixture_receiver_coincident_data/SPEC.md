# Fixture: fixture_receiver_coincident_data

Spec: RIPR-SPEC-0240

## Given

Three independent same-file helpers build literal receiver data, and each eager
assertion pins the returned total exactly. Owner method names are unique across
the crate so unrelated owner ambiguity cannot mask the refusal.

## When

One edit swaps equal tuple columns. A second changes a single-row product to a
sum. A third changes an all-zero product to a sum. Each original and changed
native computation produces the same literal asserted value (7, 7 and 0).

## Then

All three findings remain `static_unknown` with the framework-oracle limit.
Equal projected totals do not discriminate the actual edit, and an unread old
product pipeline supplies no bounded fixture pin.

## Must Not

- Promote any finding from a readable but coincident literal vector.
- Clear fixture opacity without an admitted before/current pipeline.
- Generalize this refusal to equivalent tuple binding renames or demand that
  fixtures distinguish every hypothetical mutant.

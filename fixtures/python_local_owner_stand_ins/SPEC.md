# Fixture: python_local_owner_stand_ins

Spec: RIPR-SPEC-0028

## Given

`fee` changes from `amount * 2` to `amount * 3`. Seven tests import `fee` and then assert `fee(10) == 30` against a substitute bound to the same name: a local `def`, a lambda, a `Mock`, an aliased `Mock`, a walrus target, a `match` capture, and a module-level reassignment after the import. Native execution passes every test with either production return (#8247).

## When

`cargo xtask fixtures python_local_owner_stand_ins`

## Then

The finding is `weakly_exposed` with `orthogonal` alignment: no assertion observes the imported owner's return.

## Must Not

Report `exposed` from an assertion that reads a substitute of the imported owner. The governed RIPR-SPEC-0108 honesty corpus independently forbids promotion.

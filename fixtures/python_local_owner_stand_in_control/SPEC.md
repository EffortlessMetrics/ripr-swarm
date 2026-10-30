# Fixture: python_local_owner_stand_in_control

Spec: RIPR-SPEC-0028

## Given

The same `fee` change as `python_local_owner_stand_ins`. The tests assert `fee(10) == 30` on the imported owner itself, once plainly and once beside a helper-local shadow that its own scope contains.

## When

`cargo xtask fixtures python_local_owner_stand_in_control`

## Then

The finding is `exposed`: the stand-in gate withholds credit only where the asserting scope rebinds the owner, so the real import keeps it.

## Must Not

Withhold credit from the real import or from an outer call beside a helper-local shadow. This control proves the stand-in gate is not a blanket veto.

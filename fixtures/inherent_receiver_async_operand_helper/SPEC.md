# Fixture: inherent_receiver_async_operand_helper

Spec: RIPR-SPEC-0005

## Given

A standard equality operand constructs and discards an unpolled future whose
body calls the same-file free helper `read_as_object`, which in turn calls the
private inherent owner.

## When

Analyze the changed Object arm and complete repository; compile and run the
original and the one-arm `None` mutant separately.

## Then

Both native executions pass. The dormant operand grants neither direct nor helper
owner credit. All four owner seams, both diff probes and the named test remain.
Full and compact metrics agree. Adjacent outer and mixed-async calls retain genuine
repository direct/helper credit; diff helper support remains conservative.

The two probes stay `weakly_exposed` here, whereas the direct-owner sibling
`inherent_receiver_async_operand` is `reachable_unrevealed`; the golden change is
next-step wording only.

## Must Not

Treat construction of a future as polling it, expand arbitrary macros or infer
value flow. Dormant item/closure/async-move controls remain conservative. The shared
owner-identity matrix (`cases.json`, fixtures slice #11029; consumer test in
#11031) is not carried by this slice.

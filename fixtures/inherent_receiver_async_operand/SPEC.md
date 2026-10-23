# Fixture: inherent_receiver_async_operand

Spec: RIPR-SPEC-0005

## Given

A standard equality operand constructs and discards an unpolled future whose
body mentions the private inherent owner, directly or through a same-file helper.

## When

Analyze the changed Object arm and complete repository; compile and run the
original and the one-arm `None` mutant separately.

## Then

Both native executions pass. The dormant operand grants neither direct nor helper
owner credit. All four owner seams, both diff probes and the named test remain.
Full and compact metrics agree. Adjacent outer and mixed-async calls retain genuine
repository direct/helper credit; diff helper support remains conservative.

## Must Not

Treat construction of a future as polling it, expand arbitrary macros or infer
value flow. Dormant item/closure/async-move controls remain conservative. The shared
34-case matrix also retains the previous22 receiver/helper/scope controls.

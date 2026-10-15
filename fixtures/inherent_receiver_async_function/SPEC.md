# Fixture: inherent_receiver_async_function

Spec: RIPR-SPEC-0005

## Given

A same-file free or inherent helper is declared `async fn`. Calling and
immediately discarding it constructs a future without executing its body.

## When

Analyze the changed Object arm through diff, full repository and fresh compact
consumers. Compile and execute each original and owner-arm mutant independently.

## Then

Async helper entries, standard assertion operands, intermediate hops and last
hops supply no inherent-owner execution credit. All four repository owner seams,
both changed-owner diff probes and the named test remain. Genuine synchronous
chains, methods and mixed synchronous/async routes retain their existing credit.
A synchronous helper containing a dormant async block remains eligible.

## Must Not

Infer polling, general type/value flow or macro expansion. The explicitly polled
native-positive control remains statically unsupported by this bounded traversal.
Native execution and static admission are independent axes. Preserve all original
34 executable identity controls and their truth labels; diff helper evidence
remains conservative. This fixture makes no performance claim for #8005.

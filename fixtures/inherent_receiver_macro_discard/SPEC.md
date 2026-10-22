# Fixture: inherent_receiver_macro_discard

Spec: RIPR-SPEC-0005

## Given

A private inherent owner and the independent review reproduction from #8004.

## When

Analyze the changed Object arm and the complete repository; compile and run the source as native Rust tests, then replace the Object arm with None.

## Then

No direct or helper reach is credited to the owner. The native test survives the owner mutant.

## Must Not

Infer receiver identity from method-name coincidence or discarded macro operands.

## Regression Controls

The Rust files under `controls/` are complete native test subjects. Original
subjects compile and pass. The local-helper, standard-assertion and sibling-macro
positives reject the private Object-arm mutant; foreign entries, foreign last
hops and a module-shadowed owner type accept it. The wrapper fixture's own test
rejects that mutant. `inherent_receiver_review_last_hop_and_wrapper_controls`
compares full and compact evidence for these subjects.

The honesty corpus also pins a repository report for this input. Regenerate with
`ripr check --root fixtures/inherent_receiver_macro_discard/input --mode ready --format repo-exposure-json`.
The checked report retains the complete consumer fields and source digests with
the volatile artifact envelope removed; unmodified raw reports belong to the
PR's immutable evidence packet. These observations do not change runtime truth
or the five existing false-actionable corpus labels.

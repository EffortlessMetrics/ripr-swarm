# Fixture: inherent_receiver_local_wrapper

Spec: RIPR-SPEC-0005

## Given

A private inherent owner and the independent review reproduction from #8004.

## When

Analyze the changed Object arm and the complete repository; compile and run the source as native Rust tests, then replace the Object arm with None.

## Then

The genuine local receiver delegation retains helper reach; its native test rejects the owner mutant.

## Must Not

Infer receiver identity from method-name coincidence or discarded macro operands.

## Regression Controls

The Rust files under `controls/` are complete native test subjects. Original
subjects compile and pass. The foreign-entry and foreign-last-hop controls
accept the private Object-arm mutant. The wrapper fixture's own test rejects that
mutant. Other owner-identity controls (local-helper, standard-assertion,
sibling-macro and module-shadowed owner type) belong to other slices. A test comparing full and compact evidence for these subjects is intended to be
carried by the integration-tests slice (#11031); this fixture slice does not
carry that test.

The honesty corpus is intended to pin a repository report for this input; this
fixture slice does not carry that report. Once added, regenerate with
`ripr check --root fixtures/inherent_receiver_local_wrapper/input --mode ready --format repo-exposure-json`.
The report is intended to retain the complete consumer fields and source digests with
the volatile artifact envelope removed; unmodified raw reports belong to the
PR's immutable evidence packet. These observations do not change runtime truth
or the five existing false-actionable corpus labels.

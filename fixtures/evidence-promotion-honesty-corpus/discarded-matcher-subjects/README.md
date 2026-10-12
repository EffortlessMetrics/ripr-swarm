# Discarded matcher calibration inputs

These fourteen tiny Rust inputs belong to #5713 and RIPR-SPEC-0001/0108.
The independent value contract is `score(1) == 2`. Each of seven observer
forms has an original source returning 2 and a wrong source returning 3;
each patch describes its actual source.

Bare wildcard, exact and guarded matchers, and an unused exact matcher
binding cannot fail a test. Their eight static findings must retain the
intended consumer while reporting `reachable_unrevealed`, an `unknown` /
`none` oracle and `no_assertion`. Asserting wildcard wrappers remain weak;
asserting exact and guarded wrappers retain strong observation.

The existing `discarded_matcher_cli_controls_reject_false_credit_and_retain_consumers`
test checks these inputs through the current CLI in JSON, human and human-full.
It checks input/patch identity, exactly one finding and intended consumer,
oracle metadata and explanations, and retains raw outputs with invocation
and producer identity. Relative canonical roots keep the producer paths
stable without normalizing captured JSON.

Canonical reports were captured from these checked-in roots, copied without
normalization and registered as fourteen typed cases in the existing corpus.
The sibling `discarded-matcher-reports` directory retains their custody and
the bootstrap's aggregate cleanup failure. Canonical validator negatives and
current-head CLI comparison remain separate execution requirements.
Earlier temporary-root outputs remain historical evidence.
These authored controls do not establish a representative population,
runtime mutation outcomes or an accuracy percentage.

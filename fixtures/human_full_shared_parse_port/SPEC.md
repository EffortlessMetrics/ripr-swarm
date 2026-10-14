# Fixture: human_full_shared_parse_port

Spec: RIPR-SPEC-0122

## Given

The #7394 reproduction changes `parse_port` from accepting every parsed port
to an `Ok(p) if p > 0` guard and a new `Err("zero port")` arm. Its one test
asserts `parse_port("8080").unwrap() == 8080`. `before.rs`, `input/src/lib.rs`
and `diff.patch` retain the actual two-commit Git reproduction. The original
audit repository was not attached to the issue; these bytes reproduce its
reported function, assertion, diff and seven-finding/265-line shape.

## When

Run `ripr check --root input --diff diff.patch --format human-full`, or the
maintained CLI test that commits both revisions and checks `--base HEAD~1`.

## Then

Every finding retains its identity, complete changed fragments, discriminator,
classification, currentness, weakness and usable drill-in commands. Identical
evidence and guidance may be defined once with attributable CLI references.
Different boundary misses and historical contexts remain distinct.

`expected/library-human-lines.json` pins the normalized complete bytes captured from
an actual public-library consumer on the pristine source baseline. Only root
and the two Git revision labels are normalized. Each JSON string retains one
complete line including its terminator, so concatenation recovers the exact
record and its intentional final empty line without a whitespace exemption. Public library renders retain
that exhaustive legacy report, including all repeated evidence.

## Must Not

- Drop a finding, changed expression or discriminator to shorten the view.
- Change JSON decisions or base-side semantics.
- Share different owners, tests, evidence, currentness or recommendations.
- Change legacy public-library bytes or lose drill-in scope.

# Fixture: javascript_tap_registration

Spec: RIPR-SPEC-0085
Spec: RIPR-SPEC-0243

## Given

The #9187 JavaScript reproduction registers two tests through tap's named
`test` export. Each compares `stringify`'s return value using its own
callback context and then calls `end`.

## When

The `undefined` predicate is inverted. Run:

```bash
cargo xtask fixtures javascript_tap_registration
```

## Then

The changed owner has two related tests with exact-value/strong assertions.
Their complementary inputs give the predicate `exposed` static evidence.
Native tap results are independent evidence, not a static runtime verdict.
No repair packet or support-tier promotion is established.

## Must Not

- Drop the tests solely because the named export comes from tap.
- Treat tap namespace assertions or all tap methods as supported.
- Claim a runtime result from this static fixture.

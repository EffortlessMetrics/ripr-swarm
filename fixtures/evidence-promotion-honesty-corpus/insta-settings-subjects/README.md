# Unread Insta configuration controls

RIPR-SPEC-0240 / RIPR-SPEC-0108, issue #13035.

These small analyzer inputs change one `make_record` field and retain one
direct related test. The `Settings`, `with_settings!` and inline-settings
inputs must disclose `rust_framework_oracle_unresolved`, with exactly one
`field_construction` finding. The configured redaction concerns `id`; both
that field and its unredacted `items` sibling remain limited because this
reader does not evaluate active settings.

Literal `.id` maps retain the known redacted-field gap and unredacted-sibling
limitation. Constant selectors remain limited. An unrelated snapshotted
value and an ordinary inline snapshot keep their existing gaps. The separate
exact field assertion without a snapshot remains exposed. These controls
reject both the previous early return and blanket snapshot withholding.

The source inputs are parsed by RIPR; they are not native Insta execution or
snapshot-adequacy evidence. No expected snapshot bytes are admitted here.
Full expected-byte, settings identity and positive projection support remain
with #8121. The public integration tests additionally cover comments, strings,
nearby identifier names, later settings, a shadowed owner result and warm
test-only configuration edits.

## Regenerate the reports

From the repository root, build the candidate `ripr` binary, then run each
of the following names through the command below:

- `settings-id`, `settings-items`
- `with-settings-id`, `with-settings-items`
- `inline-settings-id`, `inline-settings-items`
- `constant-selector-id`, `constant-selector-items`
- `literal-map-id`, `literal-map-items`
- `other-result`, `ordinary-inline`, `independent-exact`

```bash
case_name=settings-id
subject=fixtures/evidence-promotion-honesty-corpus/insta-settings-subjects/$case_name
target/debug/ripr check --root "$subject" --diff "$subject/change.diff" \
  --mode ready --format json --quiet \
  > "fixtures/evidence-promotion-honesty-corpus/reports/rust-insta-$case_name.json"
cargo xtask check-evidence-promotion-honesty
```

The reports are actual public JSON output. The independent case files pin the
required classes and limitation; do not change those expectations merely to
make a new capture pass. A complete successful capture has one changed Rust
file, one candidate line, one probe and one finding on the named changed
field. Keep the executable's source identity and native command results in
the PR's validation record.

The native `canonical_next_action` detail route resolves paths against the
capture workspace even when the root argument is relative. Preserve those
captured display strings as emitted; the case's semantic assertions and the
checked-in source inputs carry the portable oracle.

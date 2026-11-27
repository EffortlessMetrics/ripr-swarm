# Fixture: python_self_container_record_unread_no_exposed_credit

Spec: RIPR-SPEC-0238

## Given

`Clock.tick` records into the private list `self._seen` and returns
`self.hours + n`. `count` reads the list and `total` reaches it through
`count`. The diff changes the recorded value from `n` to `n + 1`. The only
test asserts `Clock(1).tick(2) == 3`, which pins the result of `tick` and
never reads the list.

## When

`cargo xtask fixtures python_self_container_record_unread_no_exposed_credit`

## Then

The finding is `weakly_exposed` and names `count` and `total` as the
readers no test calls. This is the #16010 false-exposed correction.

## Must Not

Promote the finding to `exposed` from an exact-value assertion on the result
of the recording method. The RIPR-SPEC-0108 honesty corpus independently
forbids promotion.

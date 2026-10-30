# Fixture: ts_class_getter_factory_rebound

Spec: RIPR-SPEC-0243

Issue: #10402

## Given

The same `Cached` class, `cached` factory and getter edit as
`ts_class_getter_factory_read`. The test binds `let box = cached(...)` and then
reassigns `box = { value: 1 }` before asserting `expect(box.value).toBe(1)`.

## When

`ripr check --root input --diff ../diff.patch --format json --mode fast`
exercises the production CLI.

## Then

The finding stays `no_static_path`: the receiver the assertion reads is not the
factory's product, so no related test is credited (RIPR-SPEC-0243 decision 23).

## Must Not

- Credit a test whose receiver is rebound after the factory call.

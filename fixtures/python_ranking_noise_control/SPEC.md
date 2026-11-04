# Fixture: python_ranking_noise_control

Spec: RIPR-SPEC-0028

## Given

A Python preview workspace has several changed behaviors:

- a direct weak predicate-boundary gap with a concrete missing discriminator,
- an already-observed exact return-value change,
- a changed owner with no related test,
- and a dynamic-dispatch static limitation.

The fixture workspace enables the Python preview adapter explicitly:

```toml
[languages]
enabled = ["rust", "python"]
```

## When

```bash
cargo xtask fixtures python_ranking_noise_control
```

or:

```bash
ripr check \
  --root fixtures/python_ranking_noise_control/input \
  --diff fixtures/python_ranking_noise_control/diff.patch \
  --mode fast
```

## Then

The report ranks the repairable direct weak Python finding before lower-value
Python preview noise, even though the noisy files sort earlier by path.

## Must Not

- Treat static-limit or no-related-test findings as repair-ready work.
- Hide the non-actionable findings.
- Emit a full repair card, agent packet, or receipt command.

## Declared classes

```expected-classes
[
  [
    "probe:src_c_discount.py:python_preview:91995184",
    "weakly_exposed"
  ],
  [
    "probe:src_b_observed.py:python_preview:3c7333c2",
    "exposed"
  ],
  [
    "probe:src_d_isolated.py:python_preview:557fe7cf",
    "no_static_path"
  ],
  [
    "probe:src_a_dynamic.py:python_preview:01f806b7",
    "static_unknown"
  ]
]
```

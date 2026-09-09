# Fixture: error_variant_boxed_wrapper_downcast_witness

Spec: RIPR-SPEC-0106

Issue: #3700

## Given

A boxed-error wrapper seam in the shape reported on #3700
(`perl-lsp-swarm` `parse_perl_summary`, lib.rs:532): a public owner returns
`Result<T, Box<dyn Error>>` by converting an inner typed error through
`try_parse_summary(raw).map_err(Into::into)`, next to a typed callee that
returns `ParseSummaryError::MalformedSource`. Two existing tests observe the
exact variant:

- a typed sibling test whose name contains the wrapper owner
  (`parse_summary_fails_closed_on_malformed_source`), calls the callee, and
  whose fallible body returns `Err` unless
  `matches!(result, Err(ParseSummaryError::MalformedSource))`;
- a boxed downcast witness
  (`parse_summary_boxed_variant_propagates_malformed_source`) that extracts
  the error with `.err().ok_or(...)?` and returns `Err` unless
  `matches!(error.downcast_ref::<ParseSummaryError>(), Some(ParseSummaryError::MalformedSource))`.

Five companion wrappers pin the fail-closed side, each with its own observing
test:

- `checksum_summary` observed by a downcast witness that pins
  `ChecksumError::MalformedPayload` — a sibling the callee never produces
  (wrong sibling variant, no callee binding);
- `render_summary` observed by a downcast witness that pins
  `ParseSummaryError::MalformedSource` — a variant of an unrelated enum
  (unrelated owner, no callee binding);
- `length_summary` observed only through `assert!(result.is_err())`
  (broad oracle, no variant pin);
- `glyph_summary` whose changed seam destroys typed identity with
  `map_err(|error| error.to_string().into())` while a well-shaped downcast
  witness still names the variant;
- `theme_summary` observed by a test that computes
  `matches!(error.downcast_ref::<ThemeError>(), ...)` and discards the result
  instead of returning `Err` on mismatch.

The diff changes each wrapper's conversion line (stringified conversion to
`Into::into`, and — for the stringified negative — `Into::into` to the
stringified conversion), so one `error_path` probe lands on each wrapper seam.

## When

```bash
cargo xtask fixtures error_variant_boxed_wrapper_downcast_witness
```

or:

```bash
ripr check --root fixtures/error_variant_boxed_wrapper_downcast_witness/input \
           --diff fixtures/error_variant_boxed_wrapper_downcast_witness/diff.patch \
           --mode fast
```

## Then

The `map_err(Into::into)` wrapper seam classifies `exposed` with
`exact_error_variant` / `strong`, crediting the boxed downcast witness as a
related discriminator (`direct_owner_call`, `exact_value` / `strong`) and the
typed sibling test (`owner_named_test`, `exact_error_variant` / `strong`).
The credit is variant-bound: the typed witness calls the seam's callee and
pins `Err(ParseSummaryError::MalformedSource)` against it — a pin the compiler
type-checks against the callee's error type — which establishes the
wrapper-to-variant binding. Released 0.10.0 reported an actionable
error-variant gap for this seam despite the existing downcast witness.

Every fail-closed companion stays `weakly_exposed`:

- the wrong-sibling downcast witness (pins `ChecksumError::MalformedPayload`
  without calling the callee) no longer confirms the seam — before the #3700
  wrapper gate this shape classified `exposed` through token overlap between
  the seam expression and the witness message text;
- the unrelated-enum downcast witness (pins
  `ParseSummaryError::MalformedSource` against the `render_summary` wrapper)
  no longer confirms the seam — same pre-fix over-credit;
- the broad `is_err()`-only observer reports `broad_error` / `weak` with the
  exact error variant discriminator still missing;
- the stringified-conversion seam loses the complete propagation witness, so
  even a well-shaped downcast witness keeps that seam at `weakly_exposed`;
- the ignored `matches!` result (`let _ = matches!(...)`) never confirms
  observation, so its seam stays `weakly_exposed` with the exact error
  variant discriminator still missing.

## Must Not

- Report the `map_err(Into::into)` wrapper seam as an actionable error-variant
  gap while the downcast witness and the typed sibling test exist.
- Upgrade a wrapper seam whose expression carries no parseable variant to
  `exposed` on token overlap alone: without an established variant identity,
  witness text that happens to name the seam's parameters, its callee, or
  `Into::into` is token coincidence, not discrimination.
- Credit a downcast witness that pins a DIFFERENT variant of the wrapper's
  inner error, or a variant of an unrelated enum, because neither pin is
  bound to the callee's error type.
- Credit the broad `is_err()`-only observer, the stringified conversion, or
  the ignored `matches!` result as an exact error variant discriminator.
- Use mutation-runtime outcome vocabulary reserved for real mutation execution.

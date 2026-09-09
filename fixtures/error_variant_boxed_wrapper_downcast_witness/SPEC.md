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
  (`parse_summary_fails_closed_on_malformed_source`) and whose fallible body
  returns `Err` unless `matches!(result, Err(ParseSummaryError::MalformedSource))`;
- a boxed downcast witness (`parse_summary_boxed_variant_propagates_malformed_source`)
  that extracts the error with `.err().ok_or(...)?` and returns `Err` unless
  `matches!(error.downcast_ref::<ParseSummaryError>(), Some(ParseSummaryError::MalformedSource))`.

Three companion wrappers pin the fail-closed side, each with its own
observing test:

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
This is the reduced producer shape of #3700: released 0.10.0 reported an
actionable error-variant gap for this seam despite the existing downcast
witness.

The fail-closed companions stay `weakly_exposed`:

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
- Credit the broad `is_err()`-only observer, the stringified conversion, or
  the ignored `matches!` result as an exact error variant discriminator.
- Use mutation-runtime outcome vocabulary reserved for real mutation execution.
- Credit a downcast witness whose `matches!` pins a DIFFERENT variant of the
  wrapper's inner error, or a variant of an unrelated enum: as of the current
  producer these shapes can still classify `exposed` through probe-token
  overlap in the witness message text (the wrapper seam expression carries no
  parseable variant, so the RIPR-SPEC-0106 Part B variant guard cannot gate
  them). That gap is recorded on #3700; this fixture deliberately does not
  bless it with goldens until the producer gates variant identity for wrapper
  seams.

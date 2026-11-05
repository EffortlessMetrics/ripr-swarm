# Fixture: error_variant_boxed_wrapper_downcast_witness

Spec: RIPR-SPEC-0106

Issue: #3700

## Given

A boxed-error wrapper seam in the shape reported on #3700
(`perl-lsp-swarm` `parse_perl_summary`, lib.rs:532): a public owner returns
`Result<T, Box<dyn Error>>` by converting an inner typed error through
`try_parse_summary(raw).map_err(Into::into)`, next to a typed callee whose
early-error branches construct `ParseSummaryError::MalformedSource`. Two
existing tests observe the exact variant: a typed sibling test (whose name
contains the wrapper owner, calls the callee, and pins
`Err(ParseSummaryError::MalformedSource)` via `matches!` with a `return Err`
on mismatch) and a boxed downcast witness (extracts the error with
`.err().ok_or(...)?` and returns `Err` unless
`matches!(error.downcast_ref::<ParseSummaryError>(), Some(ParseSummaryError::MalformedSource))`).

Five companion wrappers pin the fail-closed side: a wrong-sibling downcast
pin (`checksum_summary`, pins `MalformedPayload` which the callee never
produces), an unrelated-enum downcast pin (`render_summary`, pins
`ParseSummaryError::MalformedSource` against a different enum), a broad
`is_err()`-only observer (`length_summary`), a stringified conversion
(`glyph_summary`, `map_err(|error| error.to_string().into())`), and an
ignored `matches!` result (`theme_summary`).

The diff changes the typed error construction inside `try_parse_summary`
(both early-error branches construct `MalformedSource`) and each wrapper's
conversion line, so one `error_path` probe lands on the parseable-variant
typed seam and one on each wrapper conversion.

## When

```bash
cargo xtask fixtures error_variant_boxed_wrapper_downcast_witness
```

or:

```bash
ripr check --root fixtures/error_variant_boxed_wrapper_downcast_witness/input            --diff fixtures/error_variant_boxed_wrapper_downcast_witness/diff.patch            --mode fast
```

## Then

The two typed parseable-variant return sites remain `weakly_exposed` when
this bound-result witness does not establish which early return produced the
asserted value. An exact variant category and an owner call alone cannot lend
sink identity to both sites (RIPR-SPEC-0106 / RIPR-SPEC-0197).

Native per-return controls distinguish this from missing test execution:
changing only the first return makes both existing parse witnesses fail;
changing only the second return leaves their outcomes unchanged. The complete
fixture harness also contains deliberately wrong companion assertions, so its
whole exit is not a passing-suite claim. Static analysis remains conservative
for both typed sites until it can bind the selected observer to the particular
return. The directly bound unique unit-variant positives remain covered by
`unwrap_err_variant_positive` and the production return-confirmation controls.

The `map_err(Into::into)` wrapper seams with asserting observers — including
the one the downcast witness reaches — classify `weakly_exposed` and carry
the typed static limitation
`static_limit_kind: wrapper_error_binding_unresolved`: whether the boxed
conversion faithfully carries the callee's error variant is not statically
establishable, so lexical confirmation is refused by construction. The
downcast witness and the typed sibling test remain listed as related, but the
emitted missing/recommendation text names the limitation instead of prescribing
an assertion the suite may already contain. Crediting a faithful typed
conversion requires modeling `Into`/`From` through `Box` (follow-up slice).

The asserting fail-closed companions stay `weakly_exposed` for their own
reasons: no variant pin (broad `is_err()`), a conversion that destroys the typed
identity (stringified), and pins that are not bound to the converted callee
(wrong sibling, unrelated enum).

The `theme_summary` observer discards its `matches!` result. Its wrapper
`error_path` and `return_value` findings are `reachable_unrevealed` with
`unknown` / `none` oracle metadata and a retained `no_assertion` consumer. The
primary guidance asks for an assertion. Secondary missing text may still name
the unresolved wrapper binding; the optional `static_limit_kind` and
`static_limitation` fields are absent for these findings.

## Must Not

- Classify a wrapper `map_err(Into::into)` seam as `exposed` from lexical
  heuristics: every overlap between the seam expression and witness text is
  token coincidence, and the conversion's variant binding is not statically
  establishable.
- Prescribe an exact-variant assertion for a wrapper seam whose witnesses
  already downcast-and-pin the variant; emit the typed limitation instead.
- Credit both typed return sites merely because one owner-level exact-variant
  witness exists; retain the unresolved per-return identity rather than infer
  which branch that witness exercises.
- Use mutation-runtime outcome vocabulary reserved for real mutation execution.

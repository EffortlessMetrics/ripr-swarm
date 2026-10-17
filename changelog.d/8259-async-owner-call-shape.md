<!-- section: Fixed -->
- `ripr explain` and `ripr context` no longer project a synchronous-looking
  owner call shape for `async fn` owners. `OwnerSignature::call_shape`
  declines instead of rendering `fetch(input: u8) -> u8`, so drill-in
  guidance does not imply a direct call returns the declared output. The
  same producer already refuses `ripr agent stub` with `owner_async`; this
  matches that boundary without inventing a runtime or assertion (#8259).

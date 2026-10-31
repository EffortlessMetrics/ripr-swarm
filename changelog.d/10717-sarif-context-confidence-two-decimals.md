<!-- section: Fixed -->
- SARIF `properties.confidence` and the context packet `witness.confidence.value` now render the two-decimal contract value (`0.4`, not the widened `f32` `0.4000000059604645` or an `f32` computation artifact), through the same `domain::contract_confidence` helper the LSP diagnostic uses, so one finding carries one confidence on every surface (#10717).

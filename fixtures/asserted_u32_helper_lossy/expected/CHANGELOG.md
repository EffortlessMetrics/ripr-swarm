# Golden Output Changes

## Pending: asserted helper comparison

Observed from the verified 7f83449e wheel with the complete fixture postimage and original case diff. The existing xtask normalization functions produced these bytes; `cargo xtask goldens bless/check` was not executed locally.

Reason: The helper compares only the low byte and can miss changes to higher bytes. Expected class `propagation_unknown` is the actual public CLI result for this independent subject (#13181).

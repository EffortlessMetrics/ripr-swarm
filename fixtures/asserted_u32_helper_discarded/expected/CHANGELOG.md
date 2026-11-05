# Golden Output Changes

## Pending: asserted helper comparison

Observed from the verified 7f83449e wheel with the complete fixture postimage and original case diff. The existing xtask normalization functions produced these bytes; `cargo xtask goldens bless/check` was not executed locally.

Reason: The selected test discards the helper result and contains no assertion. Expected class `reachable_unrevealed` is the actual public CLI result for this independent subject (#13181).

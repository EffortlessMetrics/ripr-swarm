# Golden Output Changes

## Pending: asserted helper comparison

Observed from the verified 7f83449e wheel with the complete fixture postimage and original case diff. The existing xtask normalization functions produced these bytes; `cargo xtask goldens bless/check` was not executed locally.

Reason: The selected helper compares an independent converter; it does not call the changed owner. Expected class `no_static_path` is the actual public CLI result for this independent subject (#13181).

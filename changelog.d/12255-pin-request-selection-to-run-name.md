<!-- section: Internal / CI -->
- A repo-policy test now renders the `ci.yml` run-name selection tail and the routed Result's `REQUEST_SELECTION` expression and requires them to match, so drift fails at review instead of as a 20 minute fail-closed wait (#12255).

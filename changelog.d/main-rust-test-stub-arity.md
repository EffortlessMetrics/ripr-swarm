<!-- section: Fixed -->
- `ripr agent stub --kind return_value` at a probe line builds again after the #12322 and #12357 merge race left main with a stale `rust_test_stub` call; the direct stub now passes the package root so the cfg-gated parent check applies there too.

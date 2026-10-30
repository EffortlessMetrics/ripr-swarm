<!-- section: Docs -->
- The `RIPR_PROBE_READ_TRACE` diagnostic records a test read by position as a keyed `test` read instead of a whole-set read. On this repository's `crates/ripr` diff that one call site accounted for 988,905 of 989,930 whole-set events, so the trace now separates the real whole-set sites (#9784).

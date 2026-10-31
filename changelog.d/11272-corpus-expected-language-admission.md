<!-- section: Fixed -->
- The verdict-corpus harness no longer scores a live case from output that does not
  establish the measurement. It now requires the producer's `analysis_outcome` (a
  scoreable kind), the digest of the case's own diff, and the expected language
  routed exactly once, where the language comes from the case's anchor file rather
  than from the output. Empty findings, another language's output, a stale digest
  and `no_scope`/`unsupported_input`/`analysis_failed` outcomes fail the case as an
  unestablished measurement. All 1613 live cases in the four corpora still pass;
  the producer output and `language_runs` are unchanged. #11272, #9430.

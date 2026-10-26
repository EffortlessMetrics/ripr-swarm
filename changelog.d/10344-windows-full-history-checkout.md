<!-- section: Internal / CI -->
- The Windows advisory jobs (`windows-advisory` and the `windows-advisory-shards`
  test matrix) check out with full history, like the Linux test lanes, so tests
  that resolve a default base against the checkout no longer refuse on a
  shallow clone. The stale-temp sweep test now plants into the file-facts store
  every run sweeps rather than whichever cache layer `read_dir` lists first, and
  the fingerprint-store failure-row test asserts the no-row result on hosts
  without a corpus fingerprint (#10344).

<!-- section: Internal / CI -->
- `.github/workflows/workflow-lint.yml` retries the `jactionlint` release tarball
  download (`--retry 5`) while preserving the pinned SHA-256 check and offline
  lint invocation, with a regression test covering transient HTTP 504 recovery,
  fast failure on permanent HTTP 404 without retry, persistent download failure,
  and checksum mismatch (#13637).

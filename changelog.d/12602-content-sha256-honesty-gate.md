<!-- section: Internal / CI -->
- `cargo xtask check-evidence-promotion-honesty` now verifies
  `artifact.content_sha256` on committed repo-exposure captures under
  `fixtures/evidence-promotion-honesty-corpus/reports/`, so hand-edited
  producer metadata without recommitting the digest fails the gate (#12602).

<!-- section: Fixed -->
- The language server no longer re-resolves every open document against the filesystem on every refresh, so opening thousands of documents is no longer
  quadratic: non-Rust files under the workspace are skipped without any path canonicalization and each open `.rs` document's workspace-relative key is
  resolved once and reused until it is reopened, closed, saved or a watched file changes (#11763).

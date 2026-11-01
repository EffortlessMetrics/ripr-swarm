<!-- section: Fixed -->
- The language server no longer re-resolves every open document against the filesystem on every refresh, so opening thousands of documents is no longer
  quadratic: non-Rust files lexically inside the workspace root are skipped without any path canonicalization, and each open `.rs` document's
  workspace-relative key is resolved once and reused until that document is reopened, closed or saved, or a watched-file event names its path or an
  ancestor directory; unrelated saves and watched-file events keep every other document's key (#11763).

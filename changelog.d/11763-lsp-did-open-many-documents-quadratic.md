<!-- section: Fixed -->
- The language server no longer re-resolves every open document against the filesystem on every refresh, so opening thousands of documents is no longer
  quadratic: non-Rust files lexically inside the workspace root are skipped without any path canonicalization, and each open `.rs` document's
  workspace-relative key is resolved once and reused until that document is reopened, closed or saved, or a watched-file event names its path or an
  ancestor directory; unrelated saves and watched-file events keep every other document's key (#11763). The cache gives up per-refresh freshness on purpose: a symlink retargeted outside the editor (`git checkout`, a build step) with no client watcher covering its path or directory is seen only at the next reopen, close, save or watched-file event for it. That is acceptable because this index is not the containment authority; workspace diagnostics still resolve their own projector on every refresh.

===== PRRT_kwDOSiSx0c6fM78g [coderabbitai]
_🎯 Functional Correctness_ | _🟡 Minor_ | _⚡ Quick win_

**Use the production content hash in this generation-transition fixture.**

`content_hash_for` computes SHA-256, but `RepoFileFactCacheKey::new` uses `hash_bytes`. The current key therefore differs in content hash as well as schema version. This test passes even if the schema generation does not change. Use `hash_bytes(&content)` so the miss proves the generation boundary.

<details>
<summary>Proposed fix</summary>

```diff
-            con

===== PRRT_kwDOSiSx0c6fNSPA [devin-ai-integration]
<!-- devin-review-comment {"id": "ANALYSIS_pr-review-job-831b5eca0f9845089c45c94c3fcba4f2_0001", "file_path": "crates/ripr/src/analysis/facts/harness_registry.rs", "start_line": 395, "end_line": 410, "side": "RIGHT", "kind": "analysis"} -->

📝 **Info: Scope selection preserves macro fallback**

`parent_ancestors` selects the exact lexical function for ordinary registrations. Registrations inside macro token trees retain the conservative line-span fallback.

<!-- devin-review-badge-begin -->
<a h


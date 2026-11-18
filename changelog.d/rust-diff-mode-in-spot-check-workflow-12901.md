<!-- section: Internal / CI -->
- The hosted mutation spot check now also scores the corpus through `ripr check --diff` (`--rust-diff-mode`, at most 300 mutants per crate), so its scoreboard reports product-path Rust recall next to the span join (#12901).

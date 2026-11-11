<!-- section: Changed -->
- Diff analysis: an added dependent file that only mentions `include!` in a comment or string, or whose `include!` cannot become an edge, no longer forces the dependent-scope index to be rebuilt from scratch (#13577).

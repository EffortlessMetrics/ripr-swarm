<!-- section: Fixed -->

- Repo-seam unresolved reach now carries a producer-owned limit kind in analysis evidence and the seam cache, and trait-dispatch reach summaries name the new `rust_trait_dispatch_unresolved` category. The evidence record's reach limitation category follows that typed kind (`opaque_static_evidence`) instead of a test, entry or type name quoted in the summary (#5492).

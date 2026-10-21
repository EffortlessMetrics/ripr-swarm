<!-- section: Added -->
- `ripr check` accepts repeatable `--path <glob>` and `--exclude <glob>` to analyze a subset of an oversized diff. The scope applies before the diff-size limits; every changed file it leaves out is named in a `language_scope_unsupported` limitation and the outcome stays `partial_with_limitations`, never clean. A scope that matches no changed file fails with `scope_filter_matched_nothing`. The `diff_scope_oversized` refusal now names the flags (#9277).
<!-- section: Fixed -->
- A committed repository-root `target/` directory is skipped as build output before the diff-size limits, with the existing generated-file limitation (#9277).
- The unresolved-base refusal no longer fuses the fetch advice and the `--base <ref>` alternative into one clause (#9277).

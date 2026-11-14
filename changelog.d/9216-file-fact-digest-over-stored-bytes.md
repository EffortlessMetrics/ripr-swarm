<!-- section: Changed -->
- Rust checks: warm file-fact cache reads verify each entry by hashing the bytes already read instead of re-serializing the decoded facts; existing cache entries are rebuilt once under schema 1.60 (#9216).

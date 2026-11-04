<!-- section: Added -->
- Hosted formatting-only golden re-bless route (`goldens-rebless.yml`, #11736): a dispatch judges a PR's golden drift in a read-only job and pushes one bot commit of expected-file changes, validated and parented on the PR head, from a separate write job. Re-arming `merge-ready` is a comment.

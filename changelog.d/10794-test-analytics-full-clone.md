<!-- section: Fixed -->
- The `rust-tests-junit` CI job now checks out full history, so base-resolution tests no longer fail on shallow PR clones and nextest runs the whole suite instead of aborting 30% in (#10794, #9646).

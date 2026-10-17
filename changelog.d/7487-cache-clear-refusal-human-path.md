<!-- section: Fixed -->
- `ripr cache clear` now names the real directory in its symlink-alias refusal
  through the shared `human_path` renderer, so a Windows drive-letter verbatim
  prefix is stripped like `Cache dir:` and `Real dir:` (#7487). The refusal
  itself stays fail-closed.

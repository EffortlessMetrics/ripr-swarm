<!-- section: Fixed -->
- `ripr cache status` now prints the human `Real dir:` alias line through the
  shared `human_path` renderer, so a Windows drive-letter verbatim prefix is
  stripped the same way `Cache dir:` already is (#7343). JSON `real_cache_dir`
  keeps `Path::display` identity.

<!-- section: Fixed -->
- `ripr cache status` and `ripr cache clear` now print the human `Cache dir:`
  line through the shared `human_path` renderer, so a Windows drive-letter
  verbatim prefix is stripped the same way `ripr doctor` already shows the
  cache location (#7326). JSON `cache_dir` keeps `Path::display` identity.

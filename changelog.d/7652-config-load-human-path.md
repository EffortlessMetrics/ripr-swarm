<!-- section: Fixed -->
- Invalid `ripr.toml` errors now name the config file through the shared
  `human_path` renderer, so a Windows drive-letter `\\?\` verbatim prefix is
  stripped from the human stderr line and the JSON refusal `message` (#7652).
  Config discovery and `source_path` identity stay canonical.

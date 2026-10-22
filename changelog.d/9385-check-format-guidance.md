<!-- section: Fixed -->
- `ripr check --format` with an unrecognized value now names the common analysis formats (human, human-full, json, github, sarif) and points at `ripr check --help --all` for the exhaustive list, instead of printing all 23 accepted names including badge and repo-report plumbing; a near-miss typo still suggests the one closest format (#9359).

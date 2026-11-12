<!-- section: Changed -->
- `ripr init --ci github` now installs ripr through the release's `ripr-check`
  composite Action (`EffortlessMetrics/ripr/.github/actions/ripr-check@v<version>`
  with `install-only: true`) instead of copying the installer shell into the
  workflow, which brings the generated file from 216 to 150 lines. The
  installer is unchanged, its checksum and archive checks included.
  `ripr doctor` reads the pin from the Action tag, and the Action gains an
  `install-only` input. (#5409)

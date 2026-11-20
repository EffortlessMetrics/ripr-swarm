<!-- section: Security -->
- The suppressions manifest loader refuses a manifest that resolves outside
  the repository (for example a committed symlink), naming only the configured
  path, so its errors no longer echo keys from a file outside the checkout. It
  also reads only regular files, under the repository metadata TOML cap (#15546).

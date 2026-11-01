<!-- section: Fixed -->
- A `ripr.toml` listing tens of thousands of `generated_file_patterns`,
  `handwritten_files` or `profiles.bun_ub.test_roots` entries no longer stalls
  the run: duplicate detection is linear instead of quadratic, with the same
  error wording and ordering (#11560).

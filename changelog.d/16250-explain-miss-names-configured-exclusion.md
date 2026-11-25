<!-- section: Changed -->
- In a complete run, `ripr explain <file:line>` and `ripr context --at
  <file:line>` now say when the file has no finding because a `ripr.toml` pattern
  (`[languages.rust] generated_file_patterns` or
  `[languages.<language>] excluded_file_patterns`) left it out of the run, and
  name that setting. The message used to give only the line-level reasons,
  which do not apply to a file that was never analyzed (#16250).

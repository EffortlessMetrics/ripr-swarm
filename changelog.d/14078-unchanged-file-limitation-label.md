<!-- section: Fixed -->
- The human limitation line for a file ripr could not read fully now says
  `some files were not fully analyzed` instead of `some changed files were
  not analyzed`, so an unreadable or fallback-indexed file that did not
  change is no longer called a changed file. JSON and the
  `language_scope_unsupported` token are unchanged (#14078).

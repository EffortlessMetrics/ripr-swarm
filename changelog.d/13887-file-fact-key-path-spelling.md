<!-- section: Fixed -->
- The file-fact cache now keys entries by a normalized path spelling, so one
  file reached through different separator, `./`, case, or Windows verbatim
  spellings reuses one entry instead of reparsing under each spelling (#13887).

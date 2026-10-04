<!-- section: Changed -->
- Commands that build file facts, such as `ripr check`, no longer decode
  every cached file-fact entry to find which files the cache already holds.
  They read each entry's header and fully decode and integrity-check only
  the files they ask about. After one committed edit on ripr-swarm (about
  4,500 cached entries) `check` took 13.0 s instead of 27.4 s; output is
  unchanged.

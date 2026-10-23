<!-- section: Fixed -->
- Analyzing a very large single-line Rust file no longer takes minutes: the unchanged-field test and call-position check that ran once per probe shape now scan the line once per distinct shape length, so a 1.3 MB one-line edit drops from 23 s to 2 s with identical findings. (#9895)

<!-- section: Internal / CI -->
- Mutation spot check: `cargo xtask mutation-spot-check --mutmut-dump <name>=<dump>` scores ripr's Python verdicts against real mutmut outcomes through the synthetic-diff verdict join, and the Python spot-check workflow gains a pinned mutmut job over humanize, itsdangerous, tomli, python-slugify and cachetools (#13842).

<!-- section: Fixed -->
- A rustfmt-layout `if`/`else if`/`else` tail whose later arm returns the first arm's value no longer reads the first condition as exposed, matching the one-line form fixed in #9843 (#10134).

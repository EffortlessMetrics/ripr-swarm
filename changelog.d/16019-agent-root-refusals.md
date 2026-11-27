<!-- section: Changed -->
- `ripr agent verify`, `agent receipt`, `agent brief` and `agent repair` refusals for a `--root` that cannot be resolved now say to pass an existing directory as `--root`, and a receipt artifact that cannot be hashed says to check that the file still exists and is readable, then rerun (#16019).

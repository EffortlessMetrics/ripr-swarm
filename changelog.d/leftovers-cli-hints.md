<!-- section: Changed -->
- `ripr --help` names its repair row "Repair a Rust gap" instead of using the internal word "seam". `ripr agent card` and `ripr agent packet` without an id say to get a seam id from `ripr pilot --root <dir>`. `ripr check --diff HEAD` (a revision, not a file) now points at `--base REV` in its error (#11570 items 2, 3 and 8).

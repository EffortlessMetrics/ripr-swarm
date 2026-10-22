<!-- section: Fixed -->
- `ripr agent status` no longer sends the user back to `ripr pilot` when the last complete pilot run ranked no seam, withheld none and declined Rust files with a parse limit. It stops, names up to three declined files with their typed reason, and routes to the repair: split them below the Rust parse budget (or save them as UTF-8), then rerun (#9371, #9308).

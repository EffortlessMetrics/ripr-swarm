<!-- section: Fixed -->
- A match-arm finding whose `after:` row is only the arm head, with an unchanged head over a changed body, now says so and tells you to open the arm at its `file:line` to read the new body, instead of printing a bare `Tier::Pro =>` beside the full old arm (#9244).

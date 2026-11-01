<!-- section: Fixed -->
- Rust boundary inputs now resolve bare imports, local callable shadows, workspace sibling calls and
  computed arguments through shared owner identity; withheld inputs remain
  `infection_unknown` instead of inventing a missing boundary gap (#10046).

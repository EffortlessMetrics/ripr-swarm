<!-- section: Fixed -->
- Python: removing, adding or changing one member of a literal membership
  collection (`if val in (True, "yes", "y", "on"):` losing `"y"`) no longer
  reads `exposed` when no strong related test passes that member; it reads
  `weakly_exposed` and names the missing case (`val == "y"`) (#15632).

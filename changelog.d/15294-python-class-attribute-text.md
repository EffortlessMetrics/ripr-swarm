<!-- section: Fixed -->
- Python: editing a class-attribute string constant (for example a
  `message_max = "..."` error template) no longer reads `exposed` through tests
  that only construct or call the class. It reads `weakly_exposed` unless a
  related assertion carries a literal that the old text matches and the new
  text does not (#15294).

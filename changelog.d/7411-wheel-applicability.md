<!-- section: Changed -->
- Python wheel qualification skips its wheel build and pip/uv consumers for
  unrelated workflow-allowance records when an unambiguous comparison shows the
  wheel-owned record is unchanged, while workflow policy still runs independently
  (#7411).

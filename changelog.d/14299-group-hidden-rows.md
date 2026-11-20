<!-- section: Changed -->
- `ripr check` human output: omitted findings that share file, function,
  class and reason print as one `Hidden:` row naming the line span, count and
  kinds, so a diff that changes one function lists a few rows instead of one
  per changed line, and the 20-row window counts rows (#14299).

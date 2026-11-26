<!-- section: Changed -->
- `ripr check` on a file with many functions no longer rereads the file for
  every finding's owner lookup; it reads the line offsets once per lookup, and
  the output is unchanged (#15621).
